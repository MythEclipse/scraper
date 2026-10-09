//! Alqanime (Anime2) scraping repository.

use async_trait::async_trait;
use tracing::warn;

use crate::domain::entity::alqanime::AlqDetailData;
use crate::domain::entity::anime::{
    CompleteAnimeItem, FilterAnimeItem, Genre, GenreAnimeItem, LatestAnimeItem, OngoingAnimeItem,
    OngoingAnimeItemWithScore, Pagination, PaginationWithStringPages, SearchAnimeItem,
};
use crate::domain::error::ScrapingError;
use crate::domain::repository::{AlqanimeAnimeRepository, ScrapingRepository};
use crate::infrastructure::repository::parsers::alqanime_parser;
use crate::infrastructure::scraping::proxy_fetch::fetch_with_proxy;

const BASE_URL: &str = "https://alqanime.si";

pub struct AlqanimeRepository;

impl AlqanimeRepository {
    pub fn new() -> Self {
        Self
    }

    pub fn index_ongoing_url(&self) -> String {
        format!("{}/anime/?status=ongoing&type=&order=update", BASE_URL)
    }

    pub fn index_complete_url(&self) -> String {
        format!("{}/anime/?status=completed&type=&order=update", BASE_URL)
    }

    pub fn genre_list_url(&self) -> String {
        format!("{}/anime/", BASE_URL)
    }

    pub fn filter_url(&self, page: u32, order: &str) -> String {
        if page > 1 {
            format!("{}/anime/page/{}/?order={}", BASE_URL, page, order)
        } else {
            format!("{}/anime/?order={}", BASE_URL, order)
        }
    }

    pub fn detail_url(&self, slug: &str) -> String {
        format!("{}/anime/{}/", BASE_URL, slug)
    }

    pub fn genre_page_url(&self, genre_slug: &str, page: u32) -> String {
        if page > 1 {
            format!(
                "{}/anime/page/{}/?genre[]={}&order=update",
                BASE_URL, page, genre_slug
            )
        } else {
            format!("{}/anime/?genre[]={}&order=update", BASE_URL, genre_slug)
        }
    }

    pub fn search_url(&self, query: &str, page: u32) -> String {
        let encoded = urlencoding::encode(query);
        if page == 1 {
            format!("{}/?s={}", BASE_URL, encoded)
        } else {
            format!("{}/page/{}/?s={}", BASE_URL, page, encoded)
        }
    }

    pub fn latest_url(&self, page: u32) -> String {
        format!(
            "{}/anime/page/{}/?status=&type=&order=latest",
            BASE_URL, page
        )
    }

    pub fn ongoing_url(&self, page: u32) -> String {
        format!(
            "{}/anime/page/{}/?status=ongoing&type=&order=update",
            BASE_URL, page
        )
    }

    pub fn complete_url(&self, page: u32) -> String {
        format!(
            "{}/anime/page/{}/?status=completed&order=update",
            BASE_URL, page
        )
    }
}

impl Default for AlqanimeRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScrapingRepository for AlqanimeRepository {
    async fn fetch_html(&self, url: &str) -> Result<String, ScrapingError> {
        let response = fetch_with_proxy(url)
            .await
            .map_err(|e| ScrapingError::Http(format!("Alqanime fetch failed: {}", e)))?;
        if response.data.trim().is_empty() {
            warn!("Alqanime fetch returned empty body for {}", url);
        }
        Ok(response.data)
    }
}

#[async_trait]
impl AlqanimeAnimeRepository for AlqanimeRepository {
    async fn fetch_index_ongoing(&self) -> Result<Vec<OngoingAnimeItem>, ScrapingError> {
        let html = self.fetch_html(&self.index_ongoing_url()).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_ongoing_anime(&html))
            .await
    }

    async fn fetch_index_complete(&self) -> Result<Vec<CompleteAnimeItem>, ScrapingError> {
        let html = self.fetch_html(&self.index_complete_url()).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_complete_anime(&html))
            .await
    }

    async fn fetch_genres(&self) -> Result<Vec<Genre>, ScrapingError> {
        let html = self.fetch_html(&self.genre_list_url()).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_genres(&html)).await
    }

    async fn fetch_filter(
        &self,
        page: u32,
        genre: &str,
        status: &str,
        anime_type: &str,
        order: &str,
    ) -> Result<(Vec<FilterAnimeItem>, Pagination), ScrapingError> {
        // The filter route composes its own query string on top of the base URL.
        let mut url = self.filter_url(page, order);
        for (key, value) in [("genre[]", genre), ("status", status), ("type", anime_type)] {
            for part in value.split(',') {
                let part = part.trim();
                if !part.is_empty() {
                    url.push_str(&format!("&{}={}", key, part));
                }
            }
        }
        let html = self.fetch_html(&url).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_filter_page(&html, page))
            .await
    }

    async fn fetch_detail(&self, slug: &str) -> Result<AlqDetailData, ScrapingError> {
        let html = self.fetch_html(&self.detail_url(slug)).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_anime_detail(&html)).await
    }

    async fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError> {
        let html = self
            .fetch_html(&self.genre_page_url(genre_slug, page))
            .await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_genre_page(&html, page))
            .await
    }

    async fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> Result<(Vec<SearchAnimeItem>, PaginationWithStringPages), ScrapingError> {
        let html = self.fetch_html(&self.search_url(query, page)).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_search_page(&html, page))
            .await
    }

    async fn fetch_latest_page(
        &self,
        page: u32,
    ) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.latest_url(page)).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_latest_page(&html, page))
            .await
    }

    async fn fetch_ongoing_page(
        &self,
        page: u32,
    ) -> Result<(Vec<OngoingAnimeItemWithScore>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.ongoing_url(page)).await?;
        AlqanimeRepository::parse_blocking(move || alqanime_parser::parse_ongoing_page(&html, page))
            .await
    }

    async fn fetch_complete_page(
        &self,
        page: u32,
    ) -> Result<(Vec<CompleteAnimeItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.complete_url(page)).await?;
        AlqanimeRepository::parse_blocking(move || {
            alqanime_parser::parse_complete_page(&html, page)
        })
        .await
    }

    /// Resolves the direct download link for one episode page.
    ///
    /// Takes the episode's own page URL, not a series slug: the episode list
    /// carries one URL per episode and each holds its own download target.
    async fn fetch_episode_download(&self, episode_url: &str) -> Result<String, ScrapingError> {
        let html = self.fetch_html(episode_url).await?;
        let link = AlqanimeRepository::parse_blocking(move || {
            alqanime_parser::parse_episode_download(&html)
        })
        .await?;
        link.ok_or(ScrapingError::EmptyResponse)
    }
}

impl AlqanimeRepository {
    /// Run a parser off the async runtime and flatten the join error.
    async fn parse_blocking<T, F>(parse: F) -> Result<T, ScrapingError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ScrapingError> + Send + 'static,
    {
        tokio::task::spawn_blocking(parse)
            .await
            .map_err(|e| ScrapingError::Parse(e.to_string()))?
    }
}
