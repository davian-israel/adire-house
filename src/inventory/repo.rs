use super::domain::{StockItem, StockLocation};
use crate::infra::pocketbase::{q, ListQuery, PbResult, PocketBase};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait InventoryRepo: Send + Sync {
    async fn for_artwork(&self, artwork_id: &str) -> PbResult<Option<StockItem>>;
    /// Stock for many artworks, keyed by artwork id.
    async fn for_artworks(&self, artwork_ids: &[String]) -> PbResult<HashMap<String, StockItem>>;
    async fn create(&self, artwork_id: &str, location: StockLocation, quantity: u32) -> PbResult<StockItem>;
    async fn save(&self, item: &StockItem) -> PbResult<()>;
}

#[derive(Deserialize)]
struct Row {
    id: String,
    artwork: String,
    location: StockLocation,
    #[serde(default)]
    quantity: u32,
}

impl From<Row> for StockItem {
    fn from(r: Row) -> Self {
        StockItem { id: r.id, artwork_id: r.artwork, location: r.location, quantity: r.quantity }
    }
}

pub struct PbInventoryRepo(pub Arc<PocketBase>);

#[async_trait]
impl InventoryRepo for PbInventoryRepo {
    async fn for_artwork(&self, artwork_id: &str) -> PbResult<Option<StockItem>> {
        let row: Option<Row> = self.0.first("stock", &ListQuery::filter(format!("artwork = {}", q(artwork_id)))).await?;
        Ok(row.map(Into::into))
    }

    async fn for_artworks(&self, ids: &[String]) -> PbResult<HashMap<String, StockItem>> {
        let mut out = HashMap::new();
        // Keep filter strings a sensible length.
        for chunk in ids.chunks(60) {
            if chunk.is_empty() {
                continue;
            }
            let f = chunk.iter().map(|id| format!("artwork = {}", q(id))).collect::<Vec<_>>().join(" || ");
            let rows: Vec<Row> = self.0.list_all("stock", &ListQuery::filter(f)).await?;
            out.extend(rows.into_iter().map(|r| (r.artwork.clone(), r.into())));
        }
        Ok(out)
    }

    async fn create(&self, artwork_id: &str, location: StockLocation, quantity: u32) -> PbResult<StockItem> {
        let row: Row = self
            .0
            .create("stock", &serde_json::json!({"artwork": artwork_id, "location": location, "quantity": quantity}))
            .await?;
        Ok(row.into())
    }

    async fn save(&self, s: &StockItem) -> PbResult<()> {
        let _: serde_json::Value = self
            .0
            .update("stock", &s.id, &serde_json::json!({"location": s.location, "quantity": s.quantity}))
            .await?;
        Ok(())
    }
}
