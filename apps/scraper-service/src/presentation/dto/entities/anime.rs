//! Wire DTOs mirroring `domain::entity::anime`.
//!
//! Generated from the domain entities: derives, `#[serde(..)]` attributes and
//! field order are identical, so the JSON this service emits does not change.
//! Domain entities carry no serialization derives; the wire contract lives here.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// PAGINATION
// ============================================================================

/// Common pagination structure shared across all endpoints
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct PaginationDto {
    pub current_page: u32,
    pub last_visible_page: u32,
    pub has_next_page: bool,
    pub next_page: Option<u32>,
    pub has_previous_page: bool,
    pub previous_page: Option<u32>,
}

/// PaginationDto variant with string-based page numbers (used in search endpoints)
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct PaginationWithStringPagesDto {
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

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct OngoingAnimeItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub current_episode: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct CompleteAnimeItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode_count: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AnimeDataDto {
    pub ongoing_anime: Vec<OngoingAnimeItemDto>,
    pub complete_anime: Vec<CompleteAnimeItemDto>,
}

// ============================================================================
// GenreDto Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GenreDto {
    pub name: String,
    pub slug: String,
    pub url: String,
}

// ============================================================================
// Otakudesu Detail Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DetailGenreDto {
    pub name: String,
    pub slug: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct EpisodeListDto {
    pub episode: String,
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct RecommendationDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AnimeDetailDataDto {
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
    pub genres: Vec<DetailGenreDto>,
    pub synopsis: String,
    pub episode_lists: Vec<EpisodeListDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub batch: Vec<EpisodeListDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub producers: Vec<String>,
    pub recommendations: Vec<RecommendationDto>,
}

// ============================================================================
// Otakudesu List Page Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct CompleteAnimeListItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode_count: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct OngoingAnimeListItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct LatestAnimeItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub episode: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct SearchAnimeItemDto {
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

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GenreAnimeItemDto {
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

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AnimeInfoDto {
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct EpisodeInfoDto {
    pub slug: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DownloadLinkDto {
    pub server: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AnimeFullDataDto {
    pub episode: String,
    pub episode_number: String,
    pub anime: AnimeInfoDto,
    pub has_next_episode: bool,
    pub next_episode: Option<EpisodeInfoDto>,
    pub has_previous_episode: bool,
    pub previous_episode: Option<EpisodeInfoDto>,
    pub stream_url: String,
    pub download_urls: std::collections::HashMap<String, Vec<DownloadLinkDto>>,
    pub image_url: String,
}

// ============================================================================
// ALQanime (Anime2 Module) — Index Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2ItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub status: String,
    pub r#type: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2DataDto {
    pub ongoing_anime: Vec<Anime2ItemDto>,
    pub complete_anime: Vec<Anime2ItemDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2ItemDetailDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub poster2: String,
    pub synopsis: String,
    pub alternative_title: String,
    pub r#type: String,
    pub status: String,
    pub score: String,
    pub genres: Vec<DetailGenreDto>,
    pub episodes: Vec<EpisodeListDto>,
    pub recommendations: Vec<RecommendationDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct OngoingAnimeItemWithScoreDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct FilterAnimeItemDto {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub score: String,
    pub status: String,
    pub r#type: String,
    pub anime_url: String,
}

// ============================================================================
// Conversions
// ============================================================================

impl From<crate::domain::entity::anime::Pagination> for PaginationDto {
    fn from(entity: crate::domain::entity::anime::Pagination) -> Self {
        Self {
            current_page: entity.current_page,
            last_visible_page: entity.last_visible_page,
            has_next_page: entity.has_next_page,
            next_page: entity.next_page,
            has_previous_page: entity.has_previous_page,
            previous_page: entity.previous_page,
        }
    }
}

impl From<crate::domain::entity::anime::PaginationWithStringPages>
    for PaginationWithStringPagesDto
{
    fn from(entity: crate::domain::entity::anime::PaginationWithStringPages) -> Self {
        Self {
            current_page: entity.current_page,
            last_visible_page: entity.last_visible_page,
            has_next_page: entity.has_next_page,
            next_page: entity.next_page,
            has_previous_page: entity.has_previous_page,
            previous_page: entity.previous_page,
        }
    }
}

impl From<crate::domain::entity::anime::OngoingAnimeItem> for OngoingAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::OngoingAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            current_episode: entity.current_episode,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::CompleteAnimeItem> for CompleteAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::CompleteAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            episode_count: entity.episode_count,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::AnimeData> for AnimeDataDto {
    fn from(entity: crate::domain::entity::anime::AnimeData) -> Self {
        Self {
            ongoing_anime: entity.ongoing_anime.into_iter().map(Into::into).collect(),
            complete_anime: entity.complete_anime.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::anime::Genre> for GenreDto {
    fn from(entity: crate::domain::entity::anime::Genre) -> Self {
        Self {
            name: entity.name,
            slug: entity.slug,
            url: entity.url,
        }
    }
}

impl From<crate::domain::entity::anime::DetailGenre> for DetailGenreDto {
    fn from(entity: crate::domain::entity::anime::DetailGenre) -> Self {
        Self {
            name: entity.name,
            slug: entity.slug,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::EpisodeList> for EpisodeListDto {
    fn from(entity: crate::domain::entity::anime::EpisodeList) -> Self {
        Self {
            episode: entity.episode,
            slug: entity.slug,
        }
    }
}

impl From<crate::domain::entity::anime::Recommendation> for RecommendationDto {
    fn from(entity: crate::domain::entity::anime::Recommendation) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            status: entity.status,
            r#type: entity.r#type,
        }
    }
}

impl From<crate::domain::entity::anime::AnimeDetailData> for AnimeDetailDataDto {
    fn from(entity: crate::domain::entity::anime::AnimeDetailData) -> Self {
        Self {
            title: entity.title,
            alternative_title: entity.alternative_title,
            poster: entity.poster,
            r#type: entity.r#type,
            status: entity.status,
            release_date: entity.release_date,
            studio: entity.studio,
            genres: entity.genres.into_iter().map(Into::into).collect(),
            synopsis: entity.synopsis,
            episode_lists: entity.episode_lists.into_iter().map(Into::into).collect(),
            batch: entity.batch.into_iter().map(Into::into).collect(),
            producers: entity.producers,
            recommendations: entity.recommendations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::anime::CompleteAnimeListItem> for CompleteAnimeListItemDto {
    fn from(entity: crate::domain::entity::anime::CompleteAnimeListItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            episode_count: entity.episode_count,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::OngoingAnimeListItem> for OngoingAnimeListItemDto {
    fn from(entity: crate::domain::entity::anime::OngoingAnimeListItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            score: entity.score,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::LatestAnimeItem> for LatestAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::LatestAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            episode: entity.episode,
            score: entity.score,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::SearchAnimeItem> for SearchAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::SearchAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            episode: entity.episode,
            anime_url: entity.anime_url,
            genres: entity.genres,
            status: entity.status,
            rating: entity.rating,
            description: entity.description,
            r#type: entity.r#type,
            season: entity.season,
        }
    }
}

impl From<crate::domain::entity::anime::GenreAnimeItem> for GenreAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::GenreAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            episode: entity.episode,
            score: entity.score,
            status: entity.status,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::AnimeInfo> for AnimeInfoDto {
    fn from(entity: crate::domain::entity::anime::AnimeInfo) -> Self {
        Self { slug: entity.slug }
    }
}

impl From<crate::domain::entity::anime::EpisodeInfo> for EpisodeInfoDto {
    fn from(entity: crate::domain::entity::anime::EpisodeInfo) -> Self {
        Self { slug: entity.slug }
    }
}

impl From<crate::domain::entity::anime::DownloadLink> for DownloadLinkDto {
    fn from(entity: crate::domain::entity::anime::DownloadLink) -> Self {
        Self {
            server: entity.server,
            url: entity.url,
        }
    }
}

impl From<crate::domain::entity::anime::AnimeFullData> for AnimeFullDataDto {
    fn from(entity: crate::domain::entity::anime::AnimeFullData) -> Self {
        Self {
            episode: entity.episode,
            episode_number: entity.episode_number,
            anime: entity.anime.into(),
            has_next_episode: entity.has_next_episode,
            next_episode: entity.next_episode.map(Into::into),
            has_previous_episode: entity.has_previous_episode,
            previous_episode: entity.previous_episode.map(Into::into),
            stream_url: entity.stream_url,
            download_urls: entity
                .download_urls
                .into_iter()
                .map(|(key, items)| (key, items.into_iter().map(Into::into).collect()))
                .collect(),
            image_url: entity.image_url,
        }
    }
}

impl From<crate::domain::entity::anime::Anime2Item> for Anime2ItemDto {
    fn from(entity: crate::domain::entity::anime::Anime2Item) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            status: entity.status,
            r#type: entity.r#type,
            score: entity.score,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::Anime2Data> for Anime2DataDto {
    fn from(entity: crate::domain::entity::anime::Anime2Data) -> Self {
        Self {
            ongoing_anime: entity.ongoing_anime.into_iter().map(Into::into).collect(),
            complete_anime: entity.complete_anime.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::anime::Anime2ItemDetail> for Anime2ItemDetailDto {
    fn from(entity: crate::domain::entity::anime::Anime2ItemDetail) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            poster2: entity.poster2,
            synopsis: entity.synopsis,
            alternative_title: entity.alternative_title,
            r#type: entity.r#type,
            status: entity.status,
            score: entity.score,
            genres: entity.genres.into_iter().map(Into::into).collect(),
            episodes: entity.episodes.into_iter().map(Into::into).collect(),
            recommendations: entity.recommendations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::domain::entity::anime::OngoingAnimeItemWithScore>
    for OngoingAnimeItemWithScoreDto
{
    fn from(entity: crate::domain::entity::anime::OngoingAnimeItemWithScore) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            score: entity.score,
            anime_url: entity.anime_url,
        }
    }
}

impl From<crate::domain::entity::anime::FilterAnimeItem> for FilterAnimeItemDto {
    fn from(entity: crate::domain::entity::anime::FilterAnimeItem) -> Self {
        Self {
            title: entity.title,
            slug: entity.slug,
            poster: entity.poster,
            score: entity.score,
            status: entity.status,
            r#type: entity.r#type,
            anime_url: entity.anime_url,
        }
    }
}
