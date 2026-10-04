use super::session::{Visitor, VendorUser};
use super::views::{image_url, usd, Chrome, Opt, RegionPrice};
use super::{redirect, render};
use crate::catalogue::domain::{Artwork, ArtworkStatus, Medium, NewArtwork};
use crate::catalogue::repo::Upload;
use crate::error::AppError;
use crate::inventory::domain::{StockItem, StockLocation};
use crate::pricing::domain::PricingPolicy;
use crate::shared::{Region, SizeClass, UsdCents};
use crate::state::AppState;
use askama::Template;
use axum::extract::{Form, Multipart, Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

pub struct ListingRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub dimensions: String,
    pub img: Option<String>,
    pub ph: &'static str,
    pub hue: u32,
    pub you_get: String,
    pub buyer_prices: Vec<(&'static str, String)>,
    pub locations: Vec<Opt>,
    pub listed: bool,
    pub sold_out: bool,
}

fn row(a: &Artwork, s: Option<&StockItem>, p: &PricingPolicy) -> ListingRow {
    let loc = s.map(|s| s.location).unwrap_or(StockLocation::Origin);
    ListingRow {
        id: a.id.clone(),
        title: a.title.clone(),
        artist: a.artist.clone(),
        dimensions: a.dimensions.clone(),
        img: image_url(&a.id, &a.image, "80x100"),
        ph: a.placeholder_class(),
        hue: a.placeholder_hue(),
        you_get: usd(a.vendor_price.0),
        buyer_prices: Region::ALL.iter().map(|r| (r.code(), p.quote(a.vendor_price, a.size, loc, *r).show_total())).collect(),
        locations: StockLocation::ALL.iter().map(|l| Opt::new(l.code(), l.label(), loc.code())).collect(),
        listed: a.is_listed(),
        sold_out: s.is_some_and(|s| !s.available()),
    }
}

#[derive(Template)]
#[template(path = "vendor/dashboard.html")]
struct Dashboard {
    chrome: Chrome,
    vendor_name: String,
    is_shop: bool,
    verified: bool,
    status_label: &'static str,
    markup_pct: String,
    rows: Vec<ListingRow>,
    media: Vec<Opt>,
    sizes: Vec<Opt>,
    locations: Vec<Opt>,
    preview: QuotePreview,
}

#[derive(Template)]
#[template(path = "vendor/_row.html")]
struct RowPartial {
    r: ListingRow,
}

#[derive(Template)]
#[template(path = "vendor/_quote.html")]
pub struct QuotePreview {
    prices: Vec<RegionPrice>,
    you_get: String,
    margin: String,
}

fn preview(p: &PricingPolicy, price: UsdCents, size: SizeClass, loc: StockLocation) -> QuotePreview {
    QuotePreview {
        prices: Region::ALL.iter().map(|r| RegionPrice::from(&p.quote(price, size, loc, *r))).collect(),
        you_get: usd(price.0),
        margin: usd(price.bps(p.markup_bps).0),
    }
}

pub async fn dashboard(State(st): State<AppState>, v: Visitor, me: VendorUser) -> Result<Response, AppError> {
    let vendor = st.vendors.get(&me.vendor_id).await?;
    let works = st.catalogue.by_vendor(&vendor.id).await?;
    let ids: Vec<String> = works.iter().map(|a| a.id.clone()).collect();
    let stock = st.inventory.for_artworks(&ids).await?;
    let policy = st.pricing.current().await?;
    Ok(render(Dashboard {
        chrome: Chrome::new(&v, 0, "vendor"),
        is_shop: vendor.kind.is_shop(),
        verified: vendor.is_live(),
        status_label: vendor.status.label(),
        markup_pct: format!("{}", policy.markup_percent()),
        rows: works.iter().map(|a| row(a, stock.get(&a.id), &policy)).collect(),
        media: Medium::ALL.iter().map(|m| Opt::new(m.code(), m.label(), "painting")).collect(),
        sizes: SizeClass::ALL.iter().map(|s| Opt::new(s.code(), s.label(), "M")).collect(),
        locations: StockLocation::ALL.iter().map(|l| Opt::new(l.code(), l.label(), "ORIGIN")).collect(),
        preview: preview(&policy, UsdCents::from_dollars(300), SizeClass::M, StockLocation::Origin),
        vendor_name: vendor.name,
    })?
    .into_response())
}

fn parse_price(s: &str) -> Option<UsdCents> {
    let d: f64 = s.trim().trim_start_matches('$').replace(',', "").parse().ok()?;
    (d.is_finite() && d >= 0.0).then(|| UsdCents((d * 100.0).round() as i64))
}

#[derive(Deserialize)]
pub struct QuoteForm {
    #[serde(default)]
    price: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    stock: String,
}

pub async fn quote_preview(State(st): State<AppState>, _me: VendorUser, Form(f): Form<QuoteForm>) -> Result<Response, AppError> {
    let policy = st.pricing.current().await?;
    let price = parse_price(&f.price).unwrap_or_default();
    let size = SizeClass::parse(&f.size).unwrap_or(SizeClass::M);
    let loc = StockLocation::parse(&f.stock).unwrap_or(StockLocation::Origin);
    Ok(render(preview(&policy, price, size, loc))?.into_response())
}

pub async fn create_artwork(State(st): State<AppState>, me: VendorUser, headers: HeaderMap, mut mp: Multipart) -> Result<Response, AppError> {
    let vendor = st.vendors.get(&me.vendor_id).await?;
    let mut fields = std::collections::HashMap::<String, String>::new();
    let mut image: Option<Upload> = None;
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::BadRequest(format!("Upload failed: {e}")))? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "image" {
            let filename = field.file_name().unwrap_or("photo.jpg").to_string();
            let ct = field.content_type().unwrap_or("").to_string();
            let bytes = field.bytes().await.map_err(|e| AppError::BadRequest(format!("Upload failed: {e}")))?;
            if bytes.is_empty() {
                continue;
            }
            if !["image/jpeg", "image/png", "image/webp"].contains(&ct.as_str()) {
                return Err(AppError::BadRequest("Photos must be JPEG, PNG or WebP.".into()));
            }
            if bytes.len() > 8 * 1024 * 1024 {
                return Err(AppError::BadRequest("Photos must be 8 MB or smaller.".into()));
            }
            image = Some(Upload { filename, content_type: ct, bytes });
        } else {
            fields.insert(name, field.text().await.unwrap_or_default());
        }
    }
    let get = |k: &str| fields.get(k).cloned().unwrap_or_default();
    let new = NewArtwork {
        vendor_id: vendor.id.clone(),
        // An individual artist always lists their own work.
        artist: if vendor.kind.is_shop() { get("artist") } else { vendor.name.clone() },
        title: get("title"),
        medium: Medium::parse(&get("medium")).ok_or_else(|| AppError::BadRequest("Choose a medium.".into()))?,
        materials: get("materials"),
        dimensions: get("dimensions"),
        size: SizeClass::parse(&get("size")).ok_or_else(|| AppError::BadRequest("Choose a shipping size.".into()))?,
        vendor_price: parse_price(&get("price")).ok_or_else(|| AppError::BadRequest("Enter your price in US dollars.".into()))?,
        year: get("year").trim().parse().ok(),
    }
    .validate()?;
    let loc = StockLocation::parse(&get("stock")).unwrap_or(StockLocation::Origin);

    let artwork = st.catalogue.create(new, image).await?;
    tracing::info!(user = %me.session.email, artwork = %artwork.id, "artwork listed");
    if let Err(e) = st.inventory.create(&artwork.id, loc, 1).await {
        // Compensate: don't leave a listing without stock.
        let _ = st.catalogue.set_status(&artwork.id, ArtworkStatus::Withdrawn).await;
        return Err(e.into());
    }
    Ok(redirect(&headers, "/vendor"))
}

async fn owned(st: &AppState, me: &VendorUser, id: &str) -> Result<Artwork, AppError> {
    let (a, _) = st.catalogue.get(id).await?;
    if a.vendor_id != me.vendor_id {
        return Err(AppError::Forbidden);
    }
    Ok(a)
}

async fn row_response(st: &AppState, a: &Artwork) -> Result<Response, AppError> {
    let stock = st.inventory.for_artwork(&a.id).await?;
    let policy = st.pricing.current().await?;
    Ok(render(RowPartial { r: row(a, stock.as_ref(), &policy) })?.into_response())
}

#[derive(Deserialize)]
pub struct StockForm {
    location: String,
}

pub async fn move_stock(State(st): State<AppState>, me: VendorUser, Path(id): Path<String>, Form(f): Form<StockForm>) -> Result<Response, AppError> {
    let a = owned(&st, &me, &id).await?;
    let loc = StockLocation::parse(&f.location).ok_or_else(|| AppError::BadRequest("Choose a stock location.".into()))?;
    match st.inventory.for_artwork(&a.id).await? {
        Some(mut s) => {
            s.move_to(loc);
            st.inventory.save(&s).await?;
        }
        None => {
            st.inventory.create(&a.id, loc, 1).await?;
        }
    }
    row_response(&st, &a).await
}

pub async fn set_listing(State(st): State<AppState>, me: VendorUser, Path((id, action)): Path<(String, String)>) -> Result<Response, AppError> {
    let mut a = owned(&st, &me, &id).await?;
    match action.as_str() {
        "withdraw" => a.withdraw(),
        "relist" => a.relist(),
        _ => return Err(AppError::NotFound),
    }
    st.catalogue.set_status(&a.id, a.status).await?;
    row_response(&st, &a).await
}
