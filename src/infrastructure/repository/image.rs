//! Infrastructure — Image generation endpoints (brat).
//!
//! Ported from Shirokami-API `scraper/image/brat.js`.
//! brat v2 uses the external brat.siputzx.my.id API (returns image bytes).

use async_trait::async_trait;

use crate::domain::error::ScrapingError;
use crate::domain::repository::ImageRepository;

use crate::infrastructure::utils::http_client::http_client;
use reqwest::header::USER_AGENT;

const BRAT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0 Safari/537.36";

fn urlencode(s: &str) -> String {
    s.replace('%', "%25")
        .replace('&', "%26")
        .replace('?', "%3F")
}

/// Fetch a brat image (static) for the given text.
pub async fn fetch_brat(text: &str) -> Result<Vec<u8>, String> {
    let url = format!("https://brat.siputzx.my.id/image?text={}", urlencode(text));
    fetch_bytes(&url).await
}

/// Fetch an animated brat GIF for the given text.
pub async fn fetch_brat_animated(text: &str) -> Result<Vec<u8>, String> {
    let url = format!("https://brat.siputzx.my.id/gif?text={}", urlencode(text));
    fetch_bytes(&url).await
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = http_client()
        .client()
        .get(url)
        .header(USER_AGENT, BRAT_UA)
        .header("X-Forwarded-For", "104.28.209.35")
        .header("X-Real-IP", "104.28.209.35")
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Body: {}", e))
}

// ============================================================================
// Port implementation — [`ImageRepository`]
// ============================================================================

/// Adapter for the [`ImageRepository`] port. Stateless: the module functions above are
/// already the whole implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageRepositoryImpl;

#[async_trait]
impl ImageRepository for ImageRepositoryImpl {
    async fn brat(&self, text: &str) -> Result<Vec<u8>, ScrapingError> {
        fetch_brat(text).await.map_err(ScrapingError::Http)
    }
    async fn brat_animated(&self, text: &str) -> Result<Vec<u8>, ScrapingError> {
        fetch_brat_animated(text).await.map_err(ScrapingError::Http)
    }
}
