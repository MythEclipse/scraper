//! Domain entities for media downloader data.
//!
//! Pure domain structs representing download results from various social
//! platforms and file hosts. No framework dependencies; the wire contract lives
//! in `presentation::dto::entities::downloader`.

use serde::{Deserialize, Serialize};

/// Media type classification for a download link.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
    Image,
    File,
}

/// The status of a download attempt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Success,
    /// Provider returned a known error (bad URL, rate limited, etc.)
    Error,
    /// All providers failed
    Failed,
}

/// A single downloadable media item (a video/audio/image/file link).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MediaItem {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<MediaType>,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadResult {
    pub status: DownloadStatus,
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
    pub media: Vec<MediaItem>,
    /// Provider that supplied the result (e.g. "snapsave", "tikwm", "savetube")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl DownloadResult {
    pub fn success(title: Option<String>) -> Self {
        Self {
            status: DownloadStatus::Success,
            message: None,
            title,
            author: None,
            thumbnail: None,
            duration: None,
            description: None,
            media: Vec::new(),
            provider: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: DownloadStatus::Error,
            message: Some(message.into()),
            title: None,
            author: None,
            thumbnail: None,
            duration: None,
            description: None,
            media: Vec::new(),
            provider: None,
        }
    }

    pub fn add_media(mut self, item: MediaItem) -> Self {
        self.media.push(item);
        self
    }
}
