//! Orders / Escrow context. The buyer's money is held until delivery is confirmed,
//! then released to the vendor. The state machine below is the whole guarantee.

use crate::pricing::domain::Quote;
use crate::shared::{DomainError, Region, UsdCents};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    AwaitingPayment,
    /// Paid by the buyer, held in escrow. Vendor ships.
    PaidHeld,
    Shipped,
    /// Buyer confirmed receipt; funds released to the vendor.
    DeliveredReleased,
    Refunded,
    Cancelled,
}

impl OrderStatus {
    pub fn code(self) -> &'static str {
        match self {
            OrderStatus::AwaitingPayment => "awaiting_payment",
            OrderStatus::PaidHeld => "paid_held",
            OrderStatus::Shipped => "shipped",
            OrderStatus::DeliveredReleased => "delivered_released",
            OrderStatus::Refunded => "refunded",
            OrderStatus::Cancelled => "cancelled",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            OrderStatus::AwaitingPayment => "Awaiting payment",
            OrderStatus::PaidHeld => "Paid, held in escrow",
            OrderStatus::Shipped => "In transit",
            OrderStatus::DeliveredReleased => "Delivered, vendor paid",
            OrderStatus::Refunded => "Refunded",
            OrderStatus::Cancelled => "Cancelled",
        }
    }
    /// Mid-sentence wording, for error messages.
    pub fn phrase(self) -> &'static str {
        match self {
            OrderStatus::AwaitingPayment => "awaiting payment",
            OrderStatus::PaidHeld => "paid and held in escrow",
            OrderStatus::Shipped => "in transit",
            OrderStatus::DeliveredReleased => "delivered and paid out",
            OrderStatus::Refunded => "refunded",
            OrderStatus::Cancelled => "cancelled",
        }
    }
    /// For badges: ok / warn / neutral / bad.
    pub fn tone(self) -> &'static str {
        match self {
            OrderStatus::DeliveredReleased => "ok",
            OrderStatus::PaidHeld | OrderStatus::AwaitingPayment => "warn",
            OrderStatus::Refunded | OrderStatus::Cancelled => "bad",
            OrderStatus::Shipped => "",
        }
    }
    /// Whether this transition returns the unit to stock.
    pub fn restocks(self) -> bool {
        matches!(self, OrderStatus::Refunded | OrderStatus::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderAction {
    MarkPaid,
    Ship,
    ConfirmDelivery,
    Refund,
    Cancel,
}

impl OrderAction {
    pub fn code(self) -> &'static str {
        match self {
            OrderAction::MarkPaid => "mark-paid",
            OrderAction::Ship => "ship",
            OrderAction::ConfirmDelivery => "confirm-delivery",
            OrderAction::Refund => "refund",
            OrderAction::Cancel => "cancel",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            OrderAction::MarkPaid => "Mark paid",
            OrderAction::Ship => "Mark shipped",
            OrderAction::ConfirmDelivery => "Confirm delivery",
            OrderAction::Refund => "Refund buyer",
            OrderAction::Cancel => "Cancel",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::MarkPaid, Self::Ship, Self::ConfirmDelivery, Self::Refund, Self::Cancel]
            .into_iter()
            .find(|a| a.code() == s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Buyer {
    pub name: String,
    pub email: String,
    pub address: String,
}

impl Buyer {
    pub fn validate(self) -> Result<Self, DomainError> {
        let b = Buyer { name: self.name.trim().into(), email: self.email.trim().into(), address: self.address.trim().into() };
        if b.name.is_empty() {
            return Err(DomainError::invalid("Enter your full name."));
        }
        let at = b.email.find('@');
        if at.is_none_or(|i| i == 0 || !b.email[i..].contains('.')) {
            return Err(DomainError::invalid("Enter a valid email address, like name@example.com."));
        }
        if b.address.len() < 10 {
            return Err(DomainError::invalid("Enter your full delivery address, including postcode."));
        }
        Ok(b)
    }
}

/// The Order aggregate. The price is a snapshot: later policy changes never alter it.
#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub id: String,
    pub number: String,
    pub artwork_id: String,
    pub vendor_id: String,
    pub buyer: Buyer,
    pub destination: Region,
    pub vendor_price: UsdCents,
    pub markup: UsdCents,
    pub shipping: UsdCents,
    pub lane: String,
    pub fx_rate: f64,
    pub status: OrderStatus,
    pub payment_ref: String,
    pub tracking: String,
    pub created: String,
}

impl Order {
    pub fn place(number: String, artwork_id: String, vendor_id: String, buyer: Buyer, quote: &Quote) -> Self {
        Order {
            id: String::new(),
            number,
            artwork_id,
            vendor_id,
            buyer,
            destination: quote.region,
            vendor_price: quote.vendor_price,
            markup: quote.markup,
            shipping: quote.shipping,
            lane: quote.lane.code().into(),
            fx_rate: quote.fx_rate,
            status: OrderStatus::AwaitingPayment,
            payment_ref: String::new(),
            tracking: String::new(),
            created: String::new(),
        }
    }

    pub fn total(&self) -> UsdCents {
        self.vendor_price + self.markup + self.shipping
    }
    pub fn show_total(&self) -> String {
        let minor = (self.total().0 as f64 * self.fx_rate).round() as i64;
        self.destination.currency().format(minor, true)
    }

    fn transition(&mut self, action: OrderAction) -> Result<(), DomainError> {
        use OrderAction as A;
        use OrderStatus as S;
        let next = match (self.status, action) {
            (S::AwaitingPayment, A::MarkPaid) => S::PaidHeld,
            (S::AwaitingPayment, A::Cancel) => S::Cancelled,
            (S::PaidHeld, A::Ship) => S::Shipped,
            (S::PaidHeld | S::Shipped, A::Refund) => S::Refunded,
            (S::Shipped, A::ConfirmDelivery) => S::DeliveredReleased,
            (from, a) => return Err(DomainError::InvalidTransition { from: from.phrase(), action: a.label() }),
        };
        self.status = next;
        Ok(())
    }

    pub fn mark_paid(&mut self, payment_ref: &str) -> Result<(), DomainError> {
        self.transition(OrderAction::MarkPaid)?;
        self.payment_ref = payment_ref.into();
        Ok(())
    }
    pub fn ship(&mut self, tracking: &str) -> Result<(), DomainError> {
        self.transition(OrderAction::Ship)?;
        self.tracking = tracking.trim().into();
        Ok(())
    }
    pub fn confirm_delivery(&mut self) -> Result<(), DomainError> {
        self.transition(OrderAction::ConfirmDelivery)
    }
    pub fn refund(&mut self) -> Result<(), DomainError> {
        self.transition(OrderAction::Refund)
    }
    pub fn cancel(&mut self) -> Result<(), DomainError> {
        self.transition(OrderAction::Cancel)
    }

    pub fn apply(&mut self, action: OrderAction, payment_ref: &str, tracking: &str) -> Result<(), DomainError> {
        match action {
            OrderAction::MarkPaid => self.mark_paid(payment_ref),
            OrderAction::Ship => self.ship(tracking),
            OrderAction::ConfirmDelivery => self.confirm_delivery(),
            OrderAction::Refund => self.refund(),
            OrderAction::Cancel => self.cancel(),
        }
    }

    /// What an admin may do next.
    pub fn admin_actions(&self) -> Vec<OrderAction> {
        use OrderAction as A;
        match self.status {
            OrderStatus::AwaitingPayment => vec![A::MarkPaid, A::Cancel],
            OrderStatus::PaidHeld => vec![A::Ship, A::Refund],
            OrderStatus::Shipped => vec![A::ConfirmDelivery, A::Refund],
            _ => vec![],
        }
    }
    /// Only the buyer releases escrow in the normal flow.
    pub fn buyer_can_confirm(&self) -> bool {
        self.status == OrderStatus::Shipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::domain::StockLocation;
    use crate::pricing::domain::{PricingPolicy, RateTable, SizeRates};
    use crate::shared::SizeClass;

    fn order() -> Order {
        let r = SizeRates { s: UsdCents(1200), m: UsdCents(2500), l: UsdCents(6000) };
        let p = PricingPolicy {
            id: "p".into(),
            markup_bps: 4500,
            rates: RateTable { local: r, cross: r, intl: r },
            fx: [("USD".to_string(), 1.0)].into(),
        };
        let quote = p.quote(UsdCents(10_000), SizeClass::M, StockLocation::US, Region::US);
        let buyer = Buyer { name: "A".into(), email: "a@b.co".into(), address: "1 Long Street, Town".into() };
        Order::place("AH-1".into(), "a".into(), "v".into(), buyer, &quote)
    }

    #[test]
    fn happy_path_releases_escrow_only_after_delivery() {
        let mut o = order();
        assert_eq!(o.total(), UsdCents(10_000 + 4_500 + 2_500));
        assert!(o.ship("X").is_err(), "cannot ship before payment");
        o.mark_paid("pay_1").unwrap();
        assert!(o.confirm_delivery().is_err(), "cannot release before shipping");
        o.ship("DHL123").unwrap();
        assert!(o.buyer_can_confirm());
        o.confirm_delivery().unwrap();
        assert_eq!(o.status, OrderStatus::DeliveredReleased);
        assert!(o.refund().is_err(), "released funds cannot be refunded here");
    }

    #[test]
    fn refund_and_cancel_restock() {
        let mut o = order();
        o.cancel().unwrap();
        assert!(o.status.restocks());
        let mut o = order();
        o.mark_paid("p").unwrap();
        o.refund().unwrap();
        assert!(o.status.restocks());
    }

    #[test]
    fn validates_buyer() {
        let b = |e: &str| Buyer { name: "A".into(), email: e.into(), address: "1 Long Street, Town".into() }.validate();
        assert!(b("a@b.co").is_ok());
        assert!(b("ab.co").is_err());
        assert!(b("@b.co").is_err());
    }
}
