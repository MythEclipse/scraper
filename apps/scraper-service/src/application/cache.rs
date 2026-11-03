//! Read-through caching policy shared by the use cases.
//!
//! The cache adapter speaks raw strings (see [`CachePort`]); deciding *what* to
//! cache, for how long, and what to do when the cache is unavailable is use-case
//! policy, so it lives here rather than in the port.

use std::future::Future;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::domain::repository::CachePort;

/// Return the cached value for `key`, or compute it, cache it, and return it.
///
/// Best-effort by contract: a cache read miss, a decode failure, or a failed
/// write all degrade to computing the value, so a cache outage costs latency
/// rather than a failed request. A compute failure propagates unchanged.
pub async fn read_through<T, C, F, Fut>(
    cache: &C,
    key: &str,
    ttl_secs: u64,
    compute: F,
) -> Result<T, String>
where
    C: CachePort,
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, String>> + Send,
{
    if let Some(raw) = cache.get_cached(key).await
        && let Ok(cached) = serde_json::from_str::<T>(&raw)
    {
        tracing::debug!(key, "cache hit");
        return Ok(cached);
    }

    let value = compute().await?;
    match serde_json::to_string(&value) {
        Ok(raw) => {
            if let Err(error) = cache.set_cached(key, raw, ttl_secs).await {
                tracing::debug!(key, %error, "cache write failed (non-fatal)");
            }
        }
        Err(error) => tracing::debug!(key, %error, "cache encode failed (non-fatal)"),
    }
    Ok(value)
}
