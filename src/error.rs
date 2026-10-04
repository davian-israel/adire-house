//! Maps domain and infrastructure failures onto HTTP. For htmx requests, user-facing errors are
//! swapped into the page's #flash slot (status 422 is configured to swap in base.html).

use crate::infra::pocketbase::PbError;
use crate::shared::DomainError;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("{0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("sign in required")]
    Unauthorized,
    #[error("you don't have access to this page")]
    Forbidden,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<PbError> for AppError {
    fn from(e: PbError) -> Self {
        match e {
            PbError::NotFound => AppError::NotFound,
            PbError::BadCredentials => AppError::BadRequest("That email and password don't match an account.".into()),
            PbError::Rejected { status, body } if status == StatusCode::BAD_REQUEST => {
                tracing::warn!(%body, "pocketbase validation failed");
                AppError::BadRequest(pb_message(&body))
            }
            other => AppError::Internal(anyhow::anyhow!(other)),
        }
    }
}

/// Pull the first field error out of a PocketBase 400 body for display.
fn pb_message(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if let Some((field, err)) = v["data"].as_object().and_then(|m| m.iter().next()) {
        if let Some(msg) = err["message"].as_str() {
            return format!("{}: {msg}", field.replace('_', " "));
        }
    }
    "Some details weren't accepted. Check the form and try again.".into()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Domain(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
            AppError::BadRequest(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "We couldn't find that page or piece. It may have sold.".into()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::Unauthorized => {
                let mut r = (StatusCode::SEE_OTHER, [(header::LOCATION, "/login")]).into_response();
                r.headers_mut().insert("HX-Redirect", HeaderValue::from_static("/login"));
                return r;
            }
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong on our side. Please try again.".into())
            }
        };
        let msg = escape(&msg);
        // Capitalise domain messages for display.
        let msg = {
            let mut c = msg.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        };
        let body = format!(
            r#"<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Adire House</title><link rel="stylesheet" href="/static/app.css"><main class="wrap error-page"><div class="flash err" role="alert">{msg}</div><p><a href="/">Back to the gallery</a></p></main>"#
        );
        let mut r = (status, Html(body)).into_response();
        let h = r.headers_mut();
        h.insert("HX-Retarget", HeaderValue::from_static("#flash"));
        h.insert("HX-Reswap", HeaderValue::from_static("innerHTML"));
        h.insert("HX-Reselect", HeaderValue::from_static(".flash"));
        r
    }
}
