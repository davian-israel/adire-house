//! Pricing context: the markup policy and shipping lanes. Pure and fully unit-tested.

use crate::inventory::domain::StockLocation;
use crate::shared::{Currency, DomainError, Region, SizeClass, UsdCents};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How a parcel travels to the buyer. Decided by where the piece sits and where it is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShippingLane {
    /// Already in the buyer's country (our US, UK or Canada warehouse).
    Local,
    /// A neighbouring warehouse: UK stock to the EU, US and Canada to each other.
    Cross,
    /// Shipped from the artist's studio in Africa (or any other long route).
    Intl,
}

impl ShippingLane {
    pub const ALL: [ShippingLane; 3] = [ShippingLane::Local, ShippingLane::Cross, ShippingLane::Intl];

    pub fn route(from: StockLocation, to: Region) -> Self {
        use Region as R;
        use StockLocation as L;
        match (from, to) {
            (L::Origin, _) => ShippingLane::Intl,
            (L::US, R::US) | (L::UK, R::UK) | (L::CA, R::CA) => ShippingLane::Local,
            (L::UK, R::EU) | (L::US, R::CA) | (L::CA, R::US) => ShippingLane::Cross,
            _ => ShippingLane::Intl,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            ShippingLane::Local => "local",
            ShippingLane::Cross => "cross",
            ShippingLane::Intl => "intl",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ShippingLane::Local => "Local stock",
            ShippingLane::Cross => "Regional stock",
            ShippingLane::Intl => "Ships from Africa",
        }
    }
    pub fn delivery(self) -> &'static str {
        match self {
            ShippingLane::Local => "2–4 business days",
            ShippingLane::Cross => "4–7 business days",
            ShippingLane::Intl => "7–14 business days",
        }
    }
    pub fn is_fast(self) -> bool {
        self != ShippingLane::Intl
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeRates {
    #[serde(rename = "S")]
    pub s: UsdCents,
    #[serde(rename = "M")]
    pub m: UsdCents,
    #[serde(rename = "L")]
    pub l: UsdCents,
}

impl SizeRates {
    pub fn get(&self, size: SizeClass) -> UsdCents {
        match size {
            SizeClass::S => self.s,
            SizeClass::M => self.m,
            SizeClass::L => self.l,
        }
    }
    pub fn set(&mut self, size: SizeClass, v: UsdCents) {
        match size {
            SizeClass::S => self.s = v,
            SizeClass::M => self.m = v,
            SizeClass::L => self.l = v,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateTable {
    pub local: SizeRates,
    pub cross: SizeRates,
    pub intl: SizeRates,
}

impl RateTable {
    pub fn lane(&self, lane: ShippingLane) -> &SizeRates {
        match lane {
            ShippingLane::Local => &self.local,
            ShippingLane::Cross => &self.cross,
            ShippingLane::Intl => &self.intl,
        }
    }
    pub fn lane_mut(&mut self, lane: ShippingLane) -> &mut SizeRates {
        match lane {
            ShippingLane::Local => &mut self.local,
            ShippingLane::Cross => &mut self.cross,
            ShippingLane::Intl => &mut self.intl,
        }
    }
}

/// The policy aggregate: one per marketplace.
#[derive(Clone, Debug, PartialEq)]
pub struct PricingPolicy {
    pub id: String,
    /// Markup on the vendor's price in basis points. 4500 = 45%.
    pub markup_bps: u32,
    pub rates: RateTable,
    /// Units of foreign currency per 1 USD.
    pub fx: HashMap<String, f64>,
}

/// Everything a buyer, vendor and admin need to know about one price.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quote {
    pub region: Region,
    pub lane: ShippingLane,
    pub vendor_price: UsdCents,
    pub markup: UsdCents,
    pub shipping: UsdCents,
    pub fx_rate: f64,
}

impl Quote {
    pub fn artwork_price(&self) -> UsdCents {
        self.vendor_price + self.markup
    }
    pub fn total(&self) -> UsdCents {
        self.artwork_price() + self.shipping
    }
    pub fn currency(&self) -> Currency {
        self.region.currency()
    }
    fn local(&self, v: UsdCents) -> i64 {
        (v.0 as f64 * self.fx_rate).round() as i64
    }
    pub fn local_total_minor(&self) -> i64 {
        self.local(self.total())
    }
    /// Buyer-facing strings in the buyer's currency.
    pub fn show_artwork(&self) -> String {
        self.currency().format(self.local(self.artwork_price()), false)
    }
    pub fn show_shipping(&self) -> String {
        self.currency().format(self.local(self.shipping), false)
    }
    pub fn show_total(&self) -> String {
        self.currency().format(self.local(self.total()), false)
    }
    pub fn show_artwork_exact(&self) -> String {
        self.currency().format(self.local(self.artwork_price()), true)
    }
    pub fn show_shipping_exact(&self) -> String {
        self.currency().format(self.local(self.shipping), true)
    }
    pub fn show_total_exact(&self) -> String {
        self.currency().format(self.local(self.total()), true)
    }
}

impl PricingPolicy {
    pub fn fx_rate(&self, region: Region) -> f64 {
        *self.fx.get(region.currency().code()).unwrap_or(&1.0)
    }

    pub fn markup_percent(&self) -> f64 {
        self.markup_bps as f64 / 100.0
    }

    /// The single rule of the business: vendor price + markup + lane shipping.
    pub fn quote(&self, vendor_price: UsdCents, size: SizeClass, from: StockLocation, to: Region) -> Quote {
        let lane = ShippingLane::route(from, to);
        Quote {
            region: to,
            lane,
            vendor_price,
            markup: vendor_price.bps(self.markup_bps),
            shipping: self.rates.lane(lane).get(size),
            fx_rate: self.fx_rate(to),
        }
    }

    pub fn set_markup_percent(&mut self, pct: f64) -> Result<(), DomainError> {
        if !(0.0..=300.0).contains(&pct) || pct.is_nan() {
            return Err(DomainError::invalid("Markup must be between 0% and 300%."));
        }
        self.markup_bps = (pct * 100.0).round() as u32;
        Ok(())
    }

    pub fn set_rate(&mut self, lane: ShippingLane, size: SizeClass, dollars: f64) -> Result<(), DomainError> {
        if !(0.0..=10_000.0).contains(&dollars) || dollars.is_nan() {
            return Err(DomainError::invalid("Shipping rates must be between $0 and $10,000."));
        }
        self.rates.lane_mut(lane).set(size, UsdCents((dollars * 100.0).round() as i64));
        Ok(())
    }

    pub fn set_fx(&mut self, currency: Currency, rate: f64) -> Result<(), DomainError> {
        if currency == Currency::USD {
            return Ok(());
        }
        if !(0.01..=1000.0).contains(&rate) || rate.is_nan() {
            return Err(DomainError::invalid("Exchange rates must be positive."));
        }
        self.fx.insert(currency.code().to_string(), rate);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> PricingPolicy {
        let r = |s, m, l| SizeRates { s: UsdCents(s), m: UsdCents(m), l: UsdCents(l) };
        PricingPolicy {
            id: "p".into(),
            markup_bps: 4500,
            rates: RateTable { local: r(1200, 2500, 6000), cross: r(2400, 4800, 9500), intl: r(4500, 9500, 18000) },
            fx: [("USD", 1.0), ("CAD", 1.37), ("EUR", 0.92), ("GBP", 0.79)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn applies_45_percent_markup_plus_shipping() {
        let q = policy().quote(UsdCents::from_dollars(620), SizeClass::L, StockLocation::UK, Region::UK);
        assert_eq!(q.lane, ShippingLane::Local);
        assert_eq!(q.markup, UsdCents::from_dollars(279));
        assert_eq!(q.artwork_price(), UsdCents::from_dollars(899));
        assert_eq!(q.shipping, UsdCents::from_dollars(60));
        assert_eq!(q.total(), UsdCents::from_dollars(959));
        assert_eq!(q.show_total(), "£758"); // 959 * 0.79 = 757.61
    }

    #[test]
    fn routes_lanes() {
        use Region as R;
        use StockLocation as L;
        assert_eq!(ShippingLane::route(L::Origin, R::US), ShippingLane::Intl);
        assert_eq!(ShippingLane::route(L::US, R::US), ShippingLane::Local);
        assert_eq!(ShippingLane::route(L::UK, R::EU), ShippingLane::Cross);
        assert_eq!(ShippingLane::route(L::CA, R::US), ShippingLane::Cross);
        assert_eq!(ShippingLane::route(L::UK, R::US), ShippingLane::Intl);
        assert_eq!(ShippingLane::route(L::US, R::EU), ShippingLane::Intl);
    }

    #[test]
    fn rejects_silly_settings() {
        let mut p = policy();
        assert!(p.set_markup_percent(-1.0).is_err());
        assert!(p.set_markup_percent(50.0).is_ok());
        assert_eq!(p.markup_bps, 5000);
        assert!(p.set_rate(ShippingLane::Local, SizeClass::S, f64::NAN).is_err());
    }
}
