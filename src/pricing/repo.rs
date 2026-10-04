use super::domain::{PricingPolicy, RateTable};
use crate::infra::pocketbase::{ListQuery, PbResult, PocketBase};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait PricingRepo: Send + Sync {
    async fn current(&self) -> PbResult<PricingPolicy>;
    async fn save(&self, policy: &PricingPolicy) -> PbResult<()>;
}

#[derive(Serialize, Deserialize)]
struct Row {
    #[serde(default, skip_serializing)]
    id: String,
    markup_bps: u32,
    rates: RateTable,
    fx: HashMap<String, f64>,
}

pub struct PbPricingRepo(pub Arc<PocketBase>);

#[async_trait]
impl PricingRepo for PbPricingRepo {
    async fn current(&self) -> PbResult<PricingPolicy> {
        let row: Row = self
            .0
            .first("pricing_policy", &ListQuery::default().sort("created"))
            .await?
            .ok_or(crate::infra::pocketbase::PbError::NotFound)?;
        Ok(PricingPolicy { id: row.id, markup_bps: row.markup_bps, rates: row.rates, fx: row.fx })
    }

    async fn save(&self, p: &PricingPolicy) -> PbResult<()> {
        let row = Row { id: String::new(), markup_bps: p.markup_bps, rates: p.rates, fx: p.fx.clone() };
        let _: serde_json::Value = self.0.update("pricing_policy", &p.id, &row).await?;
        Ok(())
    }
}
