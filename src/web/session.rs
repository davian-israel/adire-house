//! Cookies: a signed session for admins and vendors, an anonymous customer key for favourites,
//! and the buyer's chosen delivery region.

use crate::error::AppError;
use crate::shared::Region;
use crate::state::AppState;
use axum::extract::{FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::{CookieJar, SignedCookieJar};
use serde::{Deserialize, Serialize};

pub const SESSION: &str = "ah_session";
pub const CUSTOMER: &str = "ah_ck";
pub const REGION: &str = "ah_region";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Vendor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub vendor_id: Option<String>,
}

impl Session {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
    pub fn cookie(&self, secure: bool) -> Cookie<'static> {
        Cookie::build((SESSION, serde_json::to_string(self).unwrap_or_default()))
            .path("/")
            .http_only(true)
            .secure(secure)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::days(7))
            .build()
    }
}

/// Everything about the current browser: region, anonymous key and optional staff session.
#[derive(Clone, Debug)]
pub struct Visitor {
    pub region: Region,
    pub customer_key: String,
    pub session: Option<Session>,
}

#[derive(Clone)]
struct CustomerKey(String);

/// Ensures every browser has an anonymous customer key before handlers run.
pub async fn ensure_customer_key(axum::extract::State(st): axum::extract::State<AppState>, mut req: Request, next: Next) -> Response {
    let jar = CookieJar::from_headers(req.headers());
    let (key, fresh) = match jar.get(CUSTOMER).map(|c| c.value().to_string()) {
        Some(k) if k.len() == 32 && k.bytes().all(|b| b.is_ascii_hexdigit()) => (k, false),
        _ => (uuid::Uuid::new_v4().simple().to_string(), true),
    };
    req.extensions_mut().insert(CustomerKey(key.clone()));
    let mut res = next.run(req).await;
    if fresh {
        let c = Cookie::build((CUSTOMER, key))
            .path("/")
            .http_only(true)
            .secure(st.secure_cookies)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::days(365))
            .build();
        if let Ok(v) = HeaderValue::from_str(&c.to_string()) {
            res.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    res
}

impl FromRequestParts<AppState> for Visitor {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let region = jar.get(REGION).and_then(|c| Region::parse(c.value())).unwrap_or(Region::US);
        let customer_key = parts
            .extensions
            .get::<CustomerKey>()
            .map(|k| k.0.clone())
            .ok_or_else(|| anyhow::anyhow!("customer key middleware missing"))?;
        let signed = SignedCookieJar::from_headers(&parts.headers, st.cookie_key.clone());
        let session = signed.get(SESSION).and_then(|c| serde_json::from_str(c.value()).ok());
        Ok(Visitor { region, customer_key, session })
    }
}

/// Extractor: a signed-in admin.
pub struct Admin(pub Session);

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let v = Visitor::from_request_parts(parts, st).await?;
        match v.session {
            Some(s) if s.is_admin() => Ok(Admin(s)),
            Some(_) => Err(AppError::Forbidden),
            None => Err(AppError::Unauthorized),
        }
    }
}

/// Extractor: a signed-in vendor, with their vendor id.
pub struct VendorUser {
    pub session: Session,
    pub vendor_id: String,
}

impl FromRequestParts<AppState> for VendorUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let v = Visitor::from_request_parts(parts, st).await?;
        match v.session {
            Some(s) if s.role == Role::Vendor => {
                let vendor_id = s.vendor_id.clone().ok_or(AppError::Forbidden)?;
                Ok(VendorUser { session: s, vendor_id })
            }
            Some(_) => Err(AppError::Forbidden),
            None => Err(AppError::Unauthorized),
        }
    }
}
