//! Anime2 (Alqanime) application use cases.
//!
//! Orchestrates repository fetching, caching, and image poster processing.
//!
//! TODO: Move parsers from `crate::modules::anime2::parser` to
//!       `crate::infrastructure::repository::parsers::alqanime_parser`.
//! TODO: Move response DTOs to `crate::presentation::dto::anime2`.
//! TODO: Once parsers return domain types, replace shared types with
//!       `crate::domain::entity::anime::{GenreAnimeItem, SearchAnimeItem, LatestAnimeItem}`.

use crate::domain::error::*;
use crate::domain::repository::{AlqanimeAnimeRepository, CachePort};

use crate::domain::entity::anime::{
    CompleteAnimeItem, FilterAnimeItem, Genre, GenreAnimeItem, LatestAnimeItem,
    OngoingAnimeItemWithScore, Pagination, SearchAnimeItem,
};

// Re-export types for handlers to use
pub use crate::domain::entity::alqanime::*;
pub use crate::domain::entity::anime::{
    CompleteAnimeItem as Anime2CompleteAnimeItem, GenreAnimeItem as Anime2GenreItem,
    LatestAnimeItem as Anime2LatestItem, OngoingAnimeItemWithScore as Anime2OngoingItem,
    SearchAnimeItem as Anime2SearchItem,
};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Response types (will move to presentation::dto::anime2)

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2Item {
    pub title: String,
    pub slug: String,
    pub poster: String,
    pub status: String,
    pub r#type: String,
    pub score: String,
    pub anime_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2Data {
    pub ongoing_anime: Vec<Anime2Item>,
    pub complete_anime: Vec<Anime2Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Anime2Response {
    pub status: String,
    pub data: Anime2Data,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GenresResponse {
    pub status: String,
    pub data: Vec<Genre>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct FiltersApplied {
    pub genre: Option<String>,
    pub status: Option<String>,
    pub r#type: Option<String>,
    pub order: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct FilterResponse {
    pub success: bool,
    pub data: Vec<FilterAnimeItem>,
    pub pagination: Pagination,
    pub filters_applied: FiltersApplied,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DetailResponse {
    pub status: String,
    pub data: AlqDetailData,
}

const INDEX_CACHE_TTL: u64 = 300;
const GENRE_LIST_CACHE_TTL: u64 = 3600;
const FILTER_CACHE_TTL: u64 = 300;
const DETAIL_CACHE_TTL: u64 = 300;
const GENRE_CACHE_TTL: u64 = 300;
const SEARCH_CACHE_TTL: u64 = 300;
const LATEST_CACHE_TTL: u64 = 120;
const ONGOING_CACHE_TTL: u64 = 300;
const COMPLETE_CACHE_TTL: u64 = 300;

// ============================================================================
// Use case struct
// ============================================================================

pub struct Anime2UseCases<R: AlqanimeAnimeRepository, C: CachePort> {
    repository: R,
    cache: C,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: AlqanimeAnimeRepository, C: CachePort>(
    repository: R,
    cache: C,
) -> Anime2UseCases<R, C> {
    Anime2UseCases { repository, cache }
}

impl<R: AlqanimeAnimeRepository, C: CachePort> Anime2UseCases<R, C> {
    pub async fn index(&self) -> Result<Anime2Response, DomainError> {
        self.cache
            .get_or_set("anime2:index", INDEX_CACHE_TTL, || async {
                let ongoing_items = self
                    .repository
                    .fetch_index_ongoing()
                    .await
                    .map_err(|e| e.to_string())?;
                let complete_items = self
                    .repository
                    .fetch_index_complete()
                    .await
                    .map_err(|e| e.to_string())?;

                let ongoing: Vec<Anime2Item> = ongoing_items
                    .into_iter()
                    .map(|item| Anime2Item {
                        title: item.title,
                        slug: item.slug,
                        poster: item.poster,
                        status: String::new(),
                        r#type: String::new(),
                        score: item.current_episode,
                        anime_url: item.anime_url,
                    })
                    .collect();

                let complete: Vec<Anime2Item> = complete_items
                    .into_iter()
                    .map(|item| Anime2Item {
                        title: item.title,
                        slug: item.slug,
                        poster: item.poster,
                        status: String::new(),
                        r#type: String::new(),
                        score: item.episode_count,
                        anime_url: item.anime_url,
                    })
                    .collect();

                Ok(Anime2Response {
                    status: "Ok".to_string(),
                    data: Anime2Data {
                        ongoing_anime: ongoing,
                        complete_anime: complete,
                    },
                })
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn genre_list(&self) -> Result<GenresResponse, DomainError> {
        self.cache
            .get_or_set("anime2:genres:list:v3", GENRE_LIST_CACHE_TTL, || async {
                let genres = self
                    .repository
                    .fetch_genres()
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(GenresResponse {
                    status: "Ok".to_string(),
                    data: genres,
                })
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn filter(
        &self,
        page: u32,
        genre: Option<String>,
        status: Option<String>,
        anime_type: Option<String>,
        order: String,
    ) -> Result<FilterResponse, DomainError> {
        let cache_key = format!(
            "anime2:filter:{}:{:?}:{:?}:{:?}:{}",
            page, genre, status, anime_type, order
        );

        self.cache
            .get_or_set(&cache_key, FILTER_CACHE_TTL, || async {
                let (data, pagination) = self
                    .repository
                    .fetch_filter(
                        page,
                        genre.as_deref().unwrap_or_default(),
                        status.as_deref().unwrap_or_default(),
                        anime_type.as_deref().unwrap_or_default(),
                        &order,
                    )
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(FilterResponse {
                    success: true,
                    data,
                    pagination,
                    filters_applied: FiltersApplied {
                        genre: genre.clone(),
                        status: status.clone(),
                        r#type: anime_type.clone(),
                        order: order.clone(),
                    },
                    status: "Ok".to_string(),
                })
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn detail(&self, slug: String) -> Result<DetailResponse, DomainError> {
        let cache_key = format!("anime2:detail:{}", slug);

        self.cache
            .get_or_set(&cache_key, DETAIL_CACHE_TTL, || async {
                let mut data = self
                    .repository
                    .fetch_detail(&slug)
                    .await
                    .map_err(|e| e.to_string())?;

                // Resolve download URLs for the five most recent episodes. A
                // failure here is not fatal: the detail page is still useful
                // without a direct download link on every episode.
                let episode_urls: Vec<String> = data
                    .episodes
                    .iter()
                    .take(5)
                    .map(|ep| ep.url.clone())
                    .collect();

                let downloads = futures::future::join_all(
                    episode_urls
                        .iter()
                        .map(|url| self.repository.fetch_episode_download(url)),
                )
                .await;

                for (episode, download) in data.episodes.iter_mut().take(5).zip(downloads) {
                    if let Ok(url) = download {
                        episode.download_url = Some(url);
                    }
                }

                Ok(DetailResponse {
                    status: "Ok".to_string(),
                    data,
                })
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn genre_slug(
        &self,
        genre_slug: String,
        page: u32,
    ) -> Result<Vec<GenreAnimeItem>, DomainError> {
        let cache_key = format!("anime2:genre:{}:{}", genre_slug, page);

        self.cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                let (data, _pagination) = self
                    .repository
                    .fetch_genre_page(&genre_slug, page)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn search(
        &self,
        query: String,
        page: u32,
    ) -> Result<Vec<SearchAnimeItem>, DomainError> {
        let cache_key = format!("anime2:search:{}:{}", query, page);

        self.cache
            .get_or_set(&cache_key, SEARCH_CACHE_TTL, || async {
                let (data, _pagination) = self
                    .repository
                    .fetch_search_page(&query, page)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn latest(&self, page: u32) -> Result<Vec<LatestAnimeItem>, DomainError> {
        let cache_key = format!("anime2:latest:{}", page);

        self.cache
            .get_or_set(&cache_key, LATEST_CACHE_TTL, || async {
                let (data, _pagination) = self
                    .repository
                    .fetch_latest_page(page)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn ongoing_anime(
        &self,
        page: u32,
    ) -> Result<Vec<OngoingAnimeItemWithScore>, DomainError> {
        let cache_key = format!("anime2:ongoing:{}", page);

        self.cache
            .get_or_set(&cache_key, ONGOING_CACHE_TTL, || async {
                let (data, _pagination) = self
                    .repository
                    .fetch_ongoing_page(page)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn complete_anime(&self, page: u32) -> Result<Vec<CompleteAnimeItem>, DomainError> {
        let cache_key = format!("anime2:complete:{}", page);

        self.cache
            .get_or_set(&cache_key, COMPLETE_CACHE_TTL, || async {
                let (data, _pagination) = self
                    .repository
                    .fetch_complete_page(page)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }
}
