//! Data shapes produced by parsing the Alqanime site.
//!
//! These are pure data with no behaviour, so they live in the domain alongside
//! the other anime entities. The parser in `infrastructure` fills them in; the
//! port trait in `domain::repository::ports` hands them to the application layer.
//!
//! The derives are fully qualified on each struct (matching `entity::anime`), so that
//! moving these types here did not have to re-list them.

use super::anime::DetailGenre;

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqEpisode {
    pub episode: String,
    pub title: String,
    pub url: String,
    pub date: String,
    pub download_url: Option<String>,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqLink {
    pub name: String,
    pub url: String,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqDownloadItem {
    pub resolution: String,
    pub links: Vec<AlqLink>,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqRecommendation {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub status: String,
    pub r#type: String,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqDetailData {
    pub title: String,
    pub alternative_title: String,
    pub poster: String,
    pub poster2: String,
    pub r#type: String,
    pub release_date: String,
    pub status: String,
    pub synopsis: String,
    pub studio: String,
    pub genres: Vec<DetailGenre>,
    pub producers: Vec<String>,
    pub recommendations: Vec<AlqRecommendation>,
    pub batch: Vec<AlqDownloadItem>,
    pub ova: Vec<AlqDownloadItem>,
    pub downloads: Vec<AlqDownloadItem>,
    pub episodes: Vec<AlqEpisode>,
}
