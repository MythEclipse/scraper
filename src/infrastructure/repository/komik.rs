//! Komik site scraping repository.

use async_trait::async_trait;

use crate::domain::error::ScrapingError;
use crate::domain::repository::ScrapingRepository;
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
