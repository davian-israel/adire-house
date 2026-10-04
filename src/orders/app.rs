//! Application services for Orders. These coordinate several contexts:
//! Catalogue (is it listed?), Vendors (is the seller verified?), Inventory (reserve a unit),
//! Pricing (quote) and Payments.
//!
//! PocketBase has no multi-collection transactions over REST, so each step compensates
//! on failure (e.g. stock is released if the order record cannot be written).

use super::domain::{Buyer, Order, OrderAction};
use super::payments::PaymentOutcome;
use crate::shared::{DomainError, Region};
use crate::state::AppState;
use crate::error::AppError;

pub struct PlaceOrder {
    pub artwork_id: String,
    pub region: Region,
    pub buyer: Buyer,
}

pub enum Placed {
    Paid(Order),
    Redirect(String),
}

fn order_number() -> String {
    let u = uuid::Uuid::new_v4().simple().to_string().to_uppercase();
    format!("AH-{}", &u[..8])
}

pub async fn place_order(st: &AppState, cmd: PlaceOrder) -> Result<Placed, AppError> {
    let buyer = cmd.buyer.validate()?;
    let (artwork, vendor) = st.catalogue.get(&cmd.artwork_id).await?;
    if !artwork.is_listed() || !vendor.is_live() {
        return Err(DomainError::NotAvailable.into());
    }
    let mut stock = st.inventory.for_artwork(&artwork.id).await?.ok_or(DomainError::NotAvailable)?;
    let policy = st.pricing.current().await?;
    let quote = policy.quote(artwork.vendor_price, artwork.size, stock.location, cmd.region);

    stock.reserve()?;
    st.inventory.save(&stock).await?;

    let draft = Order::place(order_number(), artwork.id.clone(), vendor.id.clone(), buyer, &quote);
    let mut order = match st.orders.create(&draft).await {
        Ok(o) => o,
        Err(e) => {
            stock.release();
            let _ = st.inventory.save(&stock).await;
            return Err(e.into());
        }
    };

    match st.payments.collect(&order).await? {
        PaymentOutcome::Captured { reference } => {
            order.mark_paid(&reference)?;
            st.orders.save(&order).await?;
            Ok(Placed::Paid(order))
        }
        PaymentOutcome::Redirect { url } => Ok(Placed::Redirect(url)),
    }
}

/// Move an order through escrow. Restocks on cancel/refund, pays the vendor on release.
pub async fn act_on_order(st: &AppState, id: &str, action: OrderAction, tracking: &str) -> Result<Order, AppError> {
    let mut order = st.orders.get(id).await?;
    order.apply(action, "", tracking)?;

    match action {
        OrderAction::Refund => st.payments.refund(&order).await?,
        OrderAction::ConfirmDelivery => st.payments.release_to_vendor(&order).await?,
        _ => {}
    }
    st.orders.save(&order).await?;

    if order.status.restocks() {
        if let Some(mut s) = st.inventory.for_artwork(&order.artwork_id).await? {
            s.release();
            st.inventory.save(&s).await?;
        }
    }
    Ok(order)
}
