use super::session::{Role, Session, Visitor, SESSION};
use super::views::Chrome;
use super::{redirect, render};
use crate::error::AppError;
use crate::state::AppState;
use askama::Template;
use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::SignedCookieJar;
use serde::Deserialize;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage {
    chrome: Chrome,
}

pub async fn login_page(v: Visitor) -> Result<Response, AppError> {
    Ok(render(LoginPage { chrome: Chrome::new(&v, 0, "login") })?.into_response())
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct UserRecord {
    id: String,
    email: String,
    #[serde(default)]
    name: String,
    role: Role,
    #[serde(default)]
    vendor: String,
}

pub async fn login(State(st): State<AppState>, jar: SignedCookieJar, headers: HeaderMap, Form(f): Form<LoginForm>) -> Result<Response, AppError> {
    let u: UserRecord = st.pb.auth_user(f.email.trim(), &f.password).await?;
    let s = Session {
        user_id: u.id,
        name: if u.name.is_empty() { u.email.clone() } else { u.name },
        email: u.email,
        role: u.role,
        vendor_id: Some(u.vendor).filter(|v| !v.is_empty()),
    };
    let to = if s.is_admin() { "/admin" } else { "/vendor" };
    let jar = jar.add(s.cookie(st.secure_cookies));
    Ok((jar, redirect(&headers, to)).into_response())
}

pub async fn logout(jar: SignedCookieJar, headers: HeaderMap) -> Response {
    (jar.remove(Cookie::build(SESSION).path("/")), redirect(&headers, "/")).into_response()
}
