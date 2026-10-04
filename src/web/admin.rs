use super::session::{Admin, Visitor};
use super::views::{usd, Chrome, OrderRow};
use super::{redirect, render};
use crate::error::AppError;
use crate::orders::app::act_on_order;
use crate::orders::domain::{OrderAction, OrderStatus};
use crate::pricing::domain::{PricingPolicy, ShippingLane};
use crate::shared::{Currency, DomainError, SizeClass};
use crate::state::AppState;
use crate::vendors::domain::{NewVendor, Vendor, VendorKind, VendorStatus};
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::HashMap;

pub struct VendorRow {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub place: String,
    pub works: usize,
    pub status: &'static str,
    pub tone: &'static str,
    pub can_approve: bool,
}

fn vendor_row(v: &Vendor, works: usize) -> VendorRow {
    VendorRow {
        id: v.id.clone(),
        name: v.name.clone(),
        kind: if v.kind.is_shop() { "Shop" } else { "Artist" },
        place: format!("{}, {}", v.city, v.country),
        works,
        status: v.status.label(),
        tone: match v.status {
            VendorStatus::Verified => "ok",
            VendorStatus::Pending => "warn",
            VendorStatus::Suspended => "bad",
        },
        can_approve: !v.is_live(),
    }
}

pub struct RateRow {
    pub lane: &'static str,
    pub delivery: &'static str,
    pub cells: Vec<(String, String)>,
}

#[derive(Template)]
#[template(path = "admin/_pricing.html")]
pub struct PricingPanel {
    markup_pct: String,
    rates: Vec<RateRow>,
    fx: Vec<(&'static str, String)>,
    saved: bool,
}

fn pricing_panel(p: &PricingPolicy, saved: bool) -> PricingPanel {
    PricingPanel {
        markup_pct: format!("{}", p.markup_percent()),
        rates: ShippingLane::ALL
            .iter()
            .map(|l| RateRow {
                lane: l.label(),
                delivery: l.delivery(),
                cells: SizeClass::ALL
                    .iter()
                    .map(|s| (format!("rate_{}_{}", l.code(), s.code()), format!("{:.2}", p.rates.lane(*l).get(*s).0 as f64 / 100.0)))
                    .collect(),
            })
            .collect(),
        fx: [Currency::CAD, Currency::EUR, Currency::GBP]
            .iter()
            .map(|c| (c.code(), format!("{}", p.fx.get(c.code()).copied().unwrap_or(1.0))))
            .collect(),
        saved,
    }
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct Dashboard {
    chrome: Chrome,
    verified: usize,
    pending: usize,
    live_works: usize,
    held: String,
    margin: String,
    vendors: Vec<VendorRow>,
    orders: Vec<OrderRow>,
    pricing: PricingPanel,
}

pub async fn dashboard(State(st): State<AppState>, v: Visitor, _a: Admin) -> Result<Response, AppError> {
    let vendors = st.vendors.all().await?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut live = 0;
    for vd in &vendors {
        let works = st.catalogue.by_vendor(&vd.id).await?;
        counts.insert(vd.id.clone(), works.len());
        if vd.is_live() {
            live += works.iter().filter(|a| a.is_listed()).count();
        }
    }
    let orders = st.orders.recent(50).await?;
    let held: i64 = orders.iter().filter(|o| matches!(o.status, OrderStatus::PaidHeld | OrderStatus::Shipped)).map(|o| o.total().0).sum();
    let margin: i64 = orders.iter().filter(|o| o.status == OrderStatus::DeliveredReleased).map(|o| o.markup.0).sum();
    let mut rows = Vec::new();
    for o in &orders {
        let title = st.catalogue.get(&o.artwork_id).await.map(|(a, _)| a.title).unwrap_or_else(|_| "(removed)".into());
        rows.push(OrderRow::new(o, title));
    }
    let policy = st.pricing.current().await?;
    Ok(render(Dashboard {
        chrome: Chrome::new(&v, 0, "admin"),
        verified: vendors.iter().filter(|x| x.is_live()).count(),
        pending: vendors.iter().filter(|x| x.status == VendorStatus::Pending).count(),
        live_works: live,
        held: usd(held),
        margin: usd(margin),
        vendors: vendors.iter().map(|x| vendor_row(x, counts.get(&x.id).copied().unwrap_or(0))).collect(),
        orders: rows,
        pricing: pricing_panel(&policy, false),
    })?
    .into_response())
}

#[derive(Deserialize)]
pub struct OnboardForm {
    name: String,
    kind: String,
    country: String,
    city: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    login_email: String,
    #[serde(default)]
    login_password: String,
}

pub async fn onboard(State(st): State<AppState>, _a: Admin, headers: HeaderMap, Form(f): Form<OnboardForm>) -> Result<Response, AppError> {
    let kind = if f.kind == "shop" { VendorKind::Shop } else { VendorKind::Artist };
    let email = f.login_email.trim().to_string();
    if !email.is_empty() && f.login_password.len() < 10 {
        return Err(DomainError::invalid("Give the vendor a temporary password of at least 10 characters.").into());
    }
    let new = NewVendor { name: f.name, kind, country: f.country, city: f.city, bio: f.bio }.validate()?;
    let vendor = st.vendors.create(new).await?;
    if !email.is_empty() {
        st.vendors.create_login(&vendor.id, &vendor.name, &email, &f.login_password).await?;
    }
    Ok(redirect(&headers, "/admin"))
}

#[derive(Template)]
#[template(path = "admin/_vendor_row.html")]
struct VendorRowPartial {
    v: VendorRow,
}

pub async fn vendor_action(State(st): State<AppState>, a: Admin, Path((id, action)): Path<(String, String)>) -> Result<Response, AppError> {
    let mut v = st.vendors.get(&id).await?;
    tracing::info!(admin = %a.0.email, vendor = %v.name, %action, "vendor status change");
    match action.as_str() {
        "approve" => v.approve()?,
        "suspend" => v.suspend()?,
        _ => return Err(AppError::NotFound),
    }
    st.vendors.save_status(&v).await?;
    let works = st.catalogue.by_vendor(&v.id).await?.len();
    Ok(render(VendorRowPartial { v: vendor_row(&v, works) })?.into_response())
}

pub async fn save_pricing(State(st): State<AppState>, _a: Admin, Form(f): Form<HashMap<String, String>>) -> Result<Response, AppError> {
    let mut p = st.pricing.current().await?;
    let num = |k: &str| -> Result<f64, AppError> {
        f.get(k)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .ok_or_else(|| AppError::BadRequest(format!("Enter a number for {}.", k.replace('_', " "))))
    };
    p.set_markup_percent(num("markup")?)?;
    for lane in ShippingLane::ALL {
        for size in SizeClass::ALL {
            p.set_rate(lane, size, num(&format!("rate_{}_{}", lane.code(), size.code()))?)?;
        }
    }
    for c in [Currency::CAD, Currency::EUR, Currency::GBP] {
        p.set_fx(c, num(&format!("fx_{}", c.code()))?)?;
    }
    st.pricing.save(&p).await?;
    Ok(render(pricing_panel(&p, true))?.into_response())
}

#[derive(Template)]
#[template(path = "admin/_order_row.html")]
struct OrderRowPartial {
    o: OrderRow,
}

#[derive(Deserialize)]
pub struct OrderActionForm {
    #[serde(default)]
    tracking: String,
}

pub async fn order_action(State(st): State<AppState>, a: Admin, Path((id, action)): Path<(String, String)>, Form(f): Form<OrderActionForm>) -> Result<Response, AppError> {
    let action = OrderAction::parse(&action).ok_or(AppError::NotFound)?;
    tracing::info!(admin = %a.0.email, order = %id, action = action.code(), "order action");
    if action == OrderAction::Ship && f.tracking.trim().is_empty() {
        return Err(DomainError::invalid("Add the courier tracking number before marking the order shipped.").into());
    }
    let o = act_on_order(&st, &id, action, &f.tracking).await?;
    let title = st.catalogue.get(&o.artwork_id).await.map(|(a, _)| a.title).unwrap_or_default();
    Ok(render(OrderRowPartial { o: OrderRow::new(&o, title) })?.into_response())
}
