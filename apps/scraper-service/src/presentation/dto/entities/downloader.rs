//! Wire DTOs mirroring `domain::entity::downloader`.
//!
//! Generated from the domain entities: derives, `#[serde(..)]` attributes and
//! field order are identical, so the JSON this service emits does not change.
//! Domain entities carry no serialization derives; the wire contract lives here.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Media type classification for a download link.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema)]
pub enum MediaTypeDto {
    Video,
    Audio,
    Image,
    File,
}

/// The status of a download attempt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatusDto {
    Success,
    /// Provider returned a known error (bad URL, rate limited, etc.)
    Error,
    /// All providers failed
    Failed,
}

/// A single downloadable media item (a video/audio/image/file link).
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct MediaItemDto {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<MediaTypeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_height: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Unified download result returned by every downloader endpoint.
///
/// This is intentionally flexible — different platforms return different
/// metadata (some have duration, some don't), so most fields are `Option`.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DownloadResultDto {
    pub status: DownloadStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaItemDto>,
    /// Provider that supplied the result (e.g. "snapsave", "tikwm", "savetube")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<crate::domain::entity::downloader::MediaItem> for MediaItemDto {
    fn from(entity: crate::domain::entity::downloader::MediaItem) -> Self {
        Self {
            url: entity.url,
            quality: entity.quality,
            file_type: entity.file_type.map(Into::into),
            extension: entity.extension,
            thumbnail: entity.thumbnail,
            file_size: entity.file_size,
            size_bytes: entity.size_bytes,
            frame_width: entity.frame_width,
            frame_height: entity.frame_height,
            note: entity.note,
        }
    }
}

impl From<crate::domain::entity::downloader::DownloadResult> for DownloadResultDto {
    fn from(entity: crate::domain::entity::downloader::DownloadResult) -> Self {
        Self {
            status: entity.status.into(),
            message: entity.message,
            title: entity.title,
            author: entity.author,
            thumbnail: entity.thumbnail,
            duration: entity.duration,
            description: entity.description,
            media: entity.media.into_iter().map(Into::into).collect(),
            provider: entity.provider,
        }
    }
}

impl From<crate::domain::entity::downloader::MediaType> for MediaTypeDto {
    fn from(value: crate::domain::entity::downloader::MediaType) -> Self {
        match value {
            crate::domain::entity::downloader::MediaType::Video => MediaTypeDto::Video,
            crate::domain::entity::downloader::MediaType::Audio => MediaTypeDto::Audio,
            crate::domain::entity::downloader::MediaType::Image => MediaTypeDto::Image,
            crate::domain::entity::downloader::MediaType::File => MediaTypeDto::File,
        }
    }
}

impl From<crate::domain::entity::downloader::DownloadStatus> for DownloadStatusDto {
    fn from(value: crate::domain::entity::downloader::DownloadStatus) -> Self {
        match value {
            crate::domain::entity::downloader::DownloadStatus::Success => {
                DownloadStatusDto::Success
            }
            crate::domain::entity::downloader::DownloadStatus::Error => DownloadStatusDto::Error,
            crate::domain::entity::downloader::DownloadStatus::Failed => DownloadStatusDto::Failed,
        }
    }
}
