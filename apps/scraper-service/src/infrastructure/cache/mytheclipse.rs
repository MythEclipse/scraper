//! Bridge between the deadpool Redis pool and `mytheclipse_cache::RedisCache`.
//!
//! The scraper keeps its deadpool `Pool` (connection lifecycle, recycling) and
//! hands each checked-out connection to mytheclipse's `RedisCache` (which
//! implements the library `Cache` trait). Because both sides now share the
//! same `redis` crate version, a `deadpool_redis::Connection` can be converted
//! directly into a `redis::aio::MultiplexedConnection` via `take()`.

use mytheclipse_cache::{Cache, CacheError, RedisCache};

use crate::infrastructure::cache::redis_pool::redis_pool;

/// Build a fresh `RedisCache` from a freshly checked-out deadpool connection
/// on each call.
///
/// Why fresh on every call (not a cached singleton): the previous design
/// initialised *one* `RedisCache` (one multiplexed connection) on first use and
/// kept it forever. If that single connection broke (Redis restart, idle
/// timeout, network blip), every cache operation failed with `broken pipe`
/// until the whole process restarted — and because `Cache::get_or_set`
/// propagated write errors, **every API returned 500**.
///
/// Building fresh lets deadpool recycle and re-establish broken connections on
/// checkout, so caching self-heals without a process restart. The multiplexed
/// connection is cheaply cloneable (`Arc`-backed), so a per-call pool checkout
/// is negligible overhead next to the network I/O.
pub(crate) async fn fresh_redis_cache() -> Result<RedisCache, CacheError> {
    let pool = redis_pool().map_err(CacheError::Io)?;
    let conn = pool
        .get()
        .await
        .map_err(|e| CacheError::Io(e.to_string()))?;
    let mux = deadpool_redis::Connection::take(conn);
    Ok(RedisCache::new(mux))
}

/// Convenience wrappers so callers can use the mytheclipse `Cache` methods
/// directly without importing the trait twice.
pub async fn get(key: &str) -> Result<Option<Vec<u8>>, CacheError> {
    fresh_redis_cache().await?.get(key).await
}

pub async fn set(
    key: &str,
    value: Vec<u8>,
    ttl: Option<std::time::Duration>,
) -> Result<(), CacheError> {
    fresh_redis_cache().await?.set(key, value, ttl).await
}

pub async fn invalidate(key: &str) -> Result<(), CacheError> {
    fresh_redis_cache().await?.invalidate(key).await
}
