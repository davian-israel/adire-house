//! Composition root: wires each bounded context's repository to its PocketBase adapter.

use crate::catalogue::repo::{CatalogueRepo, PbCatalogueRepo};
use crate::customers::{FavouritesRepo, PbFavouritesRepo};
use crate::infra::pocketbase::PocketBase;
use crate::inventory::repo::{InventoryRepo, PbInventoryRepo};
use crate::orders::payments::{DevGateway, PaymentGateway};
use crate::orders::repo::{OrderRepo, PbOrderRepo};
use crate::pricing::repo::{PbPricingRepo, PricingRepo};
use crate::vendors::repo::{PbVendorRepo, VendorRepo};
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pb: Arc<PocketBase>,
    pub catalogue: Arc<dyn CatalogueRepo>,
    pub vendors: Arc<dyn VendorRepo>,
    pub pricing: Arc<dyn PricingRepo>,
    pub inventory: Arc<dyn InventoryRepo>,
    pub orders: Arc<dyn OrderRepo>,
    pub favourites: Arc<dyn FavouritesRepo>,
    pub payments: Arc<dyn PaymentGateway>,
    pub cookie_key: Key,
    pub secure_cookies: bool,
}

impl AppState {
    pub fn new(pb: PocketBase, cookie_key: Key, secure_cookies: bool) -> Self {
        let pb = Arc::new(pb);
        AppState {
            catalogue: Arc::new(PbCatalogueRepo(pb.clone())),
            vendors: Arc::new(PbVendorRepo(pb.clone())),
            pricing: Arc::new(PbPricingRepo(pb.clone())),
            inventory: Arc::new(PbInventoryRepo(pb.clone())),
            orders: Arc::new(PbOrderRepo(pb.clone())),
            favourites: Arc::new(PbFavouritesRepo(pb.clone())),
            payments: Arc::new(DevGateway),
            pb,
            cookie_key,
            secure_cookies,
        }
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(s: &AppState) -> Key {
        s.cookie_key.clone()
    }
}
