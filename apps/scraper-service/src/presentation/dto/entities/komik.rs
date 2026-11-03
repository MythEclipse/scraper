//! Wire DTOs mirroring `domain::entity::komik`.
//!
//! Generated from the domain entities: derives, `#[serde(..)]` attributes and
//! field order are identical, so the JSON this service emits does not change.
//! Domain entities carry no serialization derives; the wire contract lives here.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct KomikGenreDto {
    pub name: String,
    pub slug: String,
    pub count: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ChapterDto {
    pub chapter: String,
    pub date: String,
    pub chapter_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ChapterDataDto {
    pub title: String,
    pub next_chapter_id: String,
    pub prev_chapter_id: String,
    pub list_chapter: String,
    pub images: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DetailDataDto {
    pub title: String,
    pub poster: String,
    pub description: String,
    pub status: String,
    pub r#type: String,
    pub release_date: String,
    pub author: String,
    pub total_chapter: String,
    pub updated_on: String,
    pub genres: Vec<String>,
    pub chapters: Vec<ChapterDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct KomikItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub chapter: String,
    pub score: String,
    pub r#type: String,
    pub komik_url: String,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<crate::domain::entity::komik::KomikGenre> for KomikGenreDto {
    fn from(entity: crate::domain::entity::komik::KomikGenre) -> Self {
        Self {
            name: entity.name,
            slug: entity.slug,
            count: entity.count,
        }
    }
}

impl From<crate::domain::entity::komik::Chapter> for ChapterDto {
    fn from(entity: crate::domain::entity::komik::Chapter) -> Self {
        Self {
            chapter: entity.chapter,
            date: entity.date,
            chapter_id: entity.chapter_id,
        }
    }
}

impl From<crate::domain::entity::komik::ChapterData> for ChapterDataDto {
    fn from(entity: crate::domain::entity::komik::ChapterData) -> Self {
        Self {
            title: entity.title,
            next_chapter_id: entity.next_chapter_id,
            prev_chapter_id: entity.prev_chapter_id,
            list_chapter: entity.list_chapter,
            images: entity.images,
        }
    }
}

impl From<crate::domain::entity::komik::DetailData> for DetailDataDto {
    fn from(entity: crate::domain::entity::komik::DetailData) -> Self {
        Self {
            title: entity.title,
            poster: entity.poster,
            description: entity.description,
            status: entity.status,
            r#type: entity.r#type,
            release_date: entity.release_date,
            author: entity.author,
            total_chapter: entity.total_chapter,
            updated_on: entity.updated_on,
            genres: entity.genres,
            chapters: entity.chapters.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::komik::KomikItem> for KomikItemDto {
    fn from(entity: crate::domain::entity::komik::KomikItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            chapter: entity.chapter,
            score: entity.score,
            r#type: entity.r#type,
            komik_url: entity.komik_url,
        }
    }
}
