//! Komik site scraping repository.

use async_trait::async_trait;

use crate::domain::entity::anime::Pagination;
use crate::domain::entity::komik::{ChapterData, DetailData, KomikGenre, KomikItem};
use crate::domain::error::ScrapingError;
use crate::domain::repository::{KomikComicRepository, ScrapingRepository};
use crate::infrastructure::repository::parsers::komik_parser;
use crate::infrastructure::scraping::html_fetcher::fetch_html_with_retry;
use crate::infrastructure::scraping::scraping_urls::{get_komik_api_url, get_komik_url};

pub struct KomikRepository;

impl KomikRepository {
    pub fn new() -> Self {
        Self
    }

    pub fn api_url(&self) -> String {
        get_komik_api_url()
    }

    pub fn base_url(&self) -> String {
        get_komik_url()
    }

    pub fn genre_url(&self, genre_slug: &str, page: u32) -> String {
        if page == 1 {
            format!("{}/genre/{}/", self.api_url(), genre_slug)
        } else {
            format!("{}/genre/{}/page/{}/", self.api_url(), genre_slug, page)
        }
    }

    pub fn search_url(&self, query: &str, page: u32) -> String {
        if page == 1 {
            format!("{}/search/{}/", self.api_url(), query)
        } else {
            format!("{}/search/{}/page/{}/", self.api_url(), query, page)
        }
    }

    pub fn manga_list_url(&self, page: u32) -> String {
        format!("{}/manga/page/{}/?tipe=manga", self.api_url(), page)
    }

    pub fn manhua_list_url(&self, page: u32) -> String {
        format!("{}/manga/page/{}/?tipe=manhua", self.api_url(), page)
    }

    pub fn manhwa_list_url(&self, page: u32) -> String {
        format!("{}/manga/page/{}/?tipe=manhwa", self.api_url(), page)
    }

    pub fn popular_list_url(&self, page: u32) -> String {
        format!(
            "{}/manga/page/{}/?orderby=meta_value_num",
            self.api_url(),
            page
        )
    }

    pub fn detail_url(&self, slug: &str) -> String {
        format!("{}/manga/{}/", self.base_url(), slug)
    }

    pub fn chapter_url(&self, chapter_url: &str) -> String {
        format!("{}/{}", self.base_url(), chapter_url)
    }
}

impl Default for KomikRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScrapingRepository for KomikRepository {
    async fn fetch_html(&self, url: &str) -> Result<String, ScrapingError> {
        fetch_html_with_retry(url).await
    }
}

#[async_trait]
impl KomikComicRepository for KomikRepository {
    async fn fetch_genres(&self) -> Result<Vec<KomikGenre>, ScrapingError> {
        let html = self.fetch_html(&self.base_url()).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genres(&html)).await
    }
    async fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.genre_url(genre_slug, page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
    async fn fetch_detail(&self, komik_id: &str) -> Result<DetailData, ScrapingError> {
        let html = self.fetch_html(&self.detail_url(komik_id)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_komik_detail_document(&html))
            .await
    }
    async fn fetch_chapter(&self, chapter_url: &str) -> Result<ChapterData, ScrapingError> {
        let html = self.fetch_html(&self.chapter_url(chapter_url)).await?;
        let owned = chapter_url.to_string();
        KomikRepository::parse_blocking(move || {
            komik_parser::parse_komik_chapter_document(&html, &owned)
        })
        .await
    }
    async fn fetch_manga_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.manga_list_url(page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
    async fn fetch_manhua_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.manhua_list_url(page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
    async fn fetch_manhwa_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.manhwa_list_url(page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
    async fn fetch_popular_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.popular_list_url(page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
    async fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
        let html = self.fetch_html(&self.search_url(query, page)).await?;
        KomikRepository::parse_blocking(move || komik_parser::parse_genre_page(&html, page)).await
    }
}

impl KomikRepository {
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
