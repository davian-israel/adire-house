//! Vendors context: artists and shops, and the verification lifecycle that gates them.

use crate::shared::DomainError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VendorKind {
    Artist,
    Shop,
}

impl VendorKind {
    pub fn label(self) -> &'static str {
        match self {
            VendorKind::Artist => "Artist studio",
            VendorKind::Shop => "Shop",
        }
    }
    pub fn is_shop(self) -> bool {
        self == VendorKind::Shop
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VendorStatus {
    /// Signed up, studio visit not yet done. Hidden from buyers.
    Pending,
    /// Visited and approved. Listings are live.
    Verified,
    /// Removed from sale by an admin.
    Suspended,
}

impl VendorStatus {
    pub fn code(self) -> &'static str {
        match self {
            VendorStatus::Pending => "pending",
            VendorStatus::Verified => "verified",
            VendorStatus::Suspended => "suspended",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            VendorStatus::Pending => "Pending",
            VendorStatus::Verified => "Verified",
            VendorStatus::Suspended => "Suspended",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vendor {
    pub id: String,
    pub name: String,
    pub kind: VendorKind,
    pub country: String,
    pub city: String,
    #[serde(default)]
    pub bio: String,
    pub status: VendorStatus,
}

impl Vendor {
    pub fn is_live(&self) -> bool {
        self.status == VendorStatus::Verified
    }
    pub fn approve(&mut self) -> Result<(), DomainError> {
        match self.status {
            VendorStatus::Pending | VendorStatus::Suspended => {
                self.status = VendorStatus::Verified;
                Ok(())
            }
            VendorStatus::Verified => Err(DomainError::InvalidVendorTransition { from: "verified", action: "approve" }),
        }
    }
    pub fn suspend(&mut self) -> Result<(), DomainError> {
        match self.status {
            VendorStatus::Suspended => Err(DomainError::InvalidVendorTransition { from: "suspended", action: "suspend" }),
            _ => {
                self.status = VendorStatus::Suspended;
                Ok(())
            }
        }
    }
}

/// Command for onboarding a vendor. Every new vendor starts as pending.
#[derive(Clone, Debug)]
pub struct NewVendor {
    pub name: String,
    pub kind: VendorKind,
    pub country: String,
    pub city: String,
    pub bio: String,
}

impl NewVendor {
    pub fn validate(self) -> Result<Self, DomainError> {
        let t = |s: String| s.trim().to_string();
        let v = NewVendor { name: t(self.name), country: t(self.country), city: t(self.city), bio: t(self.bio), kind: self.kind };
        if v.name.is_empty() || v.name.len() > 120 {
            return Err(DomainError::invalid("Enter the vendor's name (up to 120 characters)."));
        }
        if v.country.is_empty() || v.city.is_empty() {
            return Err(DomainError::invalid("Enter the vendor's country and city."));
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(status: VendorStatus) -> Vendor {
        Vendor { id: "x".into(), name: "n".into(), kind: VendorKind::Artist, country: "Ghana".into(), city: "Accra".into(), bio: "".into(), status }
    }

    #[test]
    fn verification_lifecycle() {
        let mut a = v(VendorStatus::Pending);
        assert!(!a.is_live());
        a.approve().unwrap();
        assert!(a.is_live());
        assert!(a.approve().is_err());
        a.suspend().unwrap();
        assert!(!a.is_live());
        a.approve().unwrap();
    }
}
