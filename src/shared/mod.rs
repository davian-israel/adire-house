//! Shared kernel: value objects every bounded context agrees on.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Sub};

pub mod error;
pub use error::DomainError;

/// Money is always held in US cents internally; conversion happens only at the edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsdCents(pub i64);

impl UsdCents {
    pub fn from_dollars(d: i64) -> Self {
        Self(d * 100)
    }
    /// Apply a rate in basis points (4500 = 45%), rounding half up.
    pub fn bps(self, bps: u32) -> Self {
        Self((self.0 * bps as i64 + 5_000) / 10_000)
    }
}
impl Add for UsdCents {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self(self.0 + o.0)
    }
}
impl Sub for UsdCents {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self(self.0 - o.0)
    }
}

/// A buyer's destination market.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Region {
    US,
    CA,
    EU,
    UK,
}

impl Region {
    pub const ALL: [Region; 4] = [Region::US, Region::CA, Region::EU, Region::UK];

    pub fn code(self) -> &'static str {
        match self {
            Region::US => "US",
            Region::CA => "CA",
            Region::EU => "EU",
            Region::UK => "UK",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Region::US => "United States",
            Region::CA => "Canada",
            Region::EU => "Europe (EU)",
            Region::UK => "United Kingdom",
        }
    }
    pub fn currency(self) -> Currency {
        match self {
            Region::US => Currency::USD,
            Region::CA => Currency::CAD,
            Region::EU => Currency::EUR,
            Region::UK => Currency::GBP,
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "US" => Some(Region::US),
            "CA" => Some(Region::CA),
            "EU" => Some(Region::EU),
            "UK" => Some(Region::UK),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    USD,
    CAD,
    EUR,
    GBP,
}

impl Currency {
    pub fn code(self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::CAD => "CAD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
        }
    }
    fn symbol(self) -> &'static str {
        match self {
            Currency::USD => "$",
            Currency::CAD => "CA$",
            Currency::EUR => "€",
            Currency::GBP => "£",
        }
    }
    /// Format minor units (cents/pence) for display.
    pub fn format(self, minor: i64, decimals: bool) -> String {
        let neg = minor < 0;
        let minor = minor.abs();
        let whole = if decimals { minor / 100 } else { (minor + 50) / 100 };
        let mut digits = whole.to_string();
        let mut grouped = String::new();
        while digits.len() > 3 {
            let tail = digits.split_off(digits.len() - 3);
            grouped = format!(",{tail}{grouped}");
        }
        grouped = format!("{digits}{grouped}");
        let frac = if decimals { format!(".{:02}", minor % 100) } else { String::new() };
        format!("{}{}{grouped}{frac}", if neg { "-" } else { "" }, self.symbol())
    }
}

/// Physical shipping size of a piece. Owned by Catalogue, read by Pricing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeClass {
    S,
    M,
    L,
}

impl SizeClass {
    pub const ALL: [SizeClass; 3] = [SizeClass::S, SizeClass::M, SizeClass::L];
    pub fn code(self) -> &'static str {
        match self {
            SizeClass::S => "S",
            SizeClass::M => "M",
            SizeClass::L => "L",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SizeClass::S => "Small (up to 50 cm)",
            SizeClass::M => "Medium (50–100 cm)",
            SizeClass::L => "Large (over 100 cm)",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "S" => Some(SizeClass::S),
            "M" => Some(SizeClass::M),
            "L" => Some(SizeClass::L),
            _ => None,
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_money() {
        assert_eq!(Currency::USD.format(123_456_78, true), "$123,456.78");
        assert_eq!(Currency::GBP.format(99_950, false), "£1,000");
        assert_eq!(Currency::CAD.format(500, false), "CA$5");
    }

    #[test]
    fn applies_basis_points() {
        assert_eq!(UsdCents(62_000).bps(4500), UsdCents(27_900));
        assert_eq!(UsdCents(1).bps(4500), UsdCents(0));
        assert_eq!(UsdCents(3).bps(5000), UsdCents(2));
    }
}
