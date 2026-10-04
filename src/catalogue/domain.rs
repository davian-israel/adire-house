//! Catalogue context: the artworks vendors list for sale.

use crate::shared::{DomainError, SizeClass, UsdCents};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Medium {
    Painting,
    Textile,
    Print,
    Sculpture,
    MixedMedia,
}

impl Medium {
    pub const ALL: [Medium; 5] = [Medium::Painting, Medium::Textile, Medium::Print, Medium::Sculpture, Medium::MixedMedia];

    pub fn code(self) -> &'static str {
        match self {
            Medium::Painting => "painting",
            Medium::Textile => "textile",
            Medium::Print => "print",
            Medium::Sculpture => "sculpture",
            Medium::MixedMedia => "mixed_media",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Medium::Painting => "Painting",
            Medium::Textile => "Textile",
            Medium::Print => "Print",
            Medium::Sculpture => "Sculpture",
            Medium::MixedMedia => "Mixed media",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtworkStatus {
    Listed,
    Withdrawn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Artwork {
    pub id: String,
    pub vendor_id: String,
    pub artist: String,
    pub title: String,
    pub medium: Medium,
    pub materials: String,
    pub dimensions: String,
    pub size: SizeClass,
    pub vendor_price: UsdCents,
    pub year: Option<i32>,
    pub status: ArtworkStatus,
    /// Stored file name in PocketBase, if a photo was uploaded.
    pub image: Option<String>,
}

impl Artwork {
    pub fn is_listed(&self) -> bool {
        self.status == ArtworkStatus::Listed
    }
    pub fn withdraw(&mut self) {
        self.status = ArtworkStatus::Withdrawn;
    }
    pub fn relist(&mut self) {
        self.status = ArtworkStatus::Listed;
    }
    /// A deterministic pattern used when no photo has been uploaded yet.
    pub fn placeholder_class(&self) -> &'static str {
        match self.medium {
            Medium::Textile => "ph-strip",
            Medium::Print => "ph-grid",
            Medium::MixedMedia => "ph-geo",
            Medium::Sculpture => "ph-form",
            Medium::Painting => "ph-wash",
        }
    }
    pub fn placeholder_hue(&self) -> u32 {
        self.id.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360
    }
}

/// Command: a vendor lists a new piece.
#[derive(Clone, Debug)]
pub struct NewArtwork {
    pub vendor_id: String,
    pub artist: String,
    pub title: String,
    pub medium: Medium,
    pub materials: String,
    pub dimensions: String,
    pub size: SizeClass,
    pub vendor_price: UsdCents,
    pub year: Option<i32>,
}

impl NewArtwork {
    pub const MIN_PRICE: UsdCents = UsdCents(1_00);
    pub const MAX_PRICE: UsdCents = UsdCents(1_000_000_00);

    pub fn validate(mut self) -> Result<Self, DomainError> {
        self.title = self.title.trim().to_string();
        self.artist = self.artist.trim().to_string();
        self.materials = self.materials.trim().to_string();
        self.dimensions = self.dimensions.trim().to_string();
        if self.title.is_empty() || self.title.len() > 120 {
            return Err(DomainError::invalid("Give the piece a title (up to 120 characters)."));
        }
        if self.artist.is_empty() {
            return Err(DomainError::invalid("Enter the artist's name."));
        }
        if self.vendor_price < Self::MIN_PRICE || self.vendor_price > Self::MAX_PRICE {
            return Err(DomainError::invalid("Your price must be between $1 and $1,000,000."));
        }
        if self.materials.is_empty() {
            self.materials = self.medium.label().to_string();
        }
        if self.dimensions.is_empty() {
            self.dimensions = "Size on request".into();
        }
        Ok(self)
    }
}

/// Buyer-side search criteria. Price and delivery-speed filters are applied after pricing.
#[derive(Clone, Debug, Default)]
pub struct ArtworkSearch {
    pub text: Option<String>,
    pub medium: Option<Medium>,
    pub country: Option<String>,
    pub vendor_id: Option<String>,
    pub artist: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_new_artwork() {
        let base = NewArtwork {
            vendor_id: "v".into(), artist: " Efua ".into(), title: " Makola ".into(), medium: Medium::Painting,
            materials: "".into(), dimensions: "".into(), size: SizeClass::M, vendor_price: UsdCents(30_000), year: Some(2026),
        };
        let ok = base.clone().validate().unwrap();
        assert_eq!(ok.title, "Makola");
        assert_eq!(ok.materials, "Painting");
        let mut cheap = base.clone();
        cheap.vendor_price = UsdCents(50);
        assert!(cheap.validate().is_err());
        let mut untitled = base;
        untitled.title = "  ".into();
        assert!(untitled.validate().is_err());
    }
}
