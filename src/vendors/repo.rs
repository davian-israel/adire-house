use super::domain::{NewVendor, Vendor, VendorStatus};
use crate::infra::pocketbase::{ListQuery, PbResult, PocketBase};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait VendorRepo: Send + Sync {
    async fn get(&self, id: &str) -> PbResult<Vendor>;
    async fn all(&self) -> PbResult<Vec<Vendor>>;
    async fn verified(&self) -> PbResult<Vec<Vendor>>;
    async fn create(&self, v: NewVendor) -> PbResult<Vendor>;
    async fn save_status(&self, v: &Vendor) -> PbResult<()>;
    /// Create the vendor's portal login.
    async fn create_login(&self, vendor_id: &str, name: &str, email: &str, password: &str) -> PbResult<()>;
}

pub struct PbVendorRepo(pub Arc<PocketBase>);

#[async_trait]
impl VendorRepo for PbVendorRepo {
    async fn get(&self, id: &str) -> PbResult<Vendor> {
        self.0.get("vendors", id, None).await
    }
    async fn all(&self) -> PbResult<Vec<Vendor>> {
        self.0.list_all("vendors", &ListQuery::default().sort("status,name")).await
    }
    async fn verified(&self) -> PbResult<Vec<Vendor>> {
        self.0.list_all("vendors", &ListQuery::filter("status = 'verified'").sort("name")).await
    }
    async fn create(&self, v: NewVendor) -> PbResult<Vendor> {
        self.0
            .create(
                "vendors",
                &serde_json::json!({
                    "name": v.name, "kind": v.kind, "country": v.country, "city": v.city,
                    "bio": v.bio, "status": VendorStatus::Pending,
                }),
            )
            .await
    }
    async fn save_status(&self, v: &Vendor) -> PbResult<()> {
        let _: serde_json::Value = self.0.update("vendors", &v.id, &serde_json::json!({"status": v.status})).await?;
        Ok(())
    }
    async fn create_login(&self, vendor_id: &str, name: &str, email: &str, password: &str) -> PbResult<()> {
        let _: serde_json::Value = self
            .0
            .create(
                "users",
                &serde_json::json!({
                    "email": email, "password": password, "passwordConfirm": password,
                    "name": name, "role": "vendor", "vendor": vendor_id, "verified": true,
                }),
            )
            .await?;
        Ok(())
    }
}
