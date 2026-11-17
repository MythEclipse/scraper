//! HTTP client wrapper with common configurations.
//!
//! One process-wide client per timeout class. Every setting below exists to
//! remove per-request work or a per-request round trip:
//!
//! * `default_headers` — the browser-like `User-Agent`/`Referer` pair is baked
//!   into the client instead of being rebuilt into a fresh `HeaderMap` (and
//!   copied into every request) on each fetch.
//! * transparent decompression — `gzip`/`brotli`/`deflate` are negotiated for
//!   us, so an upstream page arrives already decoded and never travels raw.
//! * keep-alive plus a wide idle pool — a scrape of one site reuses its TLS
//!   connection instead of paying a handshake per page.
//! * `http2_adaptive_window` — the flow-control window follows the measured
//!   bandwidth-delay product, so a large HTML body is not stalled waiting for
//!   WINDOW_UPDATE frames.

use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use reqwest::{Client, ClientBuilder, Response};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::debug;

/// Idle connections kept alive per upstream host. One site scrape fans out over
/// several paths on the same host (index + detail + chapter list) and reuses
/// them.
const POOL_MAX_IDLE_PER_HOST: usize = 64;
/// Drop an idle connection after this long. Kept below the usual 60–75s
/// upstream keep-alive so the client never hands out a socket the server has
/// already closed — that costs a retried request, not just a handshake.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

/// Browser-like identity for every outbound scrape. Built once and shared.
static DEFAULT_HEADERS: LazyLock<HeaderMap> = LazyLock::new(|| {
    let mut headers = HeaderMap::with_capacity(2);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://google.com"));
    headers
});

/// Pre-configured HTTP client with sensible defaults.
#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    /// Shared builder for every timeout class, so tuning lands in one place.
    fn builder(timeout: Duration) -> ClientBuilder {
        ClientBuilder::new()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT)
            .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .tcp_nodelay(true)
            .tcp_keepalive(TCP_KEEPALIVE)
            .http2_adaptive_window(true)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .default_headers(DEFAULT_HEADERS.clone())
    }

    /// Create a new HTTP client with default settings.
    pub fn new() -> Result<Self, String> {
        let client = Self::builder(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self { inner: client })
    }

    /// Create with custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Result<Self, String> {
        let client = Self::builder(Duration::from_secs(timeout_secs))
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
