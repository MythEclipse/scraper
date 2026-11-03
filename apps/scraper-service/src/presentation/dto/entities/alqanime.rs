//! Wire DTOs mirroring `domain::entity::alqanime`.
//!
//! Generated from the domain entities: derives, `#[serde(..)]` attributes and
//! field order are identical, so the JSON this service emits does not change.
//! Domain entities carry no serialization derives; the wire contract lives here.

use super::anime::DetailGenreDto;

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqEpisodeDto {
    pub episode: String,
    pub title: String,
    pub url: String,
    pub date: String,
    pub download_url: Option<String>,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqLinkDto {
    pub name: String,
    pub url: String,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqDownloadItemDto {
    pub resolution: String,
    pub links: Vec<AlqLinkDto>,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqRecommendationDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub status: String,
    pub r#type: String,
}
#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Debug, Clone)]
pub struct AlqDetailDataDto {
    pub title: String,
    pub alternative_title: String,
    pub poster: String,
    pub poster2: String,
    pub r#type: String,
    pub release_date: String,
    pub status: String,
    pub synopsis: String,
    pub studio: String,
    pub genres: Vec<DetailGenreDto>,
    pub producers: Vec<String>,
    pub recommendations: Vec<AlqRecommendationDto>,
    pub batch: Vec<AlqDownloadItemDto>,
    pub ova: Vec<AlqDownloadItemDto>,
    pub downloads: Vec<AlqDownloadItemDto>,
    pub episodes: Vec<AlqEpisodeDto>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<crate::domain::entity::alqanime::AlqEpisode> for AlqEpisodeDto {
    fn from(entity: crate::domain::entity::alqanime::AlqEpisode) -> Self {
        Self {
            episode: entity.episode,
            title: entity.title,
            url: entity.url,
            date: entity.date,
            download_url: entity.download_url,
        }
    }
}

impl From<crate::domain::entity::alqanime::AlqLink> for AlqLinkDto {
    fn from(entity: crate::domain::entity::alqanime::AlqLink) -> Self {
        Self {
            name: entity.name,
            url: entity.url,
        }
    }
}

impl From<crate::domain::entity::alqanime::AlqDownloadItem> for AlqDownloadItemDto {
    fn from(entity: crate::domain::entity::alqanime::AlqDownloadItem) -> Self {
        Self {
            resolution: entity.resolution,
            links: entity.links.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::alqanime::AlqRecommendation> for AlqRecommendationDto {
    fn from(entity: crate::domain::entity::alqanime::AlqRecommendation) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            status: entity.status,
            r#type: entity.r#type,
        }
    }
}

impl From<crate::domain::entity::alqanime::AlqDetailData> for AlqDetailDataDto {
    fn from(entity: crate::domain::entity::alqanime::AlqDetailData) -> Self {
        Self {
            title: entity.title,
            alternative_title: entity.alternative_title,
            poster: entity.poster,
            poster2: entity.poster2,
            r#type: entity.r#type,
            release_date: entity.release_date,
            status: entity.status,
            synopsis: entity.synopsis,
            studio: entity.studio,
            genres: entity.genres.into_iter().map(Into::into).collect(),
            producers: entity.producers,
            recommendations: entity.recommendations.into_iter().map(Into::into).collect(),
            batch: entity.batch.into_iter().map(Into::into).collect(),
            ova: entity.ova.into_iter().map(Into::into).collect(),
            downloads: entity.downloads.into_iter().map(Into::into).collect(),
            episodes: entity.episodes.into_iter().map(Into::into).collect(),
        }
    }
}
