use super::domain::{Buyer, Order, OrderStatus};
use crate::infra::pocketbase::{ListQuery, PbResult, PocketBase};
use crate::shared::{Region, UsdCents};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

#[async_trait]
pub trait OrderRepo: Send + Sync {
    async fn get(&self, id: &str) -> PbResult<Order>;
    async fn recent(&self, limit: u32) -> PbResult<Vec<Order>>;
    async fn create(&self, o: &Order) -> PbResult<Order>;
    async fn save(&self, o: &Order) -> PbResult<()>;
}

#[derive(Deserialize)]
struct Row {
    id: String,
    number: String,
    artwork: String,
    vendor: String,
    buyer_name: String,
    buyer_email: String,
    shipping_address: String,
    destination: Region,
    vendor_price_cents: i64,
    markup_cents: i64,
    shipping_cents: i64,
    lane: String,
    fx_rate: f64,
    status: OrderStatus,
    #[serde(default)]
    payment_ref: String,
    #[serde(default)]
    tracking: String,
    #[serde(default)]
    created: String,
}

impl From<Row> for Order {
    fn from(r: Row) -> Self {
        Order {
            id: r.id,
            number: r.number,
            artwork_id: r.artwork,
            vendor_id: r.vendor,
            buyer: Buyer { name: r.buyer_name, email: r.buyer_email, address: r.shipping_address },
            destination: r.destination,
            vendor_price: UsdCents(r.vendor_price_cents),
            markup: UsdCents(r.markup_cents),
            shipping: UsdCents(r.shipping_cents),
            lane: r.lane,
            fx_rate: r.fx_rate,
            status: r.status,
            payment_ref: r.payment_ref,
            tracking: r.tracking,
            created: r.created.get(..10).unwrap_or_default().to_string(),
        }
    }
}

pub struct PbOrderRepo(pub Arc<PocketBase>);

#[async_trait]
impl OrderRepo for PbOrderRepo {
    async fn get(&self, id: &str) -> PbResult<Order> {
        let r: Row = self.0.get("orders", id, None).await?;
        Ok(r.into())
    }
    async fn recent(&self, limit: u32) -> PbResult<Vec<Order>> {
        let page = self
            .0
            .list::<Row>("orders", &ListQuery { per_page: Some(limit), ..ListQuery::default().sort("-created") })
            .await?;
        Ok(page.items.into_iter().map(Into::into).collect())
    }
    async fn create(&self, o: &Order) -> PbResult<Order> {
        let r: Row = self
            .0
            .create(
                "orders",
                &serde_json::json!({
                    "number": o.number, "artwork": o.artwork_id, "vendor": o.vendor_id,
                    "buyer_name": o.buyer.name, "buyer_email": o.buyer.email, "shipping_address": o.buyer.address,
                    "destination": o.destination, "vendor_price_cents": o.vendor_price.0, "markup_cents": o.markup.0,
                    "shipping_cents": o.shipping.0, "total_cents": o.total().0, "lane": o.lane,
                    "currency": o.destination.currency().code(), "fx_rate": o.fx_rate, "status": o.status,
                    "payment_ref": o.payment_ref, "tracking": o.tracking,
                }),
            )
            .await?;
        Ok(r.into())
    }
    async fn save(&self, o: &Order) -> PbResult<()> {
        let _: serde_json::Value = self
            .0
            .update(
                "orders",
                &o.id,
                &serde_json::json!({"status": o.status, "payment_ref": o.payment_ref, "tracking": o.tracking}),
            )
            .await?;
        Ok(())
    }
}
