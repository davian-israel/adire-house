use super::domain::{Artwork, ArtworkSearch, ArtworkStatus, Medium, NewArtwork};
use crate::infra::pocketbase::{q, ListQuery, PbError, PbResult, PocketBase};
use crate::shared::{SizeClass, UsdCents};
use crate::vendors::domain::Vendor;
use async_trait::async_trait;
use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::sync::Arc;

pub struct Upload {
    pub filename: String,
    pub content_type: String,
    pub bytes: Bytes,
}

#[async_trait]
pub trait CatalogueRepo: Send + Sync {
    /// Listed artworks from verified vendors, with their vendor.
    async fn search(&self, s: &ArtworkSearch) -> PbResult<Vec<(Artwork, Vendor)>>;
    async fn get(&self, id: &str) -> PbResult<(Artwork, Vendor)>;
    async fn by_vendor(&self, vendor_id: &str) -> PbResult<Vec<Artwork>>;
    async fn create(&self, a: NewArtwork, image: Option<Upload>) -> PbResult<Artwork>;
    async fn set_status(&self, id: &str, status: ArtworkStatus) -> PbResult<()>;
}

#[derive(Deserialize)]
struct Row {
    id: String,
    vendor: String,
    artist: String,
    title: String,
    medium: Medium,
    #[serde(default)]
    materials: String,
    #[serde(default)]
    dimensions: String,
    size_class: SizeClass,
    vendor_price_cents: i64,
    #[serde(default)]
    year: Option<i32>,
    status: ArtworkStatus,
    #[serde(default)]
    image: String,
    #[serde(default)]
    expand: Option<Expand>,
}

#[derive(Deserialize)]
struct Expand {
    vendor: Option<Vendor>,
}

impl Row {
    fn split(self) -> (Artwork, Option<Vendor>) {
        let vendor = self.expand.and_then(|e| e.vendor);
        let a = Artwork {
            id: self.id,
            vendor_id: self.vendor,
            artist: self.artist,
            title: self.title,
            medium: self.medium,
            materials: self.materials,
            dimensions: self.dimensions,
            size: self.size_class,
            vendor_price: UsdCents(self.vendor_price_cents),
            year: self.year.filter(|y| *y > 0),
            status: self.status,
            image: Some(self.image).filter(|s| !s.is_empty()),
        };
        (a, vendor)
    }
}

pub struct PbCatalogueRepo(pub Arc<PocketBase>);

#[async_trait]
impl CatalogueRepo for PbCatalogueRepo {
    async fn search(&self, s: &ArtworkSearch) -> PbResult<Vec<(Artwork, Vendor)>> {
        let mut f = vec!["status = 'listed'".to_string(), "vendor.status = 'verified'".to_string()];
        if let Some(t) = s.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let t = q(t);
            f.push(format!(
                "(title ~ {t} || artist ~ {t} || materials ~ {t} || vendor.name ~ {t} || vendor.city ~ {t} || vendor.country ~ {t})"
            ));
        }
        if let Some(m) = s.medium {
            f.push(format!("medium = {}", q(m.code())));
        }
        if let Some(c) = &s.country {
            f.push(format!("vendor.country = {}", q(c)));
        }
        if let Some(v) = &s.vendor_id {
            f.push(format!("vendor = {}", q(v)));
        }
        if let Some(a) = &s.artist {
            f.push(format!("artist = {}", q(a)));
        }
        let rows: Vec<Row> = self
            .0
            .list_all("artworks", &ListQuery::filter(f.join(" && ")).sort("-created").expand("vendor"))
            .await?;
        Ok(rows.into_iter().filter_map(|r| { let (a, v) = r.split(); v.map(|v| (a, v)) }).collect())
    }

    async fn get(&self, id: &str) -> PbResult<(Artwork, Vendor)> {
        let row: Row = self.0.get("artworks", id, Some("vendor")).await?;
        let (a, v) = row.split();
        Ok((a, v.ok_or(PbError::NotFound)?))
    }

    async fn by_vendor(&self, vendor_id: &str) -> PbResult<Vec<Artwork>> {
        let rows: Vec<Row> = self
            .0
            .list_all("artworks", &ListQuery::filter(format!("vendor = {}", q(vendor_id))).sort("-created"))
            .await?;
        Ok(rows.into_iter().map(|r| r.split().0).collect())
    }

    async fn create(&self, a: NewArtwork, image: Option<Upload>) -> PbResult<Artwork> {
        let build = || {
            let mut form = Form::new()
                .text("vendor", a.vendor_id.clone())
                .text("artist", a.artist.clone())
                .text("title", a.title.clone())
                .text("medium", a.medium.code())
                .text("materials", a.materials.clone())
                .text("dimensions", a.dimensions.clone())
                .text("size_class", a.size.code())
                .text("vendor_price_cents", a.vendor_price.0.to_string())
                .text("status", "listed");
            if let Some(y) = a.year {
                form = form.text("year", y.to_string());
            }
            if let Some(img) = &image {
                let part = Part::stream(img.bytes.clone())
                    .file_name(img.filename.clone())
                    .mime_str(&img.content_type)
                    .unwrap_or_else(|_| Part::stream(img.bytes.clone()).file_name(img.filename.clone()));
                form = form.part("image", part);
            }
            form
        };
        let row: Row = self.0.create_multipart("artworks", build).await?;
        Ok(row.split().0)
    }

    async fn set_status(&self, id: &str, status: ArtworkStatus) -> PbResult<()> {
        let _: serde_json::Value = self.0.update("artworks", id, &serde_json::json!({"status": status})).await?;
        Ok(())
    }
}
