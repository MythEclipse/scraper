//! Anime (Otakudesu) application use cases.
//!
//! Orchestrates repository fetching, caching, and image poster processing.
//! Returns pure domain types — no DTOs.

use crate::domain::entity::anime::*;
use crate::domain::error::*;
use crate::infrastructure::cache::redis::Cache;
use crate::infrastructure::repository::OtakudesuRepository;

const INDEX_CACHE_TTL: u64 = 10;
const GENRE_LIST_CACHE_TTL: u64 = 3600;
const DEFAULT_CACHE_TTL: u64 = 300;

pub struct AnimeUseCases {
    repository: OtakudesuRepository,
}

impl AnimeUseCases {
    pub fn new(repository: OtakudesuRepository) -> Self {
        Self { repository }
    }

    fn cache(&self) -> Cache {
        Cache::new()
    }

    pub async fn get_anime_index(&self) -> Result<AnimeData, DomainError> {
        self.cache()
            .get_or_set("anime:index:v2", INDEX_CACHE_TTL, || async {
                let data = self
                    .repository
                    .fetch_anime_index()
                    .await
                    .map_err(|e| e.to_string())?;

                if data.ongoing_anime.is_empty() && data.complete_anime.is_empty() {
                    return Err("Empty anime index — refusing to cache".to_string());
                }

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_genres(&self) -> Result<Vec<Genre>, DomainError> {
        self.cache()
            .get_or_set("anime:genres:list", GENRE_LIST_CACHE_TTL, || async {
                self.repository
                    .fetch_genres()
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_anime_detail(&self, slug: String) -> Result<AnimeDetailData, DomainError> {
        let cache_key = format!("anime:detail:{}", slug);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                let data = self
                    .repository
                    .fetch_anime_detail(&slug)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_complete_anime_page(
        &self,
        slug: String,
    ) -> Result<(Vec<CompleteAnimeListItem>, Pagination), DomainError> {
        let cache_key = format!("anime:complete:{}", slug);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_complete_anime_page(&slug)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_ongoing_anime_page(
        &self,
        slug: String,
    ) -> Result<(Vec<OngoingAnimeListItem>, Pagination), DomainError> {
        let cache_key = format!("anime:ongoing:{}", slug);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_ongoing_anime_page(&slug)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_latest_anime_page(
        &self,
        slug: String,
    ) -> Result<(Vec<LatestAnimeItem>, Pagination), DomainError> {
        let cache_key = format!("anime:latest:{}", slug);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_latest_anime_page(&slug)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_search_anime_page(
        &self,
        slug: String,
        page: String,
    ) -> Result<(Vec<SearchAnimeItem>, Pagination), DomainError> {
        let cache_key = format!("anime:search:{}:{}", slug, page);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_search_anime_page(&slug, &page)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_genre_anime_page(
        &self,
        genre_slug: String,
        page: String,
    ) -> Result<(Vec<GenreAnimeItem>, Pagination), DomainError> {
        let cache_key = format!("anime:genre:{}:{}", genre_slug, page);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_genre_anime_page(&genre_slug, &page)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn get_anime_full(&self, slug: String) -> Result<AnimeFullData, DomainError> {
        let cache_key = format!("anime:full:{}", slug);
        self.cache()
            .get_or_set(&cache_key, DEFAULT_CACHE_TTL, || async {
                self.repository
                    .fetch_anime_full(&slug)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }
}
