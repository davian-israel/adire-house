//! View models: flat, template-friendly structs built from domain objects.

use crate::orders::domain::{Order, OrderAction};
use crate::pricing::domain::Quote;
use crate::shared::Region;
use crate::storefront::Listing;
use crate::web::session::{Session, Visitor};

#[derive(Clone, Debug)]
pub struct Opt {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

impl Opt {
    pub fn new(value: impl Into<String>, label: impl Into<String>, current: &str) -> Self {
        let value = value.into();
        let selected = value == current;
        Opt { value, label: label.into(), selected }
    }
}

/// Header and navigation shared by every full page.
pub struct Chrome {
    pub regions: Vec<Opt>,
    pub session: Option<Session>,
    pub fav_count: usize,
    pub tab: &'static str,
}

impl Chrome {
    pub fn new(v: &Visitor, fav_count: usize, tab: &'static str) -> Self {
        Chrome {
            regions: Region::ALL
                .iter()
                .map(|r| Opt::new(r.code(), format!("{} ({})", r.label(), r.currency().code()), v.region.code()))
                .collect(),
            session: v.session.clone(),
            fav_count,
            tab,
        }
    }
    pub fn is_admin(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.is_admin())
    }
    pub fn is_vendor(&self) -> bool {
        self.session.as_ref().is_some_and(|s| !s.is_admin())
    }
    pub fn user_name(&self) -> String {
        self.session.as_ref().map(|s| s.name.clone()).unwrap_or_default()
    }
}

pub fn image_url(artwork_id: &str, file: &Option<String>, thumb: &str) -> Option<String> {
    file.as_ref().map(|f| format!("/media/{artwork_id}/{f}?thumb={thumb}"))
}

/// One artwork tile in the gallery grid.
#[derive(Clone, Debug)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub country: String,
    pub img: Option<String>,
    pub ph: &'static str,
    pub hue: u32,
    pub price: String,
    pub shipping: String,
    pub fast: Option<&'static str>,
}

impl From<&Listing> for Card {
    fn from(l: &Listing) -> Self {
        Card {
            id: l.artwork.id.clone(),
            title: l.artwork.title.clone(),
            artist: l.artwork.artist.clone(),
            country: l.vendor.country.clone(),
            img: image_url(&l.artwork.id, &l.artwork.image, "480x600"),
            ph: l.artwork.placeholder_class(),
            hue: l.artwork.placeholder_hue(),
            price: l.quote.show_artwork(),
            shipping: l.quote.show_shipping(),
            fast: l.quote.lane.is_fast().then(|| l.quote.lane.delivery()),
        }
    }
}

/// Small thumbnail used in shop and artist strips.
#[derive(Clone, Debug)]
pub struct Thumb {
    pub img: Option<String>,
    pub ph: &'static str,
    pub hue: u32,
}

impl From<&Listing> for Thumb {
    fn from(l: &Listing) -> Self {
        Thumb {
            img: image_url(&l.artwork.id, &l.artwork.image, "240x240"),
            ph: l.artwork.placeholder_class(),
            hue: l.artwork.placeholder_hue(),
        }
    }
}

pub struct FavButton {
    pub kind: &'static str,
    pub target: String,
    pub saved: bool,
    pub label_on: &'static str,
    pub label_off: &'static str,
}

/// The price table vendors and admins see: one row per buyer region.
pub struct RegionPrice {
    pub region: &'static str,
    pub artwork: String,
    pub shipping: String,
    pub total: String,
    pub lane: &'static str,
}

impl From<&Quote> for RegionPrice {
    fn from(q: &Quote) -> Self {
        RegionPrice {
            region: q.region.label(),
            artwork: q.show_artwork(),
            shipping: q.show_shipping(),
            total: q.show_total(),
            lane: q.lane.label(),
        }
    }
}

pub struct OrderRow {
    pub id: String,
    pub number: String,
    pub created: String,
    pub artwork: String,
    pub buyer: String,
    pub destination: &'static str,
    pub total: String,
    pub vendor_gets: String,
    pub margin: String,
    pub status: &'static str,
    pub tone: &'static str,
    pub tracking: String,
    pub actions: Vec<(&'static str, &'static str)>,
    pub needs_tracking: bool,
}

pub fn usd(cents: i64) -> String {
    crate::shared::Currency::USD.format(cents, false)
}

impl OrderRow {
    pub fn new(o: &Order, artwork_title: String) -> Self {
        let actions = o.admin_actions();
        OrderRow {
            id: o.id.clone(),
            number: o.number.clone(),
            created: o.created.clone(),
            artwork: artwork_title,
            buyer: o.buyer.name.clone(),
            destination: o.destination.code(),
            total: o.show_total(),
            vendor_gets: usd(o.vendor_price.0),
            margin: usd(o.markup.0),
            status: o.status.label(),
            tone: o.status.tone(),
            tracking: o.tracking.clone(),
            needs_tracking: actions.contains(&OrderAction::Ship),
            actions: actions.into_iter().map(|a| (a.code(), a.label())).collect(),
        }
    }
}
