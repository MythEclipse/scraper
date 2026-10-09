//! HTTP client wrapper with common configurations.

use reqwest::{Client, ClientBuilder, Response};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::debug;

/// Pre-configured HTTP client with sensible defaults.
#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    /// Create a new HTTP client with default settings.
    pub fn new() -> Result<Self, String> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(20)
            .pool_idle_timeout(Duration::from_secs(60))
            .tcp_nodelay(true)
            .user_agent("Scraper/1.0")
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self { inner: client })
    }

    /// Create with custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Result<Self, String> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Scraper/1.0")
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self { inner: client })
    }

    /// GET request.
    pub async fn get(&self, url: &str) -> reqwest::Result<Response> {
        debug!("GET {}", url);
        self.inner.get(url).send().await
    }

    /// GET request and return text.
    pub async fn get_text(&self, url: &str) -> reqwest::Result<String> {
        self.get(url).await?.text().await
    }

    /// GET request and parse JSON.
    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> reqwest::Result<T> {
        self.get(url).await?.json().await
    }

    /// POST request with JSON body.
    pub async fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> reqwest::Result<Response> {
        debug!("POST {}", url);
        self.inner.post(url).json(body).send().await
    }

    /// PUT request with JSON body.
    pub async fn put_json<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> reqwest::Result<Response> {
        debug!("PUT {}", url);
        self.inner.put(url).json(body).send().await
    }

    /// DELETE request.
    pub async fn delete(&self, url: &str) -> reqwest::Result<Response> {
        debug!("DELETE {}", url);
        self.inner.delete(url).send().await
    }

    /// Get the underlying reqwest client.
    pub fn client(&self) -> &Client {
        &self.inner
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("Valid HTTP client configuration")
    }
}

static HTTP_CLIENT_INIT: LazyLock<Result<Arc<HttpClient>, String>> = LazyLock::new(|| {
    HttpClient::new()
        .map(Arc::new)
        .map_err(|e| format!("Failed to initialize HTTP client: {}", e))
});

static HTTP_CLIENT_FAST_INIT: LazyLock<Result<Arc<HttpClient>, String>> = LazyLock::new(|| {
    HttpClient::with_timeout(10)
        .map(Arc::new)
        .map_err(|e| format!("Failed to initialize fast HTTP client: {}", e))
});

static HTTP_CLIENT_SLOW_INIT: LazyLock<Result<Arc<HttpClient>, String>> = LazyLock::new(|| {
    HttpClient::with_timeout(60)
        .map(Arc::new)
        .map_err(|e| format!("Failed to initialize slow HTTP client: {}", e))
});

/// Global HTTP client instance (30s timeout - general purpose).
pub fn http_client() -> &'static HttpClient {
    HTTP_CLIENT_INIT
        .as_ref()
        .expect("HTTP client initialization failed")
}

/// Get the fast HTTP client (10s timeout).
pub fn http_client_fast() -> &'static HttpClient {
    HTTP_CLIENT_FAST_INIT
        .as_ref()
        .expect("Fast HTTP client initialization failed")
}

/// Get the slow HTTP client (60s timeout).
pub fn http_client_slow() -> &'static HttpClient {
    HTTP_CLIENT_SLOW_INIT
        .as_ref()
        .expect("Slow HTTP client initialization failed")
}
