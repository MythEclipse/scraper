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
use crate::domain::repository::ScrapingRepository;
use crate::infrastructure::cache::redis::Cache;
use crate::infrastructure::repository::AlqanimeRepository;

use crate::infrastructure::repository::parsers::alqanime_parser as parser;

use crate::domain::entity::anime::{
    CompleteAnimeItem, FilterAnimeItem, Genre, GenreAnimeItem, LatestAnimeItem,
    OngoingAnimeItemWithScore, Pagination, SearchAnimeItem,
};

// Re-export types for handlers to use
pub use crate::domain::entity::anime::{
    CompleteAnimeItem as Anime2CompleteAnimeItem, GenreAnimeItem as Anime2GenreItem,
    LatestAnimeItem as Anime2LatestItem, OngoingAnimeItemWithScore as Anime2OngoingItem,
    SearchAnimeItem as Anime2SearchItem,
};
pub use crate::infrastructure::repository::parsers::alqanime_parser::{
    AlqDetailData, AlqDownloadItem, AlqEpisode, AlqLink, AlqRecommendation,
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

pub struct Anime2UseCases {
    repository: AlqanimeRepository,
}

impl Anime2UseCases {
    pub fn new(repository: AlqanimeRepository) -> Self {
        Self { repository }
    }

    fn cache(&self) -> Cache {
        Cache::new()
    }

    pub async fn index(&self) -> Result<Anime2Response, DomainError> {
        self.cache()
            .get_or_set("anime2:index", INDEX_CACHE_TTL, || async {
                let ongoing_html = self
                    .repository
                    .fetch_html(&self.repository.index_ongoing_url())
                    .await
                    .map_err(|e| e.to_string())?;
                let complete_html = self
                    .repository
                    .fetch_html(&self.repository.index_complete_url())
                    .await
                    .map_err(|e| e.to_string())?;

                let data = tokio::task::spawn_blocking(move || {
                    Ok::<_, String>((
                        parser::parse_ongoing_anime(&ongoing_html).map_err(|e| e.to_string())?,
                        parser::parse_complete_anime(&complete_html).map_err(|e| e.to_string())?,
                    ))
                })
                .await
                .map_err(|e| e.to_string())??;
                let ongoing: Vec<Anime2Item> = data
                    .0
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

                let complete: Vec<Anime2Item> = data
                    .1
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
        self.cache()
            .get_or_set("anime2:genres:list:v3", GENRE_LIST_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.genre_list_url())
                    .await
                    .map_err(|e| e.to_string())?;

                let genres = tokio::task::spawn_blocking(move || {
                    parser::parse_genres(&html).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                Ok(GenresResponse {
                    status: "Ok".to_string(),
                    data: genres,
                })
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

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
        let genre_clone = genre.clone();
        let status_clone = status.clone();
        let anime_type_clone = anime_type.clone();

        self.cache()
            .get_or_set(&cache_key, FILTER_CACHE_TTL, || async {
                let mut url = self.repository.filter_url(page, &order);

                if let Some(g) = &genre {
                    for genre_item in g.split(',') {
                        url.push_str(&format!("&genre[]={}", genre_item.trim()));
                    }
                }
                if let Some(s) = &status {
                    url.push_str(&format!("&status={}", s));
                }
                if let Some(t) = &anime_type {
                    url.push_str(&format!("&type={}", t));
                }

                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_filter_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                Ok(FilterResponse {
                    success: true,
                    data,
                    pagination,
                    filters_applied: FiltersApplied {
                        genre: genre_clone,
                        status: status_clone,
                        r#type: anime_type_clone,
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

        self.cache()
            .get_or_set(&cache_key, DETAIL_CACHE_TTL, || async {
                let detail_url = self.repository.detail_url(&slug);
                let detail_html = self
                    .repository
                    .fetch_html(&detail_url)
                    .await
                    .map_err(|e| e.to_string())?;

                let mut data = tokio::task::spawn_blocking(move || {
                    parser::parse_anime_detail(&detail_html).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                // Fetch download URLs for the latest episodes
                let episodes = &mut data.episodes;
                if !episodes.is_empty() {
                    // Limit to latest 5 episodes to keep things fast
                    let latest: Vec<_> = episodes
                        .iter_mut()
                        .take(5)
                        .map(|ep| ep.url.clone())
                        .collect();

                    let results: Vec<_> = futures::future::join_all(
                        latest.iter().map(|url| self.repository.fetch_html(url)),
                    )
                    .await;

                    for (i, result) in results.into_iter().enumerate() {
                        if let Ok(html) = result {
                            if let Ok(Some(dl_url)) = tokio::task::spawn_blocking(move || {
                                parser::parse_episode_download(&html)
                            })
                            .await
                            .unwrap_or(Ok(None))
                            {
                                if let Some(ep) = episodes.get_mut(i) {
                                    ep.download_url = Some(dl_url);
                                }
                            }
                        }
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

        self.cache()
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.genre_page_url(&genre_slug, page))
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, _pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_genre_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

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

        self.cache()
            .get_or_set(&cache_key, SEARCH_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.search_url(&query, page))
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, _pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_search_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn latest(&self, page: u32) -> Result<Vec<LatestAnimeItem>, DomainError> {
        let cache_key = format!("anime2:latest:{}", page);

        self.cache()
            .get_or_set(&cache_key, LATEST_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.latest_url(page))
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, _pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_latest_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

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

        self.cache()
            .get_or_set(&cache_key, ONGOING_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.ongoing_url(page))
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, _pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_ongoing_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn complete_anime(&self, page: u32) -> Result<Vec<CompleteAnimeItem>, DomainError> {
        let cache_key = format!("anime2:complete:{}", page);

        self.cache()
            .get_or_set(&cache_key, COMPLETE_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.complete_url(page))
                    .await
                    .map_err(|e| e.to_string())?;
                let (data, _pagination) = tokio::task::spawn_blocking(move || {
                    parser::parse_complete_page(&html, page).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }
}
