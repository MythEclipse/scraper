//! Komik application use cases.
//!
//! Orchestrates repository fetching, caching, and image poster processing.
//!
//! TODO: Move parsers from `crate::modules::komik::parser` to
//!       `crate::infrastructure::repository::parsers::komik_parser`.
//! TODO: Move response DTOs to `crate::presentation::dto::komik`.

use crate::domain::entity::anime::Pagination;
use crate::domain::entity::komik::{ChapterData, DetailData, KomikGenre, KomikItem};
use crate::domain::error::*;
use crate::domain::repository::ScrapingRepository;
use crate::infrastructure::cache::redis::Cache;
use crate::infrastructure::repository::KomikRepository;

use crate::infrastructure::repository::parsers::komik_parser as parser;

const GENRE_LIST_CACHE_TTL: u64 = 3600;
const GENRE_CACHE_TTL: u64 = 300;
const DETAIL_CACHE_TTL: u64 = 300;
const CHAPTER_CACHE_TTL: u64 = 300;
const SEARCH_CACHE_TTL: u64 = 300;

// ============================================================================
// Use case struct
// ============================================================================

pub struct KomikUseCases {
    repository: KomikRepository,
}

impl KomikUseCases {
    pub fn new(repository: KomikRepository) -> Self {
        Self { repository }
    }

    pub async fn genre_list(&self) -> Result<Vec<KomikGenre>, DomainError> {
        Cache
            .get_or_set("komik:genres:list:v3", GENRE_LIST_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&self.repository.base_url())
                    .await
                    .map_err(|e| e.to_string())?;
                let genres = tokio::task::spawn_blocking(move || parser::parse_genres(&html))
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;

                Ok(genres)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn genre_slug(
        &self,
        genre_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = 1u32;
        let cache_key = format!("komik:genre:{}:{}:v2", genre_slug, page);

        Cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                let url = self.repository.genre_url(&genre_slug, page);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let (komik_list, pagination) =
                    tokio::task::spawn_blocking(move || parser::parse_genre_page(&html, page))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                Ok((komik_list, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn genre_slug_page(
        &self,
        genre_slug: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:genre:{}:{}:v2", genre_slug, page);

        Cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                let url = self.repository.genre_url(&genre_slug, page);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let (komik_list, pagination) =
                    tokio::task::spawn_blocking(move || parser::parse_genre_page(&html, page))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                Ok((komik_list, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn detail_slug(&self, komik_id: String) -> Result<DetailData, DomainError> {
        let cache_key = format!("komik:detail:{}", komik_id);

        Cache
            .get_or_set(&cache_key, DETAIL_CACHE_TTL, || async {
                let url = self.repository.detail_url(&komik_id);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let data =
                    tokio::task::spawn_blocking(move || parser::parse_komik_detail_document(&html))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn chapter_slug(&self, chapter_url: String) -> Result<ChapterData, DomainError> {
        let cache_key = format!("komik:chapter:{}", chapter_url);

        Cache
            .get_or_set(&cache_key, CHAPTER_CACHE_TTL, || async {
                let url = self.repository.chapter_url(&chapter_url);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let data = tokio::task::spawn_blocking({
                    let chapter_url = chapter_url.clone();
                    move || parser::parse_komik_chapter_document(&html, &chapter_url)
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;

                Ok(data)
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn manga_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = page_slug
            .parse::<u32>()
            .map_err(|_| DomainError::Validation("Invalid page number".to_string()))?;
        self.list_by_url("manga", page, self.repository.manga_list_url(page))
            .await
    }

    pub async fn manhua_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = page_slug
            .parse::<u32>()
            .map_err(|_| DomainError::Validation("Invalid page number".to_string()))?;
        self.list_by_url("manhua", page, self.repository.manhua_list_url(page))
            .await
    }

    pub async fn manhwa_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = page_slug
            .parse::<u32>()
            .map_err(|_| DomainError::Validation("Invalid page number".to_string()))?;
        self.list_by_url("manhwa", page, self.repository.manhwa_list_url(page))
            .await
    }

    pub async fn popular_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = page_slug
            .parse::<u32>()
            .map_err(|_| DomainError::Validation("Invalid page number".to_string()))?;
        self.list_by_url("popular", page, self.repository.popular_list_url(page))
            .await
    }

    async fn list_by_url(
        &self,
        list_name: &str,
        page: u32,
        url: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:list:{}:{}:v2", list_name, page);

        Cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let (komik_list, pagination) =
                    tokio::task::spawn_blocking(move || parser::parse_genre_page(&html, page))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                if komik_list.is_empty() {
                    return Err(format!("Empty komik {} page {}", list_name, page));
                }

                Ok((komik_list, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn search_slug(
        &self,
        query: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = 1u32;
        let cache_key = format!("komik:search:{}:{}", query, page);

        Cache
            .get_or_set(&cache_key, SEARCH_CACHE_TTL, || async {
                let url = self.repository.search_url(&query, page);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let (komik_list, pagination) =
                    tokio::task::spawn_blocking(move || parser::parse_genre_page(&html, page))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                Ok((komik_list, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn search_slug_page(
        &self,
        query: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:search:{}:{}", query, page);

        Cache
            .get_or_set(&cache_key, SEARCH_CACHE_TTL, || async {
                let url = self.repository.search_url(&query, page);
                let html = self
                    .repository
                    .fetch_html(&url)
                    .await
                    .map_err(|e| e.to_string())?;

                let (komik_list, pagination) =
                    tokio::task::spawn_blocking(move || parser::parse_genre_page(&html, page))
                        .await
                        .map_err(|e| e.to_string())?
                        .map_err(|e| e.to_string())?;

                Ok((komik_list, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }
}
