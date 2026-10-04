use super::session::{Visitor, REGION};
use super::views::{Card, Chrome, FavButton, Opt, Thumb};
use super::{back, hx_target_is, is_htmx, redirect, render};
use crate::catalogue::domain::{ArtworkSearch, Medium};
use crate::customers::{FavouriteKind, Favourites};
use crate::error::AppError;
use crate::pricing::domain::ShippingLane;
use crate::shared::Region;
use crate::state::AppState;
use crate::storefront::{self, Browse, Listing, Sort};
use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;
use std::collections::BTreeMap;

// ---------- browse ----------

#[derive(Deserialize, Default, Clone)]
pub struct BrowseQuery {
    q: Option<String>,
    medium: Option<String>,
    country: Option<String>,
    vendor: Option<String>,
    artist: Option<String>,
    fast: Option<String>,
    max: Option<String>,
    sort: Option<String>,
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl BrowseQuery {
    fn to_browse(&self) -> Browse {
        Browse {
            search: ArtworkSearch {
                text: non_empty(&self.q),
                medium: self.medium.as_deref().and_then(Medium::parse),
                country: non_empty(&self.country),
                vendor_id: non_empty(&self.vendor),
                artist: non_empty(&self.artist),
            },
            max_total: self.max.as_deref().and_then(|m| m.parse().ok()),
            fast_only: self.fast.as_deref() == Some("1"),
            sort: Sort::parse(self.sort.as_deref().unwrap_or("")),
        }
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        [
            ("q", &self.q),
            ("medium", &self.medium),
            ("country", &self.country),
            ("vendor", &self.vendor),
            ("artist", &self.artist),
            ("fast", &self.fast),
            ("max", &self.max),
            ("sort", &self.sort),
        ]
        .into_iter()
        .filter_map(|(k, v)| non_empty(v).map(|v| (k, v)))
        .collect()
    }

    /// URL for the current filters minus one of them.
    fn without(&self, key: &str) -> String {
        let qs: Vec<String> = self
            .pairs()
            .into_iter()
            .filter(|(k, _)| *k != key)
            .map(|(k, v)| format!("{k}={}", urlencode(&v)))
            .collect();
        if qs.is_empty() { "/".into() } else { format!("/?{}", qs.join("&")) }
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub struct Chip {
    pub label: String,
    pub href: String,
}

pub struct Filters {
    pub q: String,
    pub fast: bool,
    pub media: Vec<Opt>,
    pub countries: Vec<Opt>,
    pub vendors: Vec<Opt>,
    pub artists: Vec<Opt>,
    pub maxes: Vec<Opt>,
    pub sorts: Vec<Opt>,
    pub currency: &'static str,
    pub region_label: &'static str,
}

#[derive(Template)]
#[template(path = "storefront/browse.html")]
struct BrowsePage {
    chrome: Chrome,
    f: Filters,
    cards: Vec<Card>,
    chips: Vec<Chip>,
}

#[derive(Template)]
#[template(path = "storefront/_results.html")]
struct ResultsPartial {
    cards: Vec<Card>,
    chips: Vec<Chip>,
}

fn chips(q: &BrowseQuery, all: &[Listing], region: Region) -> Vec<Chip> {
    let mut out = Vec::new();
    let mut add = |key: &str, label: String| out.push(Chip { label, href: q.without(key) });
    if let Some(t) = non_empty(&q.q) {
        add("q", format!("“{t}”"));
    }
    if let Some(v) = non_empty(&q.vendor) {
        let name = all.iter().find(|l| l.vendor.id == v).map(|l| l.vendor.name.clone()).unwrap_or(v);
        add("vendor", name);
    }
    if let Some(a) = non_empty(&q.artist) {
        add("artist", a);
    }
    if let Some(m) = q.medium.as_deref().and_then(Medium::parse) {
        add("medium", m.label().into());
    }
    if let Some(c) = non_empty(&q.country) {
        add("country", c);
    }
    if q.fast.as_deref() == Some("1") {
        add("fast", "Fast delivery".into());
    }
    if let Some(m) = q.max.as_deref().and_then(|m| m.parse::<i64>().ok()) {
        add("max", format!("Under {}", region.currency().format(m * 100, false)));
    }
    out
}

pub async fn browse(State(st): State<AppState>, v: Visitor, headers: HeaderMap, Query(q): Query<BrowseQuery>) -> Result<Response, AppError> {
    let b = q.to_browse();
    let (listings, _) = storefront::listings(&st, v.region, &b).await?;
    let cards: Vec<Card> = listings.iter().map(Card::from).collect();

    // Facets come from the unfiltered catalogue so options don't vanish as you filter.
    let (all, _) = storefront::listings(&st, v.region, &Browse::default()).await?;
    let chips = chips(&q, &all, v.region);

    if hx_target_is(&headers, "results") {
        return Ok(render(ResultsPartial { cards, chips })?.into_response());
    }

    let mut countries: Vec<String> = all.iter().map(|l| l.vendor.country.clone()).collect();
    countries.sort();
    countries.dedup();
    let vendors: BTreeMap<String, String> = all.iter().map(|l| (l.vendor.name.clone(), l.vendor.id.clone())).collect();
    let mut artists: Vec<String> = all.iter().map(|l| l.artwork.artist.clone()).collect();
    artists.sort();
    artists.dedup();

    let cur = |s: &Option<String>| s.clone().unwrap_or_default();
    let with_any = |any: &str, current: &str, mut opts: Vec<Opt>| {
        opts.insert(0, Opt::new("", any, current));
        opts
    };
    let currency = v.region.currency();
    let f = Filters {
        q: cur(&q.q),
        fast: b.fast_only,
        media: with_any("All media", &cur(&q.medium), Medium::ALL.iter().map(|m| Opt::new(m.code(), m.label(), &cur(&q.medium))).collect()),
        countries: with_any("All countries", &cur(&q.country), countries.iter().map(|c| Opt::new(c, c, &cur(&q.country))).collect()),
        vendors: with_any("All shops", &cur(&q.vendor), vendors.iter().map(|(n, id)| Opt::new(id, n, &cur(&q.vendor))).collect()),
        artists: with_any("All artists", &cur(&q.artist), artists.iter().map(|a| Opt::new(a, a, &cur(&q.artist))).collect()),
        maxes: with_any(
            "Any price",
            &cur(&q.max),
            [250, 500, 750, 1000, 1500, 2500]
                .iter()
                .map(|n| Opt::new(n.to_string(), format!("Under {}", currency.format(n * 100, false)), &cur(&q.max)))
                .collect(),
        ),
        sorts: [("featured", "Fastest delivery first"), ("new", "Newest"), ("low", "Price: low to high"), ("high", "Price: high to low")]
            .iter()
            .map(|(k, l)| Opt::new(*k, *l, q.sort.as_deref().unwrap_or("featured")))
            .collect(),
        currency: currency.code(),
        region_label: v.region.label(),
    };
    let favs = st.favourites.load(&v.customer_key).await?;
    Ok(render(BrowsePage { chrome: Chrome::new(&v, favs.count(), "art"), f, cards, chips })?.into_response())
}

// ---------- detail ----------

pub struct Detail {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: String,
    pub materials: String,
    pub dimensions: String,
    pub vendor_name: String,
    pub vendor_city: String,
    pub vendor_kind: &'static str,
    pub ships_from: String,
    pub delivery: &'static str,
    pub lane_label: &'static str,
    pub fast: bool,
    pub region_label: &'static str,
    pub price: String,
    pub shipping: String,
    pub total: String,
    pub duty_note: String,
    pub img: Option<String>,
    pub ph: &'static str,
    pub hue: u32,
    pub fav_artist: FavButton,
    pub fav_vendor: FavButton,
}

fn detail_view(l: &Listing, region: Region, favs: &Favourites) -> Detail {
    use crate::inventory::domain::StockLocation as L;
    let duty_note = match (region, l.stock.location) {
        (Region::EU, _) => "Import VAT and any duty are added at checkout.".to_string(),
        (_, L::Origin) => "Import duty, if any, is added at checkout.".to_string(),
        _ if l.quote.lane == ShippingLane::Local => "No import charges: this piece is already in your country.".to_string(),
        _ => "Import duty, if any, is added at checkout.".to_string(),
    };
    Detail {
        id: l.artwork.id.clone(),
        title: l.artwork.title.clone(),
        artist: l.artwork.artist.clone(),
        year: l.artwork.year.map(|y| y.to_string()).unwrap_or_default(),
        materials: l.artwork.materials.clone(),
        dimensions: l.artwork.dimensions.clone(),
        vendor_name: l.vendor.name.clone(),
        vendor_city: format!("{}, {}", l.vendor.city, l.vendor.country),
        vendor_kind: if l.vendor.kind.is_shop() { "shop" } else { "artist" },
        ships_from: match l.stock.location {
            L::Origin => format!("Artist's studio, {}", l.vendor.country),
            other => other.label().to_string(),
        },
        delivery: l.quote.lane.delivery(),
        lane_label: l.quote.lane.label(),
        fast: l.quote.lane.is_fast(),
        region_label: region.label(),
        price: l.quote.show_artwork_exact(),
        shipping: l.quote.show_shipping_exact(),
        total: l.quote.show_total_exact(),
        duty_note,
        img: super::views::image_url(&l.artwork.id, &l.artwork.image, "1200x0"),
        ph: l.artwork.placeholder_class(),
        hue: l.artwork.placeholder_hue(),
        fav_artist: fav_button(FavouriteKind::Artist, &l.artwork.artist, favs.has(FavouriteKind::Artist, &l.artwork.artist)),
        fav_vendor: fav_button(FavouriteKind::Vendor, &l.vendor.id, favs.has(FavouriteKind::Vendor, &l.vendor.id)),
    }
}

#[derive(Template)]
#[template(path = "storefront/detail.html")]
struct DetailPage {
    chrome: Chrome,
    d: Detail,
}

#[derive(Template)]
#[template(path = "storefront/_detail_modal.html")]
struct DetailModal {
    d: Detail,
}

pub async fn detail(State(st): State<AppState>, v: Visitor, headers: HeaderMap, Path(id): Path<String>) -> Result<Response, AppError> {
    let l = storefront::listing(&st, v.region, &id).await?;
    let favs = st.favourites.load(&v.customer_key).await?;
    let d = detail_view(&l, v.region, &favs);
    if hx_target_is(&headers, "modal") {
        return Ok(render(DetailModal { d })?.into_response());
    }
    Ok(render(DetailPage { chrome: Chrome::new(&v, favs.count(), "art"), d })?.into_response())
}

// ---------- shops & artists ----------

pub struct VendorCard {
    pub id: String,
    pub name: String,
    pub kind_label: &'static str,
    pub is_shop: bool,
    pub place: String,
    pub bio: String,
    pub works: usize,
    pub artists: usize,
    pub thumbs: Vec<Thumb>,
    pub fav: FavButton,
}

pub struct ArtistCard {
    pub name: String,
    pub country: String,
    pub via: String,
    pub works: usize,
    pub thumbs: Vec<Thumb>,
    pub href: String,
    pub fav: FavButton,
}

fn fav_button(kind: FavouriteKind, target: &str, saved: bool) -> FavButton {
    let (on, off) = match kind {
        FavouriteKind::Vendor => ("Shop saved", "Save shop"),
        FavouriteKind::Artist => ("Artist saved", "Save artist"),
    };
    FavButton { kind: kind.code(), target: target.into(), saved, label_on: on, label_off: off }
}

fn vendor_cards(all: &[Listing], favs: &Favourites, only_saved: bool) -> Vec<VendorCard> {
    let mut by: BTreeMap<String, Vec<&Listing>> = BTreeMap::new();
    for l in all {
        by.entry(l.vendor.name.clone()).or_default().push(l);
    }
    by.into_values()
        .filter(|ls| !only_saved || favs.has(FavouriteKind::Vendor, &ls[0].vendor.id))
        .map(|ls| {
            let v = &ls[0].vendor;
            let mut names: Vec<&str> = ls.iter().map(|l| l.artwork.artist.as_str()).collect();
            names.sort();
            names.dedup();
            VendorCard {
                id: v.id.clone(),
                name: v.name.clone(),
                kind_label: v.kind.label(),
                is_shop: v.kind.is_shop(),
                place: format!("{}, {}", v.city, v.country),
                bio: v.bio.clone(),
                works: ls.len(),
                artists: names.len(),
                thumbs: ls.iter().take(3).map(|l| Thumb::from(*l)).collect(),
                fav: fav_button(FavouriteKind::Vendor, &v.id, favs.has(FavouriteKind::Vendor, &v.id)),
            }
        })
        .collect()
}

fn artist_cards(all: &[Listing], favs: &Favourites, only_saved: bool) -> Vec<ArtistCard> {
    let mut by: BTreeMap<String, Vec<&Listing>> = BTreeMap::new();
    for l in all {
        by.entry(l.artwork.artist.clone()).or_default().push(l);
    }
    by.into_iter()
        .filter(|(n, _)| !only_saved || favs.has(FavouriteKind::Artist, n))
        .map(|(name, ls)| {
            let mut via: Vec<&str> = ls.iter().map(|l| l.vendor.name.as_str()).collect();
            via.sort();
            via.dedup();
            ArtistCard {
                href: format!("/?artist={}", urlencode(&name)),
                fav: fav_button(FavouriteKind::Artist, &name, favs.has(FavouriteKind::Artist, &name)),
                country: ls[0].vendor.country.clone(),
                via: via.join(", "),
                works: ls.len(),
                thumbs: ls.iter().take(3).map(|l| Thumb::from(*l)).collect(),
                name,
            }
        })
        .collect()
}

#[derive(Template)]
#[template(path = "storefront/shops.html")]
struct ShopsPage {
    chrome: Chrome,
    vendors: Vec<VendorCard>,
}

#[derive(Template)]
#[template(path = "storefront/artists.html")]
struct ArtistsPage {
    chrome: Chrome,
    artists: Vec<ArtistCard>,
}

#[derive(Template)]
#[template(path = "storefront/saved.html")]
struct SavedPage {
    chrome: Chrome,
    vendors: Vec<VendorCard>,
    artists: Vec<ArtistCard>,
    cards: Vec<Card>,
}

pub async fn shops(State(st): State<AppState>, v: Visitor) -> Result<Response, AppError> {
    let (all, _) = storefront::listings(&st, v.region, &Browse { sort: Sort::Newest, ..Default::default() }).await?;
    let favs = st.favourites.load(&v.customer_key).await?;
    Ok(render(ShopsPage { chrome: Chrome::new(&v, favs.count(), "shops"), vendors: vendor_cards(&all, &favs, false) })?.into_response())
}

pub async fn artists(State(st): State<AppState>, v: Visitor) -> Result<Response, AppError> {
    let (all, _) = storefront::listings(&st, v.region, &Browse { sort: Sort::Newest, ..Default::default() }).await?;
    let favs = st.favourites.load(&v.customer_key).await?;
    Ok(render(ArtistsPage { chrome: Chrome::new(&v, favs.count(), "artists"), artists: artist_cards(&all, &favs, false) })?.into_response())
}

pub async fn saved(State(st): State<AppState>, v: Visitor) -> Result<Response, AppError> {
    let (all, _) = storefront::listings(&st, v.region, &Browse { sort: Sort::Newest, ..Default::default() }).await?;
    let favs = st.favourites.load(&v.customer_key).await?;
    let cards = all
        .iter()
        .filter(|l| favs.has(FavouriteKind::Vendor, &l.vendor.id) || favs.has(FavouriteKind::Artist, &l.artwork.artist))
        .map(Card::from)
        .collect();
    Ok(render(SavedPage {
        chrome: Chrome::new(&v, favs.count(), "saved"),
        vendors: vendor_cards(&all, &favs, true),
        artists: artist_cards(&all, &favs, true),
        cards,
    })?
    .into_response())
}

// ---------- favourites ----------

#[derive(Deserialize)]
pub struct FavForm {
    kind: String,
    target: String,
}

#[derive(Template)]
#[template(path = "storefront/_fav_toggle.html")]
struct FavToggle {
    fav: FavButton,
    count: usize,
}

pub async fn toggle_favourite(State(st): State<AppState>, v: Visitor, headers: HeaderMap, Form(f): Form<FavForm>) -> Result<Response, AppError> {
    let kind = FavouriteKind::parse(&f.kind).ok_or_else(|| AppError::BadRequest("Unknown favourite type.".into()))?;
    let target = f.target.trim();
    if target.is_empty() || target.len() > 120 {
        return Err(AppError::BadRequest("Nothing to save.".into()));
    }
    let saved = st.favourites.toggle(&v.customer_key, kind, target).await?;
    if !is_htmx(&headers) {
        return Ok(redirect(&headers, &back(&headers)));
    }
    let count = st.favourites.load(&v.customer_key).await?.count();
    Ok(render(FavToggle { fav: fav_button(kind, target, saved), count })?.into_response())
}

// ---------- region ----------

#[derive(Deserialize)]
pub struct RegionForm {
    region: String,
}

pub async fn set_region(State(st): State<AppState>, headers: HeaderMap, Form(f): Form<RegionForm>) -> Result<Response, AppError> {
    let region = Region::parse(&f.region).ok_or_else(|| AppError::BadRequest("Choose a delivery region.".into()))?;
    let c = Cookie::build((REGION, region.code()))
        .path("/")
        .secure(st.secure_cookies)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(365))
        .build();
    let mut r = if is_htmx(&headers) {
        let mut r = axum::http::StatusCode::NO_CONTENT.into_response();
        r.headers_mut().insert("HX-Refresh", HeaderValue::from_static("true"));
        r
    } else {
        redirect(&headers, &back(&headers))
    };
    if let Ok(v) = HeaderValue::from_str(&c.to_string()) {
        r.headers_mut().append(header::SET_COOKIE, v);
    }
    Ok(r)
}

// ---------- media proxy ----------

#[derive(Deserialize)]
pub struct ThumbQuery {
    thumb: Option<String>,
}

/// Streams artwork photos out of the locked PocketBase collection.
pub async fn media(State(st): State<AppState>, Path((id, file)): Path<(String, String)>, Query(t): Query<ThumbQuery>) -> Result<Response, AppError> {
    // Only serve the file actually attached to a listed artwork.
    let (a, _) = st.catalogue.get(&id).await?;
    if a.image.as_deref() != Some(file.as_str()) {
        return Err(AppError::NotFound);
    }
    let thumb = t.thumb.filter(|t| ["480x600", "240x240", "1200x0", "80x100"].contains(&t.as_str()));
    let (ct, bytes) = st.pb.file("artworks", &id, &file, thumb.as_deref()).await?;
    Ok(([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "public, max-age=86400".to_string())], bytes).into_response())
}
