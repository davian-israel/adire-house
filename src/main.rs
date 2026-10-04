//! Adire House: a marketplace connecting African artists and shops with buyers in the
//! US, Canada, the UK and Europe.
//!
//! Bounded contexts (each a module with domain + repository):
//!   catalogue  artworks and listing status
//!   vendors    artists and shops, onboarding and verification
//!   pricing    markup and shipping lanes as a policy
//!   inventory  stock locations (studio, US/UK/Canada warehouses)
//!   orders     orders and escrow, with a payment gateway port
//!   customers  favourites (supporting context)

mod catalogue;
mod config;
mod customers;
mod error;
mod infra;
mod inventory;
mod orders;
mod pricing;
mod shared;
mod state;
mod storefront;
mod vendors;
mod web;

use axum_extra::extract::cookie::Key;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "adire_house=info,tower_http=info".into()))
        .init();

    let cfg = config::Config::from_env()?;
    let pb = infra::pocketbase::PocketBase::new(&cfg.pb_url, &cfg.pb_email, &cfg.pb_password)?;
    let state = state::AppState::new(pb, Key::derive_from(cfg.session_secret.as_bytes()), cfg.secure_cookies);

    // Fail fast if PocketBase or the pricing policy is missing.
    state.pricing.current().await.map_err(|e| anyhow::anyhow!("cannot load pricing policy from PocketBase: {e}"))?;

    let app = web::router(state);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("Adire House listening on http://{}", cfg.bind);
    axum::serve(listener, app).with_graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; }).await?;
    Ok(())
}
