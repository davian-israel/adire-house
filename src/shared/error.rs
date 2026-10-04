use thiserror::Error;

/// Rule violations raised by domain objects. The web layer turns these into 4xx responses.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("{0}")]
    Invalid(String),
    #[error("that step isn't possible while the order is {from}")]
    InvalidTransition { from: &'static str, action: &'static str },
    #[error("this vendor is already {from}")]
    InvalidVendorTransition { from: &'static str, action: &'static str },
    #[error("this piece has just sold out")]
    OutOfStock,
    #[error("this piece is not available for sale")]
    NotAvailable,
}

impl DomainError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        DomainError::Invalid(msg.into())
    }
}
