//! Repository trait for scraping HTML from remote sources.

use std::future::Future;

use crate::domain::error::ScrapingError;

/// Trait for repositories that fetch and scrape HTML content.
pub trait ScrapingRepository: Send + Sync {
    /// Fetch raw HTML from a URL.
    fn fetch_html(&self, url: &str) -> impl Future<Output = Result<String, ScrapingError>> + Send;
}
