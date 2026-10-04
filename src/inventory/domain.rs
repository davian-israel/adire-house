//! Inventory context: where each piece physically sits, and how many are left.

use crate::shared::DomainError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StockLocation {
    #[serde(rename = "ORIGIN")]
    Origin,
    US,
    UK,
    CA,
}

impl StockLocation {
    pub const ALL: [StockLocation; 4] = [StockLocation::Origin, StockLocation::US, StockLocation::UK, StockLocation::CA];

    pub fn code(self) -> &'static str {
        match self {
            StockLocation::Origin => "ORIGIN",
            StockLocation::US => "US",
            StockLocation::UK => "UK",
            StockLocation::CA => "CA",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            StockLocation::Origin => "At the artist's studio",
            StockLocation::US => "US warehouse",
            StockLocation::UK => "UK warehouse",
            StockLocation::CA => "Canada warehouse",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code() == s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StockItem {
    pub id: String,
    pub artwork_id: String,
    pub location: StockLocation,
    pub quantity: u32,
}

impl StockItem {
    pub fn available(&self) -> bool {
        self.quantity > 0
    }
    /// Hold one unit for an order.
    pub fn reserve(&mut self) -> Result<(), DomainError> {
        if self.quantity == 0 {
            return Err(DomainError::OutOfStock);
        }
        self.quantity -= 1;
        Ok(())
    }
    /// Put a unit back after a cancelled or refunded order.
    pub fn release(&mut self) {
        self.quantity += 1;
    }
    pub fn move_to(&mut self, loc: StockLocation) {
        self.location = loc;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_oversell() {
        let mut s = StockItem { id: "x".into(), artwork_id: "a".into(), location: StockLocation::US, quantity: 1 };
        assert!(s.reserve().is_ok());
        assert_eq!(s.reserve(), Err(DomainError::OutOfStock));
        s.release();
        assert!(s.available());
    }
}
