use anyhow::{bail, Context, Result};

pub struct Config {
    pub bind: String,
    pub pb_url: String,
    pub pb_email: String,
    pub pb_password: String,
    /// At least 64 bytes; signs session cookies.
    pub session_secret: String,
    pub secure_cookies: bool,
}

fn var(k: &str) -> Result<String> {
    std::env::var(k).with_context(|| format!("missing env var {k}"))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let c = Config {
            bind: std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            pb_url: std::env::var("PB_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".into()),
            pb_email: var("PB_SUPERUSER_EMAIL")?,
            pb_password: var("PB_SUPERUSER_PASSWORD")?,
            session_secret: var("SESSION_SECRET")?,
            secure_cookies: std::env::var("SECURE_COOKIES").map(|v| v == "true").unwrap_or(false),
        };
        if c.session_secret.len() < 64 {
            bail!("SESSION_SECRET must be at least 64 characters (try: openssl rand -hex 32)");
        }
        Ok(c)
    }
}
