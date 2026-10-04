//! Customers context (supporting): favourite shops and artists.
//! Visitors are identified by an anonymous customer key cookie, so saving works without an account.
//! When buyer accounts are added, merge the key's favourites into the account on sign-in.

use crate::infra::pocketbase::{q, ListQuery, PbResult, PocketBase};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FavouriteKind {
    Vendor,
    Artist,
}

impl FavouriteKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "vendor" => Some(Self::Vendor),
            "artist" => Some(Self::Artist),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Vendor => "vendor",
            Self::Artist => "artist",
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Favourites {
    pub vendors: HashSet<String>,
    pub artists: HashSet<String>,
}

impl Favourites {
    pub fn has(&self, kind: FavouriteKind, target: &str) -> bool {
        match kind {
            FavouriteKind::Vendor => self.vendors.contains(target),
            FavouriteKind::Artist => self.artists.contains(target),
        }
    }
    pub fn count(&self) -> usize {
        self.vendors.len() + self.artists.len()
    }
}

#[async_trait]
pub trait FavouritesRepo: Send + Sync {
    async fn load(&self, customer_key: &str) -> PbResult<Favourites>;
    /// Returns true if the target is now saved.
    async fn toggle(&self, customer_key: &str, kind: FavouriteKind, target: &str) -> PbResult<bool>;
}

#[derive(Deserialize)]
struct Row {
    id: String,
    kind: FavouriteKind,
    target: String,
}

pub struct PbFavouritesRepo(pub Arc<PocketBase>);

#[async_trait]
impl FavouritesRepo for PbFavouritesRepo {
    async fn load(&self, key: &str) -> PbResult<Favourites> {
        let rows: Vec<Row> = self.0.list_all("favourites", &ListQuery::filter(format!("customer_key = {}", q(key)))).await?;
        let mut f = Favourites::default();
        for r in rows {
            match r.kind {
                FavouriteKind::Vendor => f.vendors.insert(r.target),
                FavouriteKind::Artist => f.artists.insert(r.target),
            };
        }
        Ok(f)
    }

    async fn toggle(&self, key: &str, kind: FavouriteKind, target: &str) -> PbResult<bool> {
        let filter = format!("customer_key = {} && kind = {} && target = {}", q(key), q(kind.code()), q(target));
        if let Some(existing) = self.0.first::<Row>("favourites", &ListQuery::filter(filter)).await? {
            self.0.delete("favourites", &existing.id).await?;
            Ok(false)
        } else {
            let _: serde_json::Value = self
                .0
                .create("favourites", &serde_json::json!({"customer_key": key, "kind": kind, "target": target}))
                .await?;
            Ok(true)
        }
    }
}
