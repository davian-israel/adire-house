//! Storefront read model. Joins Catalogue + Vendors + Inventory + Pricing into what a buyer sees.
//! Text, medium, country, shop and artist filters run in PocketBase; price and delivery-speed
//! filters run here because they depend on the pricing policy and the buyer's region.

use crate::catalogue::domain::{Artwork, ArtworkSearch};
use crate::error::AppError;
use crate::inventory::domain::StockItem;
use crate::pricing::domain::{PricingPolicy, Quote};
use crate::shared::Region;
use crate::state::AppState;
use crate::vendors::domain::Vendor;

#[derive(Clone, Debug)]
pub struct Listing {
    pub artwork: Artwork,
    pub vendor: Vendor,
    pub stock: StockItem,
    pub quote: Quote,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    FastestFirst,
    Newest,
    PriceLow,
    PriceHigh,
}

impl Sort {
    pub fn parse(s: &str) -> Self {
        match s {
            "new" => Sort::Newest,
            "low" => Sort::PriceLow,
            "high" => Sort::PriceHigh,
            _ => Sort::FastestFirst,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Browse {
    pub search: ArtworkSearch,
    /// Maximum total in the buyer's currency, whole units.
    pub max_total: Option<i64>,
    pub fast_only: bool,
    pub sort: Sort,
}

pub async fn listings(st: &AppState, region: Region, b: &Browse) -> Result<(Vec<Listing>, PricingPolicy), AppError> {
    let policy = st.pricing.current().await?;
    let found = st.catalogue.search(&b.search).await?;
    let ids: Vec<String> = found.iter().map(|(a, _)| a.id.clone()).collect();
    let stock = st.inventory.for_artworks(&ids).await?;

    let mut out: Vec<Listing> = found
        .into_iter()
        .filter_map(|(artwork, vendor)| {
            let stock = stock.get(&artwork.id)?.clone();
            if !stock.available() {
                return None;
            }
            let quote = policy.quote(artwork.vendor_price, artwork.size, stock.location, region);
            Some(Listing { artwork, vendor, stock, quote })
        })
        .filter(|l| !b.fast_only || l.quote.lane.is_fast())
        .filter(|l| b.max_total.is_none_or(|max| l.quote.local_total_minor() <= max * 100))
        .collect();

    match b.sort {
        // PocketBase already returned newest first; a stable sort keeps that within each group.
        Sort::FastestFirst => out.sort_by_key(|l| !l.quote.lane.is_fast()),
        Sort::Newest => {}
        Sort::PriceLow => out.sort_by_key(|l| l.quote.total()),
        Sort::PriceHigh => out.sort_by_key(|l| std::cmp::Reverse(l.quote.total())),
    }
    Ok((out, policy))
}

pub async fn listing(st: &AppState, region: Region, artwork_id: &str) -> Result<Listing, AppError> {
    let (artwork, vendor) = st.catalogue.get(artwork_id).await?;
    if !artwork.is_listed() || !vendor.is_live() {
        return Err(AppError::NotFound);
    }
    let stock = st.inventory.for_artwork(&artwork.id).await?.ok_or(AppError::NotFound)?;
    let policy = st.pricing.current().await?;
    let quote = policy.quote(artwork.vendor_price, artwork.size, stock.location, region);
    Ok(Listing { artwork, vendor, stock, quote })
}
