//! Komik application use cases.
//!
//! Orchestrates repository fetching and caching, and returns pure domain types.
//! URL construction, HTTP fetching and HTML parsing all live behind
//! [`KomikComicRepository`], so this layer needs no knowledge of any of them.

use crate::domain::entity::anime::Pagination;
use crate::domain::entity::komik::{ChapterData, DetailData, KomikGenre, KomikItem};
use crate::domain::error::*;
use crate::domain::repository::{CachePort, KomikComicRepository};

const GENRE_LIST_CACHE_TTL: u64 = 3600;
const GENRE_CACHE_TTL: u64 = 300;
const DETAIL_CACHE_TTL: u64 = 300;
const CHAPTER_CACHE_TTL: u64 = 300;
const SEARCH_CACHE_TTL: u64 = 300;

pub struct KomikUseCases<R: KomikComicRepository, C: CachePort> {
    repository: R,
    cache: C,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: KomikComicRepository, C: CachePort>(
    repository: R,
    cache: C,
) -> KomikUseCases<R, C> {
    KomikUseCases { repository, cache }
}

impl<R: KomikComicRepository, C: CachePort> KomikUseCases<R, C> {
    pub async fn genre_list(&self) -> Result<Vec<KomikGenre>, DomainError> {
        self.cache
            .get_or_set("komik:genres:list:v3", GENRE_LIST_CACHE_TTL, || async {
                self.repository
                    .fetch_genres()
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn genre_slug(
        &self,
        genre_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        self.genre_page(genre_slug, 1).await
    }

    pub async fn genre_slug_page(
        &self,
        genre_slug: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        self.genre_page(genre_slug, page).await
    }

    /// Shared body of the first-page and paginated genre routes.
    async fn genre_page(
        &self,
        genre_slug: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:genre:{}:{}:v2", genre_slug, page);

        self.cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async {
                self.repository
                    .fetch_genre_page(&genre_slug, page)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn detail_slug(&self, komik_id: String) -> Result<DetailData, DomainError> {
        let cache_key = format!("komik:detail:{}", komik_id);

        self.cache
            .get_or_set(&cache_key, DETAIL_CACHE_TTL, || async {
                self.repository
                    .fetch_detail(&komik_id)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn chapter_slug(&self, chapter_url: String) -> Result<ChapterData, DomainError> {
        let cache_key = format!("komik:chapter:{}", chapter_url);

        self.cache
            .get_or_set(&cache_key, CHAPTER_CACHE_TTL, || async {
                self.repository
                    .fetch_chapter(&chapter_url)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn manga_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = parse_page(&page_slug)?;
        self.list_page("manga", page, self.repository.fetch_manga_page(page))
            .await
    }

    pub async fn manhua_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = parse_page(&page_slug)?;
        self.list_page("manhua", page, self.repository.fetch_manhua_page(page))
            .await
    }

    pub async fn manhwa_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = parse_page(&page_slug)?;
        self.list_page("manhwa", page, self.repository.fetch_manhwa_page(page))
            .await
    }

    pub async fn popular_slug(
        &self,
        page_slug: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let page = parse_page(&page_slug)?;
        self.list_page("popular", page, self.repository.fetch_popular_page(page))
            .await
    }

    /// Shared body of the four paginated list routes.
    ///
    /// Takes the already-started fetch rather than a closure so the caller
    /// stays free of borrow gymnastics; the empty-page check happens after the
    /// fetch so a broken upstream page is never pinned in the cache.
    async fn list_page(
        &self,
        list_name: &str,
        page: u32,
        fetch: impl std::future::Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>>
            + Send,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:list:{}:{}:v2", list_name, page);

        self.cache
            .get_or_set(&cache_key, GENRE_CACHE_TTL, || async move {
                let (items, pagination) = fetch.await.map_err(|e| e.to_string())?;
                if items.is_empty() {
                    return Err(format!("Empty komik {} page {}", list_name, page));
                }
                Ok((items, pagination))
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }

    pub async fn search_slug(
        &self,
        query: String,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        self.search_page(query, 1).await
    }

    pub async fn search_slug_page(
        &self,
        query: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        self.search_page(query, page).await
    }

    /// Shared body of the first-page and paginated search routes.
    async fn search_page(
        &self,
        query: String,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), DomainError> {
        let cache_key = format!("komik:search:{}:{}", query, page);

        self.cache
            .get_or_set(&cache_key, SEARCH_CACHE_TTL, || async {
                self.repository
                    .fetch_search_page(&query, page)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| DomainError::Scraping(ScrapingError::Http(e)))
    }
}

/// Parses a `{page}` path segment, rejecting anything that is not a number.
fn parse_page(page_slug: &str) -> Result<u32, DomainError> {
    page_slug
        .parse::<u32>()
        .map_err(|_| DomainError::Validation("Invalid page number".to_string()))
}
