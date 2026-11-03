//! Domain entities for anime data.
//!
//! Pure domain structs: no framework dependencies. Serialization derives stay
//! because the read-through cache in `application` stores these values as JSON;
//! the HTTP wire contract lives in `presentation::dto::entities`.
//! These represent the scraped anime data model regardless of source site.

use serde::{Deserialize, Serialize};

// ============================================================================
// PAGINATION
// ============================================================================

/// Common pagination structure shared across all endpoints
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pagination {
    pub current_page: u32,
    pub last_visible_page: u32,
    pub has_next_page: bool,
    pub next_page: Option<u32>,
    pub has_previous_page: bool,
    pub previous_page: Option<u32>,
}

impl Pagination {
    pub fn with_string_pages(
        current_page: u32,
        last_visible_page: u32,
        has_next_page: bool,
        next_page: Option<String>,
        has_previous_page: bool,
        previous_page: Option<String>,
    ) -> PaginationWithStringPages {
        PaginationWithStringPages {
            current_page,
            last_visible_page,
            has_next_page,
            next_page,
            has_previous_page,
            previous_page,
        }
    }
}

/// Pagination variant with string-based page numbers (used in search endpoints)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaginationWithStringPages {
    pub current_page: u32,
    pub last_visible_page: u32,
    pub has_next_page: bool,
    pub next_page: Option<String>,
    pub has_previous_page: bool,
    pub previous_page: Option<String>,
}

// ============================================================================
// OTakudesu (Anime Module) — Index Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OngoingAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub current_episode: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompleteAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode_count: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimeData {
    pub ongoing_anime: Vec<OngoingAnimeItem>,
    pub complete_anime: Vec<CompleteAnimeItem>,
}

// ============================================================================
// Genre Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Genre {
    pub name: String,
    pub slug: String,
    pub url: String,
}

// ============================================================================
// Otakudesu Detail Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetailGenre {
    pub name: String,
    pub slug: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EpisodeList {
    pub episode: String,
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Recommendation {
    pub title: String,
    pub slug: String,
    pub poster: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimeDetailData {
    pub title: String,
    pub alternative_title: String,
    pub poster: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub release_date: String,
    pub studio: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<DetailGenre>,
    pub synopsis: String,
    pub episode_lists: Vec<EpisodeList>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub batch: Vec<EpisodeList>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub producers: Vec<String>,
    pub recommendations: Vec<Recommendation>,
}

// ============================================================================
// Otakudesu List Page Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompleteAnimeListItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode_count: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OngoingAnimeListItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LatestAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode: String,
    pub anime_url: String,
    pub genres: Vec<String>,
    pub status: String,
    pub rating: String,
    pub description: String,
    pub r#type: String,
    pub season: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GenreAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode: String,
    pub score: String,
    pub status: String,
    pub anime_url: String,
}

// ============================================================================
// Otakudesu Full Episode Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimeInfo {
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EpisodeInfo {
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadLink {
    pub server: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnimeFullData {
    pub episode: String,
    pub episode_number: String,
    pub anime: AnimeInfo,
    pub has_next_episode: bool,
    pub next_episode: Option<EpisodeInfo>,
    pub has_previous_episode: bool,
    pub previous_episode: Option<EpisodeInfo>,
    pub stream_url: String,
    pub download_urls: std::collections::HashMap<String, Vec<DownloadLink>>,
    pub image_url: String,
}

// ============================================================================
// ALQanime (Anime2 Module) — Index Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Anime2Item {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub status: String,
    pub r#type: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Anime2Data {
    pub ongoing_anime: Vec<Anime2Item>,
    pub complete_anime: Vec<Anime2Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Anime2ItemDetail {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub poster2: String,
    pub synopsis: String,
    pub alternative_title: String,
    pub r#type: String,
    pub status: String,
    pub score: String,
    pub genres: Vec<DetailGenre>,
    pub episodes: Vec<EpisodeList>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OngoingAnimeItemWithScore {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FilterAnimeItem {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub status: String,
    pub r#type: String,
    pub anime_url: String,
}
