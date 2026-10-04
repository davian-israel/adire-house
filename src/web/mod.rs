//! HTTP layer: Axum handlers rendering Askama templates for htmx.

pub mod admin;
pub mod auth;
pub mod checkout;
pub mod session;
pub mod storefront;
pub mod vendor;
pub mod views;

use crate::error::AppError;
use crate::state::AppState;
use askama::Template;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{middleware, Router};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn router(st: AppState) -> Router {
    Router::new()
        // Storefront (Catalogue + Pricing + Inventory read model)
        .route("/", get(storefront::browse))
        .route("/artworks", get(storefront::browse))
        .route("/artworks/{id}", get(storefront::detail))
        .route("/shops", get(storefront::shops))
        .route("/artists", get(storefront::artists))
        .route("/saved", get(storefront::saved))
        .route("/favourites", post(storefront::toggle_favourite))
        .route("/region", post(storefront::set_region))
        .route("/media/{id}/{file}", get(storefront::media))
        // Orders / Escrow (buyer side)
        .route("/checkout/{artwork_id}", get(checkout::form).post(checkout::place))
        .route("/orders/{id}", get(checkout::order_page))
        .route("/orders/{id}/confirm-delivery", post(checkout::confirm_delivery))
        // Staff sign-in
        .route("/login", get(auth::login_page).post(auth::login))
        .route("/logout", post(auth::logout))
        // Vendor portal
        .route("/vendor", get(vendor::dashboard))
        .route("/vendor/quote", post(vendor::quote_preview))
        .route("/vendor/artworks", post(vendor::create_artwork))
        .route("/vendor/artworks/{id}/stock", post(vendor::move_stock))
        .route("/vendor/artworks/{id}/{action}", post(vendor::set_listing))
        // Admin
        .route("/admin", get(admin::dashboard))
        .route("/admin/vendors", post(admin::onboard))
        .route("/admin/vendors/{id}/{action}", post(admin::vendor_action))
        .route("/admin/pricing", post(admin::save_pricing))
        .route("/admin/orders/{id}/{action}", post(admin::order_action))
        .route("/healthz", get(|| async { "ok" }))
        .nest_service("/static", ServeDir::new("static"))
        .layer(middleware::from_fn_with_state(st.clone(), session::ensure_customer_key))
        .layer(axum::extract::DefaultBodyLimit::max(12 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(st)
}

pub fn is_htmx(h: &HeaderMap) -> bool {
    h.get("HX-Request").is_some_and(|v| v == "true")
}

/// True when htmx asked for a specific element (so we can return just a fragment).
pub fn hx_target_is(h: &HeaderMap, id: &str) -> bool {
    is_htmx(h) && h.get("HX-Target").is_some_and(|v| v == id)
}

pub fn render(t: impl Template) -> Result<Html<String>, AppError> {
    t.render().map(Html).map_err(|e| AppError::Internal(e.into()))
}

/// Redirect that works for both htmx and plain form posts.
pub fn redirect(h: &HeaderMap, to: &str) -> Response {
    if is_htmx(h) {
        let mut r = StatusCode::NO_CONTENT.into_response();
        if let Ok(v) = HeaderValue::from_str(to) {
            r.headers_mut().insert("HX-Redirect", v);
        }
        r
    } else {
        (StatusCode::SEE_OTHER, [(header::LOCATION, to.to_string())]).into_response()
    }
}

/// Where a plain form post should land after a toggle: the page it came from, if same-site.
pub fn back(h: &HeaderMap) -> String {
    h.get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.find("://").map(|i| &r[i + 3..]))
        .and_then(|r| r.find('/').map(|i| r[i..].to_string()))
        .unwrap_or_else(|| "/".into())
}
