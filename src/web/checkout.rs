use super::session::Visitor;
use super::views::{image_url, Chrome};
use super::{redirect, render};
use crate::error::AppError;
use crate::orders::app::{act_on_order, place_order, PlaceOrder, Placed};
use crate::orders::domain::{Buyer, OrderAction, OrderStatus};
use crate::state::AppState;
use crate::storefront;
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[derive(Template)]
#[template(path = "checkout.html")]
struct CheckoutPage {
    chrome: Chrome,
    id: String,
    title: String,
    artist: String,
    vendor: String,
    img: Option<String>,
    ph: &'static str,
    hue: u32,
    price: String,
    shipping: String,
    total: String,
    delivery: &'static str,
    region_label: &'static str,
    currency: &'static str,
}

pub async fn form(State(st): State<AppState>, v: Visitor, Path(id): Path<String>) -> Result<Response, AppError> {
    let l = storefront::listing(&st, v.region, &id).await?;
    let favs = st.favourites.load(&v.customer_key).await?;
    Ok(render(CheckoutPage {
        chrome: Chrome::new(&v, favs.count(), "art"),
        id: l.artwork.id.clone(),
        title: l.artwork.title.clone(),
        artist: l.artwork.artist.clone(),
        vendor: l.vendor.name.clone(),
        img: image_url(&l.artwork.id, &l.artwork.image, "240x240"),
        ph: l.artwork.placeholder_class(),
        hue: l.artwork.placeholder_hue(),
        price: l.quote.show_artwork_exact(),
        shipping: l.quote.show_shipping_exact(),
        total: l.quote.show_total_exact(),
        delivery: l.quote.lane.delivery(),
        region_label: v.region.label(),
        currency: v.region.currency().code(),
    })?
    .into_response())
}

#[derive(Deserialize)]
pub struct CheckoutForm {
    name: String,
    email: String,
    address: String,
}

pub async fn place(State(st): State<AppState>, v: Visitor, headers: HeaderMap, Path(id): Path<String>, Form(f): Form<CheckoutForm>) -> Result<Response, AppError> {
    let placed = place_order(
        &st,
        PlaceOrder { artwork_id: id, region: v.region, buyer: Buyer { name: f.name, email: f.email, address: f.address } },
    )
    .await?;
    Ok(match placed {
        Placed::Paid(order) => redirect(&headers, &format!("/orders/{}", order.id)),
        Placed::Redirect(url) => redirect(&headers, &url),
    })
}

pub struct Step {
    pub label: &'static str,
    pub done: bool,
}

#[derive(Template)]
#[template(path = "order.html")]
struct OrderPage {
    chrome: Chrome,
    id: String,
    number: String,
    title: String,
    status: &'static str,
    tone: &'static str,
    total: String,
    tracking: String,
    steps: Vec<Step>,
    can_confirm: bool,
    closed: bool,
}

pub async fn order_page(State(st): State<AppState>, v: Visitor, Path(id): Path<String>) -> Result<Response, AppError> {
    let o = st.orders.get(&id).await?;
    let title = st.catalogue.get(&o.artwork_id).await.map(|(a, _)| a.title).unwrap_or_default();
    let favs = st.favourites.load(&v.customer_key).await?;
    use OrderStatus as S;
    let rank = match o.status {
        S::AwaitingPayment => 0,
        S::PaidHeld => 1,
        S::Shipped => 2,
        S::DeliveredReleased => 3,
        S::Refunded | S::Cancelled => -1,
    };
    let steps = ["Payment held safely", "Shipped by the vendor", "You confirm delivery and the vendor is paid"]
        .iter()
        .enumerate()
        .map(|(i, l)| Step { label: l, done: rank > i as i32 })
        .collect();
    Ok(render(OrderPage {
        chrome: Chrome::new(&v, favs.count(), "art"),
        id: o.id.clone(),
        number: o.number.clone(),
        title,
        status: o.status.label(),
        tone: o.status.tone(),
        total: o.show_total(),
        tracking: o.tracking.clone(),
        steps,
        can_confirm: o.buyer_can_confirm(),
        closed: rank < 0,
    })?
    .into_response())
}

pub async fn confirm_delivery(State(st): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response, AppError> {
    act_on_order(&st, &id, OrderAction::ConfirmDelivery, "").await?;
    Ok(redirect(&headers, &format!("/orders/{id}")))
}
