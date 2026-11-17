// Proxy fetch logic with Redis cache AND Request Coalescing (SingleFlight)
// Updated for sync Redis API, reqwest API changes, and concurrency optimization.

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::sync::LazyLock;
use tokio::sync::broadcast;
use tracing::{debug, error, warn};

use crate::domain::error::ScrapingError;
use crate::infrastructure::cache::mytheclipse;
use crate::infrastructure::utils::cache_ttl::CACHE_TTL_VERY_SHORT;
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

/// Decode a response body into a `String` with a single copy.
///
/// `String::from_utf8` validates in place and hands the buffer back on failure,
/// so a valid page is a move rather than a validate-then-copy, and an invalid
/// one re-uses the same allocation for the lossy decode.
fn into_utf8(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            warn!("Response body is not valid UTF-8, using lossy conversion");
            String::from_utf8_lossy(err.as_bytes()).into_owned()
        }
    }
}

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

async fn get_cached_fetch(slug: &str) -> Result<Option<FetchResult>, ScrapingError> {
    let key = get_fetch_cache_key(slug);
    let bytes = mytheclipse::get(&key)
        .await
        .map_err(|e| ScrapingError::Http(format!("Cache get failed for {}: {}", slug, e)))?;

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

async fn set_cached_fetch(slug: &str, value: &FetchResult) -> Result<(), ScrapingError> {
    let key = get_fetch_cache_key(slug);
    let json = serde_json::to_vec(value)
        .map_err(|e| ScrapingError::Http(format!("cache encode failed: {e}")))?;

    // Use standardized TTL
    mytheclipse::set(
        &key,
        json,
        Some(std::time::Duration::from_secs(CACHE_TTL_VERY_SHORT)),
    )
    .await
    .map_err(|e| ScrapingError::Http(format!("Cache set failed for {}: {}", slug, e)))
}
// --- REDIS CACHE WRAPPER END ---

/// Main entry point: Fetches with proxy, using Cache and Request Coalescing
pub async fn fetch_with_proxy(slug: &str) -> Result<FetchResult, ScrapingError> {
    // 1. Try Cache First
    if let Ok(Some(cached)) = get_cached_fetch(slug).await {
        return Ok(cached);
    }

    // 2. Request Coalescing (SingleFlight)
    //
    // The receiver is created BEFORE the leader task is spawned. A broadcast
    // channel with zero receivers drops the value it is sent, so subscribing
    // afterwards (the previous order) raced the leader: whenever the fetch
    // finished before the subscribe — always the case for a cache-warm or
    // fast-failing upstream — the send went nowhere and the leader then waited
    // on a channel that could never deliver, holding the request for the full
    // client timeout. Subscribing first makes that impossible.
    //
    // `entry` (not `get` + `insert`) makes leader election atomic: two requests
    // arriving in the same instant can no longer both decide they are the
    // leader and fetch the same page twice.
    let (tx, mut rx, is_leader) = match IN_FLIGHT.get(slug) {
        Some(in_flight) => {
            debug!("[Coalesce] Joining in-flight request for {}", slug);
            let tx = in_flight.value().clone();
            let rx = tx.subscribe();
            (tx, rx, false)
        }
        None => match IN_FLIGHT.entry(slug.to_string()) {
            Entry::Occupied(occupied) => {
                debug!("[Coalesce] Joining in-flight request for {}", slug);
                let tx = occupied.get().clone();
                let rx = tx.subscribe();
                (tx, rx, false)
            }
            Entry::Vacant(vacant) => {
                debug!("[Coalesce] Starting leader request for {}", slug);
                let (tx, rx) = broadcast::channel(1); // Capacity 1 is enough for a single result
                vacant.insert(tx.clone());
                (tx, rx, true)
            }
        },
    };

    // The map holds its own sender clone, so this one is only needed by the
    // leader task. Followers drop theirs: a live sender held by a waiter keeps
    // the channel open, which would turn a lost leader into a hang instead of a
    // closed channel.
    if is_leader {
        let slug_owned = slug.to_string();

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
            let _guard = DropGuard(slug_owned.clone());

            let _permit = crate::infrastructure::scraping::limiter::fetch_limiter()
                .acquire()
                .await;
            let result = perform_fetch(&slug_owned).await;

            // Map the fetch error to String for broadcast (it is not Clone).
            // FetchResult is Clone.
            let broadcast_result = match &result {
                Ok(res) => Ok(res.clone()),
                Err(e) => Err(e.to_string()),
            };

            // Broadcast result to all waiting subscribers
            let _ = tx.send(broadcast_result);
        });
    } else {
        drop(tx);
    }

    // 3. Wait for result (Leader or Follower)
    match rx.recv().await {
        Ok(Ok(res)) => Ok(res),
        Ok(Err(e_str)) => Err(ScrapingError::Http(e_str)),
        Err(e) => {
            warn!("[Coalesce] Receive mismatch for {}: {:?}", slug, e);
            Err(ScrapingError::Http("Request coalescing error".to_string()))
        }
    }
}

/// The actual fetch logic (Direct -> Retry -> Proxy)
async fn perform_fetch(slug: &str) -> Result<FetchResult, ScrapingError> {
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

    // Use shared global HTTP client. Its default headers already carry the
    // browser-like User-Agent/Referer, so no per-request HeaderMap is built.
    let client = http_client().client();

    match client
        .get(slug)
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

                let bytes = res
                    .bytes()
                    .await
                    .map_err(|e| ScrapingError::Http(format!("body read failed: {e}")))?;

                // Check if response is Gzip compressed (magic header 1f 8b).
                // reqwest decodes `Content-Encoding` itself, so this only
                // catches a body that is gzipped WITHOUT the header.
                let text_data = if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
                    // Gzip compressed: decompress through mytheclipse compute, which
                    // catches panics so a malformed payload cannot take the request
                    // task down. `compute` runs the closure on the calling thread
                    // (mytheclipse >= 2.0 has no dedicated pool), so this stays a
                    // bounded, panic-isolated call on the current worker.
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
                                ScrapingError::Http(format!(
                                    "Decompression failed or exceeded limits: {:?}",
                                    e
                                ))
                            })
                    })
                    .map_err(|e| {
                        ScrapingError::Http(format!("Compute decompression failed: {e}"))
                    })??;

                    // `from_utf8` validates in place and hands the buffer back
                    // on failure, so the page is never copied twice.
                    into_utf8(decompressed)
                } else {
                    into_utf8(bytes.to_vec())
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
                        warn!(
                            "[Circuit Breaker] Blacklisting domain {} due to hostile HTTP status {}",
                            domain,
                            res.status()
                        );
                        FAILED_DOMAINS.insert(domain.clone());
                    }
                    error!("{}", error_msg);
                    return perform_proxy_chain(slug).await;
                } else {
                    warn!("{}", error_msg);
                }
                Err(ScrapingError::Http(error_msg))
            }
        }
        Err(e) => {
            let error_msg = format!("Direct fetch failed for {}: {:?}", slug, e);
            warn!("{}", error_msg);

            // Hard panic mapping: If reqwest core network/TLS fails, instantly blacklist the domain
            if !domain.is_empty() {
                warn!(
                    "[Circuit Breaker] Blacklisting domain {} due to core network/SSL trace failure",
                    domain
                );
                FAILED_DOMAINS.insert(domain.clone());
            }

            // Fall back seamlessly to Relay network proxy instead of throwing fatal transient backoff
            perform_proxy_chain(slug).await
        }
    }
}

pub async fn fetch_with_proxy_only(slug: &str) -> Result<FetchResult, ScrapingError> {
    if let Ok(Some(cached)) = get_cached_fetch(slug).await {
        return Ok(cached);
    }

    perform_proxy_chain(slug).await
}

async fn perform_proxy_chain(slug: &str) -> Result<FetchResult, ScrapingError> {
    match fetch_via_relays(slug).await {
        Ok(res) => Ok(res),
        Err(e) => {
            error!("[ProxyChain] All relays failed for {}: {:?}", slug, e);
            Err(e)
        }
    }
}

/// A single relay attempt: `Ok` carries the page, `Err` carries why this relay
/// lost the race.
async fn fetch_via_relay(slug: &str, relay: &str) -> Result<FetchResult, ScrapingError> {
    debug!(
        "[fetch_via_relays] Attempting to fetch {} via relay {}",
        slug, relay
    );

    let res = http_client()
        .client()
        .get(relay)
        .header("x-relay-target", slug)
        .send()
        .await
        .map_err(|e| {
            warn!(
                "[fetch_via_relays] Relay {} failed for {}: {:?}",
                relay, slug, e
            );
            ScrapingError::Http(format!("relay {relay} transport error: {e}"))
        })?;

    if !res.status().is_success() {
        warn!(
            "[fetch_via_relays] Relay {} returned status {} for {}",
            relay,
            res.status(),
            slug
        );
        return Err(ScrapingError::Http(format!(
            "relay {relay} returned status {}",
            res.status()
        )));
    }

    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let data = res
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("body read failed: {e}")))?;

    debug!(
        "[fetch_via_relays] Successfully fetched {} via relay {}",
        slug, relay
    );
    Ok(FetchResult { data, content_type })
}

async fn fetch_via_relays(slug: &str) -> Result<FetchResult, ScrapingError> {
    // Race the relays instead of walking them in order. They are independent
    // proxies for the same target, so the sequential walk paid the full client
    // timeout (30s) for every dead relay — 90s worst case, inside a request that
    // also holds one of the 16 global fetch permits. Racing turns that into a
    // single timeout window and keeps the fastest relay's latency.
    let attempts = RELAY_ENDPOINTS
        .iter()
        .map(|relay| Box::pin(fetch_via_relay(slug, relay)))
        .collect::<Vec<_>>();

    match futures::future::select_ok(attempts).await {
        Ok((result, _losers)) => {
            if let Err(e) = set_cached_fetch(slug, &result).await {
                warn!("Failed to cache relay result for {}: {:?}", slug, e);
            }
            Ok(result)
        }
        Err(e) => {
            error!("[ProxyChain] All relays failed for {}: {:?}", slug, e);
            Err(ScrapingError::Http(
                "All relay endpoints failed".to_string(),
            ))
        }
    }
}
