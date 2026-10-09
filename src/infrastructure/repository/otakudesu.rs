//! Otakudesu anime scraping repository.

use async_trait::async_trait;
use tracing::{info, warn};

use crate::domain::entity::anime::{
    AnimeData, AnimeDetailData, AnimeFullData, CompleteAnimeListItem, Genre, GenreAnimeItem,
    LatestAnimeItem, OngoingAnimeListItem, Pagination, SearchAnimeItem,
};
use crate::domain::error::ScrapingError;
use crate::domain::repository::ScrapingRepository;
use crate::infrastructure::repository::parsers::otakudesu_parser;
use crate::infrastructure::scraping::html_fetcher::fetch_html_with_retry;
use crate::infrastructure::scraping::proxy_fetch::fetch_with_proxy;
use crate::infrastructure::scraping::retry::{default_backoff, retry, retry_all};

const OTAKUDESU_BASE_URL: &str = "https://otakudesu.cloud";

pub struct OtakudesuRepository;

impl OtakudesuRepository {
    pub fn new() -> Self {
        Self
    }

    fn base_url(&self) -> String {
        "https://otakudesu.cloud".to_string()
    }

    pub fn index_urls(&self) -> (String, String) {
        let base = self.base_url();
        (
            format!("{}/ongoing-anime/", base),
            format!("{}/complete-anime/", base),
        )
    }

    pub fn genres_url(&self) -> String {
        format!("{}/genre-list/", self.base_url())
    }

    pub fn detail_url(&self, slug: &str) -> String {
        format!("{}/anime/{}", OTAKUDESU_BASE_URL, slug)
    }

    pub fn page_url(&self, category: &str, page: &str) -> String {
        format!("{}/{}/page/{}/", OTAKUDESU_BASE_URL, category, page)
    }

    pub fn search_url(&self, query: &str, page: &str) -> String {
        if page == "1" {
            format!("{}/search/{}/", self.base_url(), query)
        } else {
            format!("{}/search/{}/page/{}/", self.base_url(), query, page)
        }
    }

    pub fn genre_page_url(&self, genre_slug: &str, page: &str) -> String {
        // Current otakudesu uses the plural `/genres/{slug}/` path (archive
        // layout). The singular `/genre/{slug}/page/{page}/` path 301-redirects
        // to otakudesu.io (a 404 placeholder), which made genre pages return 0.
        format!("{}/genres/{}/page/{}/", self.base_url(), genre_slug, page)
    }

    pub fn full_episode_url(&self, slug: &str) -> String {
        format!("{}/episode/{}", OTAKUDESU_BASE_URL, slug)
    }
}

impl Default for OtakudesuRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScrapingRepository for OtakudesuRepository {
    async fn fetch_html(&self, url: &str) -> Result<String, ScrapingError> {
        fetch_html_with_retry(url).await
    }
}

impl OtakudesuRepository {
    pub async fn fetch_anime_index(&self) -> Result<AnimeData, ScrapingError> {
        let (ongoing_url, complete_url) = self.index_urls();
        let (ongoing_html, complete_html) = tokio::join!(
            self.fetch_html(&ongoing_url),
            self.fetch_html(&complete_url)
        );
        let ongoing_html = ongoing_html?;
        let complete_html = complete_html?;

        let ongoing_anime = tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_ongoing_anime(&ongoing_html)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))??;

        let complete_anime = tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_complete_anime(&complete_html)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))??;

        Ok(AnimeData {
            ongoing_anime,
            complete_anime,
        })
    }

    pub async fn fetch_genres(&self) -> Result<Vec<Genre>, ScrapingError> {
        let html = self.fetch_html(&self.genres_url()).await?;
        tokio::task::spawn_blocking(move || otakudesu_parser::parse_genres(&html))
            .await
            .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_anime_detail(&self, slug: &str) -> Result<AnimeDetailData, ScrapingError> {
        let url = self.detail_url(slug);
        let html = self.fetch_with_proxy_retry(&url).await?;
        tokio::task::spawn_blocking(move || otakudesu_parser::parse_anime_detail_document(&html))
            .await
            .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_complete_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<CompleteAnimeListItem>, Pagination), ScrapingError> {
        let url = self.page_url("complete-anime", slug);
        let html = self.fetch_html(&url).await?;
        let slug_owned = slug.to_string();
        tokio::task::spawn_blocking(move || otakudesu_parser::parse_anime_page(&html, &slug_owned))
            .await
            .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_ongoing_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<OngoingAnimeListItem>, Pagination), ScrapingError> {
        let url = self.page_url("ongoing-anime", slug);
        let html = self.fetch_html(&url).await?;
        let slug_owned = slug.to_string();
        tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_ongoing_anime_document(&html, &slug_owned)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_latest_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError> {
        // The site's `/latest-anime/` path was removed (301 → otakudesu.io
        // placeholder). The homepage IS the latest-episodes page (`.venz ul li`
        // with `.thumbz h2.jdlflm`, `.epz`), paginated via WordPress `/page/N/`.
        let url = if slug == "1" {
            format!("{}/", self.base_url())
        } else {
            format!("{}/page/{}/", self.base_url(), slug)
        };
        let html = self.fetch_html(&url).await?;
        let slug_owned = slug.to_string();
        tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_latest_anime_document(&html, &slug_owned)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_search_anime_page(
        &self,
        slug: &str,
        page: &str,
    ) -> Result<(Vec<SearchAnimeItem>, Pagination), ScrapingError> {
        let url = self.search_url(slug, page);
        let html = self.fetch_html(&url).await?;
        let page_owned = page.to_string();
        tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_search_anime_document(&html, &page_owned)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_genre_anime_page(
        &self,
        genre_slug: &str,
        page: &str,
    ) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError> {
        let url = self.genre_page_url(genre_slug, page);
        let html = self.fetch_html(&url).await?;
        let page_owned = page.to_string();
        tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_genre_anime_document(&html, &page_owned)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    pub async fn fetch_anime_full(&self, slug: &str) -> Result<AnimeFullData, ScrapingError> {
        let url = self.full_episode_url(slug);
        let html = self.fetch_html(&url).await?;
        let slug_owned = slug.to_string();
        tokio::task::spawn_blocking(move || {
            otakudesu_parser::parse_anime_full_document(&html, &slug_owned)
        })
        .await
        .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }

    async fn fetch_with_proxy_retry(&self, url: &str) -> Result<String, ScrapingError> {
        let backoff = default_backoff();
        let url_owned = url.to_string();
        let fetch_op = || async {
            info!("Fetching URL: {}", url_owned);
            match fetch_with_proxy(&url_owned).await {
                Ok(response) => {
                    info!("Successfully fetched URL: {}", url_owned);
                    Ok(response.data)
                }
                Err(e) => {
                    warn!("Failed to fetch URL: {}, error: {:?}", url_owned, e);
                    Err(ScrapingError::Http(format!("Proxy fetch failed: {}", e)))
                }
            }
        };
        retry(backoff, retry_all, fetch_op)
            .await
            .map_err(|e| ScrapingError::Http(e.to_string()))
    }
}
