// Proxy fetch logic with Redis cache AND Request Coalescing (SingleFlight)
// Updated for sync Redis API, reqwest API changes, and concurrency optimization.

use dashmap::DashMap;
use std::sync::LazyLock;
use tokio::sync::broadcast;
use tracing::{debug, error, warn};

use crate::domain::error::AppError;
use crate::infrastructure::cache::mytheclipse;
use crate::infrastructure::utils::cache_ttl::CACHE_TTL_VERY_SHORT;
use crate::infrastructure::utils::http::common_headers;
use crate::infrastructure::utils::http::is_internet_baik_block_page;
use crate::infrastructure::utils::http_client::http_client;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FetchResult {
    pub data: String,
    pub content_type: Option<String>,
}

// Implement Display to allow .to_string()
impl std::fmt::Display for FetchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FetchResult {{ data: {}, content_type: {} }}",
            self.data,
            self.content_type.as_deref().unwrap_or("None")
        )
    }
}

// Global In-Flight Request Map for Request Coalescing
// Maps URL slug -> Broadcast Sender
static IN_FLIGHT: LazyLock<DashMap<String, broadcast::Sender<Result<FetchResult, String>>>> =
    LazyLock::new(DashMap::new);

// Global Blacklist for domains that consistently fail direct fetch (Timeouts, SSL, Cloudflare blocks)
static FAILED_DOMAINS: LazyLock<dashmap::DashSet<String>> = LazyLock::new(dashmap::DashSet::new);

const RELAY_ENDPOINTS: &[&str] = &[
    "https://opennext-app.superaseph.workers.dev",
    "https://proxy-bun.vercel.app",
    "https://proxy-bun-mytheclipse8647-orfq73fe.apn.leapcell.dev",
];

// --- REDIS CACHE WRAPPER START ---
fn get_fetch_cache_key(slug: &str) -> String {
    format!("fetch:proxy:{slug}")
}

async fn get_cached_fetch(slug: &str) -> Result<Option<FetchResult>, AppError> {
    let key = get_fetch_cache_key(slug);
    let bytes = mytheclipse::get(&key)
        .await
        .map_err(|e| AppError::Internal(format!("Cache get failed for {}: {}", slug, e)))?;

    if let Some(bytes) = bytes {
        match serde_json::from_slice::<FetchResult>(&bytes) {
            Ok(parsed) => {
                debug!("[fetchWithProxy] Returning cached response for {}", slug);
                Ok(Some(parsed))
            }
            Err(_) => Ok(None),
        }
    } else {
        Ok(None)
    }
}

async fn set_cached_fetch(slug: &str, value: &FetchResult) -> Result<(), AppError> {
    let key = get_fetch_cache_key(slug);
    let json = serde_json::to_vec(value)?;

    // Use standardized TTL
    mytheclipse::set(
        &key,
        json,
        Some(std::time::Duration::from_secs(CACHE_TTL_VERY_SHORT)),
    )
    .await
    .map_err(|e| AppError::Internal(format!("Cache set failed for {}: {}", slug, e)))
}
// --- REDIS CACHE WRAPPER END ---

/// Main entry point: Fetches with proxy, using Cache and Request Coalescing
pub async fn fetch_with_proxy(slug: &str) -> Result<FetchResult, AppError> {
    // 1. Try Cache First
    if let Ok(Some(cached)) = get_cached_fetch(slug).await {
        return Ok(cached);
    }

    // 2. Request Coalescing (SingleFlight)
    // Check if there is already an in-flight request for this slug
    let tx = {
        if let Some(in_flight) = IN_FLIGHT.get(slug) {
            debug!("[Coalesce] Joining in-flight request for {}", slug);
            in_flight.value().clone()
        } else {
            // No in-flight request, create a new channel
            let (tx, _) = broadcast::channel(1); // Capacity 1 is enough for single result
            IN_FLIGHT.insert(slug.to_string(), tx.clone());
            debug!("[Coalesce] Starting leader request for {}", slug);

            // We are the leader, we must execute the fetch
            // Spawn the fetch task so we don't block holding the map lock (though insert is fast)
            // But actually we are not holding the lock here anymore.

            // Clone for the async block
            let slug_clone = slug.to_string();
            let tx_clone = tx.clone();

            // Leader task: bounded by mytheclipse spawn_io (tracing-instrumented)
            // and the global fetch concurrency limiter (tokio Semaphore bridge).
            // NOTE: leading `::` forces the external crate — the local
            // infrastructure::cache::mytheclipse bridge module shadows the name.
            ::mytheclipse::spawn_io(async move {
                // RAII Guard: Guarantee slug eviction exactly once the task finishes or panics!
                struct DropGuard(String);
                impl Drop for DropGuard {
                    fn drop(&mut self) {
                        IN_FLIGHT.remove(&self.0);
                    }
                }
                let _guard = DropGuard(slug_clone.clone());

                let _permit = crate::infrastructure::scraping::limiter::fetch_limiter()
                    .acquire()
                    .await;
                let result = perform_fetch(&slug_clone).await;

                // Map AppError to String for broadcast (since AppError might not be Clone)
                // FetchResult is Clone.
                let broadcast_result = match &result {
                    Ok(res) => Ok(res.clone()),
                    Err(e) => Err(e.to_string()),
                };

                // Broadcast result to all waiting subscribers
                let _ = tx_clone.send(broadcast_result);
            });

            tx
        }
    };

    // 3. Wait for result (Leader or Follower)
    let mut rx = tx.subscribe();
    match rx.recv().await {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(e_str)) => Err(AppError::Internal(e_str)),
        Err(e) => {
            warn!("[Coalesce] Receive mismatch for {}: {:?}", slug, e);
            Err(AppError::Internal("Request coalescing error".to_string()))
        }
    }
}

/// The actual fetch logic (Direct -> Retry -> Proxy)
async fn perform_fetch(slug: &str) -> Result<FetchResult, AppError> {
    // 1. Extract domain for Circuit Breaker logic
    let domain = reqwest::Url::parse(slug)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // 2. Immediate proxy fallback if domain has a known history of brutal timeouts/blocks
    if !domain.is_empty() && FAILED_DOMAINS.contains(&domain) {
        warn!(
            "[Circuit Breaker] Domain {} is blacklisted from direct-fetch. Routing via Relays.",
            domain
        );
        return perform_proxy_chain(slug).await;
    }

    // Use shared global HTTP client
    let client = http_client().client();
    let headers = common_headers();

    match client
        .get(slug)
        .headers(headers)
        .send() // Timeout handled by client
        .await
    {
        Ok(res) => {
            debug!(
                "[fetchWithProxy] Direct fetch response: url={}, status={}",
                slug,
                res.status()
            );
            if res.status().is_success() {
                let content_type = res
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());

                let bytes = res.bytes().await?;

                // Check if response is Gzip compressed (magic header 1f 8b)
                let text_data = if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
                    // Gzip compressed, offload decompression to mytheclipse compute
                    // (sized rayon pool, panic-isolated) instead of the blocking pool.
                    // NOTE: leading `::` forces the external crate (shadowed by the
                    // local infrastructure::cache::mytheclipse bridge module).
                    let decompressed = ::mytheclipse::compute(move || {
                        use flate2::read::GzDecoder;
                        use std::io::Read;
                        let decoder = GzDecoder::new(&bytes[..]);
                        let mut decompressed = Vec::new();
                        // 10MB absolute decompression bounds to prevent GZIP Bombs (OOM vulnerability)
                        decoder
                            .take(10_000_000)
                            .read_to_end(&mut decompressed)
                            .map(|_| decompressed)
                            .map_err(|e| {
                                AppError::Internal(format!(
                                    "Decompression failed or exceeded limits: {:?}",
                                    e
                                ))
                            })
                    })
                    .map_err(|e| {
                        AppError::Internal(format!("Compute decompression failed: {e}"))
                    })??;

                    match std::str::from_utf8(&decompressed) {
                        Ok(s) => s.to_string(),
                        Err(_) => String::from_utf8_lossy(&decompressed).to_string(),
                    }
                } else {
                    match std::str::from_utf8(&bytes) {
                        Ok(s) => s.to_string(),
                        Err(_) => {
                            warn!("Response bytes are not valid UTF-8, using lossy conversion");
                            String::from_utf8_lossy(&bytes).to_string()
                        }
                    }
                };

                if is_internet_baik_block_page(&text_data) {
                    warn!("Blocked by internetbaik (direct fetch) for {}", slug);
                    return perform_proxy_chain(slug).await;
                } else {
                    let result = FetchResult {
                        data: text_data,
                        content_type,
                    };
                    // Cache the success result
                    if let Err(e) = set_cached_fetch(slug, &result).await {
                        warn!("Failed to cache result for {}: {:?}", slug, e);
                    }
                    Ok(result)
                }
            } else {
                let error_msg = format!(
                    "Direct fetch failed with status {} for {}",
                    res.status(),
                    slug
                );

                // Penalize domain if it throws aggressive Anti-Bot or Gateway Timeout codes
                if res.status().is_server_error()
                    || res.status() == reqwest::StatusCode::FORBIDDEN
                    || res.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                {
                    if !domain.is_empty() {
                        warn!("[Circuit Breaker] Blacklisting domain {} due to hostile HTTP status {}", domain, res.status());
                        FAILED_DOMAINS.insert(domain.clone());
                    }
                    error!("{}", error_msg);
                    return perform_proxy_chain(slug).await;
                } else {
                    warn!("{}", error_msg);
                }
                Err(AppError::Internal(error_msg))
            }
        }
        Err(e) => {
            let error_msg = format!("Direct fetch failed for {}: {:?}", slug, e);
            warn!("{}", error_msg);

            // Hard panic mapping: If reqwest core network/TLS fails, instantly blacklist the domain
            if !domain.is_empty() {
                warn!("[Circuit Breaker] Blacklisting domain {} due to core network/SSL trace failure", domain);
                FAILED_DOMAINS.insert(domain.clone());
            }

            // Fall back seamlessly to Relay network proxy instead of throwing fatal transient backoff
            perform_proxy_chain(slug).await
        }
    }
}

pub async fn fetch_with_proxy_only(slug: &str) -> Result<FetchResult, AppError> {
    if let Ok(Some(cached)) = get_cached_fetch(slug).await {
        return Ok(cached);
    }

    perform_proxy_chain(slug).await
}

async fn perform_proxy_chain(slug: &str) -> Result<FetchResult, AppError> {
    match fetch_via_relays(slug).await {
        Ok(res) => Ok(res),
        Err(e) => {
            error!("[ProxyChain] All relays failed for {}: {:?}", slug, e);
            Err(e)
        }
    }
}

async fn fetch_via_relays(slug: &str) -> Result<FetchResult, AppError> {
    // Use shared client
    let client = http_client().client();

    for relay in RELAY_ENDPOINTS {
        debug!(
            "[fetch_via_relays] Attempting to fetch {} via relay {}",
            slug, relay
        );

        match client
            .get(*relay)
            .header("x-relay-target", slug)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                let content_type = res
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());
                let data = res.text().await?;

                let result = FetchResult { data, content_type };
                debug!(
                    "[fetch_via_relays] Successfully fetched {} via relay {}",
                    slug, relay
                );

                if let Err(e) = set_cached_fetch(slug, &result).await {
                    warn!("Failed to cache relay result for {}: {:?}", slug, e);
                }

                return Ok(result);
            }
            Ok(res) => {
                warn!(
                    "[fetch_via_relays] Relay {} returned status {} for {}",
                    relay,
                    res.status(),
                    slug
                );
            }
            Err(e) => {
                warn!(
                    "[fetch_via_relays] Relay {} failed for {}: {:?}",
                    relay, slug, e
                );
            }
        }
    }

    Err(AppError::Internal("All relay endpoints failed".to_string()))
}
