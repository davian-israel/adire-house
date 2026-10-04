//! Thin PocketBase REST client. The Rust service authenticates as a superuser,
//! so every collection can stay locked (rules = null) and PocketBase never faces browsers.

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use reqwest::{multipart::Form, Method, RequestBuilder, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, thiserror::Error)]
pub enum PbError {
    #[error("record not found")]
    NotFound,
    #[error("invalid credentials")]
    BadCredentials,
    #[error("pocketbase rejected the request ({status}): {body}")]
    Rejected { status: StatusCode, body: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type PbResult<T> = std::result::Result<T, PbError>;

#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(rename = "totalPages")]
    pub total_pages: i64,
}

#[derive(Default, Clone)]
pub struct ListQuery {
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub expand: Option<String>,
    pub per_page: Option<u32>,
    pub page: Option<u32>,
}

impl ListQuery {
    pub fn filter(f: impl Into<String>) -> Self {
        Self { filter: Some(f.into()), ..Default::default() }
    }
    pub fn sort(mut self, s: &str) -> Self {
        self.sort = Some(s.into());
        self
    }
    pub fn expand(mut self, e: &str) -> Self {
        self.expand = Some(e.into());
        self
    }
}

/// Quote a user-supplied value for a PocketBase filter expression.
pub fn q(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// PocketBase record ids are 15 lowercase alphanumerics. Validate anything that came from a URL.
pub fn valid_id(id: &str) -> bool {
    id.len() == 15 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse<T> {
    pub token: String,
    pub record: T,
}

pub struct PocketBase {
    http: reqwest::Client,
    base: String,
    email: String,
    password: String,
    token: RwLock<Option<(String, Instant)>>,
}

const TOKEN_TTL: Duration = Duration::from_secs(50 * 60);

impl PocketBase {
    pub fn new(base: &str, email: &str, password: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            email: email.into(),
            password: password.into(),
            token: RwLock::new(None),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn superuser_token(&self, force: bool) -> PbResult<String> {
        if !force {
            if let Some((t, at)) = self.token.read().await.as_ref() {
                if at.elapsed() < TOKEN_TTL {
                    return Ok(t.clone());
                }
            }
        }
        let auth: AuthResponse<serde_json::Value> = self
            .http
            .post(self.url("/api/collections/_superusers/auth-with-password"))
            .json(&serde_json::json!({"identity": self.email, "password": self.password}))
            .send()
            .await
            .context("pocketbase unreachable")?
            .error_for_status()
            .context("superuser login failed: check PB_SUPERUSER_EMAIL / PB_SUPERUSER_PASSWORD")?
            .json()
            .await
            .context("bad auth response")?;
        *self.token.write().await = Some((auth.token.clone(), Instant::now()));
        Ok(auth.token)
    }

    /// Send an authenticated request, re-authenticating once if the token was rejected.
    async fn send(&self, build: impl Fn(&reqwest::Client) -> RequestBuilder) -> PbResult<reqwest::Response> {
        let mut force = false;
        loop {
            let token = self.superuser_token(force).await?;
            let res = build(&self.http)
                .header("Authorization", token)
                .send()
                .await
                .context("pocketbase unreachable")?;
            match res.status() {
                StatusCode::UNAUTHORIZED if !force => force = true,
                StatusCode::NOT_FOUND => return Err(PbError::NotFound),
                s if s.is_success() => return Ok(res),
                status => {
                    let body = res.text().await.unwrap_or_default();
                    return Err(PbError::Rejected { status, body });
                }
            }
        }
    }

    async fn json<T: DeserializeOwned>(res: reqwest::Response) -> PbResult<T> {
        Ok(res.json().await.context("unexpected pocketbase response shape")?)
    }

    pub async fn list<T: DeserializeOwned>(&self, coll: &str, q: &ListQuery) -> PbResult<Page<T>> {
        let mut params: Vec<(&str, String)> = vec![
            ("perPage", q.per_page.unwrap_or(200).to_string()),
            ("page", q.page.unwrap_or(1).to_string()),
            ("skipTotal", "0".into()),
        ];
        if let Some(f) = &q.filter {
            params.push(("filter", f.clone()));
        }
        if let Some(s) = &q.sort {
            params.push(("sort", s.clone()));
        }
        if let Some(e) = &q.expand {
            params.push(("expand", e.clone()));
        }
        let url = self.url(&format!("/api/collections/{coll}/records"));
        let res = self.send(|c| c.get(&url).query(&params)).await?;
        Self::json(res).await
    }

    /// Fetch every page. Fine for back-office sized collections.
    pub async fn list_all<T: DeserializeOwned>(&self, coll: &str, q: &ListQuery) -> PbResult<Vec<T>> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let mut pq = q.clone();
            pq.page = Some(page);
            pq.per_page = Some(500);
            let p: Page<T> = self.list(coll, &pq).await?;
            out.extend(p.items);
            if page >= p.total_pages as u32 || p.total_pages == 0 {
                break;
            }
            page += 1;
        }
        Ok(out)
    }

    pub async fn first<T: DeserializeOwned>(&self, coll: &str, q: &ListQuery) -> PbResult<Option<T>> {
        let mut pq = q.clone();
        pq.per_page = Some(1);
        Ok(self.list::<T>(coll, &pq).await?.items.into_iter().next())
    }

    pub async fn get<T: DeserializeOwned>(&self, coll: &str, id: &str, expand: Option<&str>) -> PbResult<T> {
        if !valid_id(id) {
            return Err(PbError::NotFound);
        }
        let url = self.url(&format!("/api/collections/{coll}/records/{id}"));
        let exp: Vec<(&str, &str)> = expand.map(|e| vec![("expand", e)]).unwrap_or_default();
        let res = self.send(|c| c.get(&url).query(&exp)).await?;
        Self::json(res).await
    }

    pub async fn create<T: DeserializeOwned, B: Serialize + ?Sized>(&self, coll: &str, body: &B) -> PbResult<T> {
        let url = self.url(&format!("/api/collections/{coll}/records"));
        let res = self.send(|c| c.post(&url).json(body)).await?;
        Self::json(res).await
    }

    /// Multipart create (for file fields). The form builder runs again if a retry is needed.
    pub async fn create_multipart<T: DeserializeOwned>(&self, coll: &str, form: impl Fn() -> Form) -> PbResult<T> {
        let url = self.url(&format!("/api/collections/{coll}/records"));
        let res = self.send(|c| c.post(&url).multipart(form())).await?;
        Self::json(res).await
    }

    pub async fn update<T: DeserializeOwned, B: Serialize + ?Sized>(&self, coll: &str, id: &str, body: &B) -> PbResult<T> {
        if !valid_id(id) {
            return Err(PbError::NotFound);
        }
        let url = self.url(&format!("/api/collections/{coll}/records/{id}"));
        let res = self.send(|c| c.request(Method::PATCH, &url).json(body)).await?;
        Self::json(res).await
    }

    pub async fn delete(&self, coll: &str, id: &str) -> PbResult<()> {
        if !valid_id(id) {
            return Err(PbError::NotFound);
        }
        let url = self.url(&format!("/api/collections/{coll}/records/{id}"));
        self.send(|c| c.delete(&url)).await?;
        Ok(())
    }

    /// Download a protected file. Returns (content-type, bytes).
    pub async fn file(&self, coll: &str, id: &str, filename: &str, thumb: Option<&str>) -> PbResult<(String, Bytes)> {
        if !valid_id(id) || filename.contains('/') || filename.contains("..") {
            return Err(PbError::NotFound);
        }
        // Files on locked collections need a short-lived file token.
        let ft: serde_json::Value = Self::json(self.send(|c| c.post(self.url("/api/files/token"))).await?).await?;
        let token = ft["token"].as_str().ok_or_else(|| anyhow!("no file token"))?.to_string();
        let url = self.url(&format!("/api/files/{coll}/{id}/{filename}"));
        let mut params = vec![("token", token)];
        if let Some(t) = thumb {
            params.push(("thumb", t.to_string()));
        }
        let res = self.send(|c| c.get(&url).query(&params)).await?;
        let ct = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        Ok((ct, res.bytes().await.context("file download failed")?))
    }

    /// Password login against the `users` auth collection (admins and vendors).
    pub async fn auth_user<T: DeserializeOwned>(&self, email: &str, password: &str) -> PbResult<T> {
        let res = self
            .http
            .post(self.url("/api/collections/users/auth-with-password"))
            .json(&serde_json::json!({"identity": email, "password": password}))
            .send()
            .await
            .context("pocketbase unreachable")?;
        match res.status() {
            s if s.is_success() => Ok(Self::json::<AuthResponse<T>>(res).await?.record),
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => Err(PbError::BadCredentials),
            status => Err(PbError::Rejected { status, body: res.text().await.unwrap_or_default() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_filter_values() {
        assert_eq!(q("Awa's"), "'Awa\\'s'");
        assert_eq!(q("a\\' || 1=1"), "'a\\\\\\' || 1=1'");
    }

    #[test]
    fn validates_ids() {
        assert!(valid_id("ydp75mc5kyonm06"));
        assert!(!valid_id("../etc/passwd00"));
        assert!(!valid_id("short"));
    }
}
