//! Payment port. Escrow is modelled in the Order aggregate; the gateway only moves money.
//!
//! Production adapters to write:
//!  - Stripe Connect (separate charges and transfers: charge the buyer now, `transfers.create`
//!    to the vendor's connected account when the order reaches DeliveredReleased).
//!  - Paystack / Flutterwave for payouts to Ghanaian, Nigerian and Kenyan bank or mobile money accounts.

use super::domain::Order;
use async_trait::async_trait;

pub enum PaymentOutcome {
    /// Funds captured immediately (dev gateway, or a confirmed card payment).
    Captured { reference: String },
    /// Buyer must be sent to a hosted checkout page; a webhook marks the order paid later.
    Redirect { url: String },
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn collect(&self, order: &Order) -> anyhow::Result<PaymentOutcome>;
    /// Pay the vendor their share once escrow is released.
    async fn release_to_vendor(&self, order: &Order) -> anyhow::Result<()>;
    async fn refund(&self, order: &Order) -> anyhow::Result<()>;
}

/// Development gateway: every payment succeeds instantly. Never enable in production.
pub struct DevGateway;

#[async_trait]
impl PaymentGateway for DevGateway {
    async fn collect(&self, order: &Order) -> anyhow::Result<PaymentOutcome> {
        tracing::info!(order = %order.number, total = order.total().0, "dev gateway: payment captured");
        Ok(PaymentOutcome::Captured { reference: format!("dev_{}", order.number) })
    }
    async fn release_to_vendor(&self, order: &Order) -> anyhow::Result<()> {
        tracing::info!(order = %order.number, vendor = %order.vendor_id, amount = order.vendor_price.0, "dev gateway: vendor payout");
        Ok(())
    }
    async fn refund(&self, order: &Order) -> anyhow::Result<()> {
        tracing::info!(order = %order.number, "dev gateway: refund");
        Ok(())
    }
}
