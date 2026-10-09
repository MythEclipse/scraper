//! Alqanime (Anime2) scraping repository.

use async_trait::async_trait;
use tracing::warn;

use crate::domain::error::ScrapingError;
use crate::domain::repository::ScrapingRepository;
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
