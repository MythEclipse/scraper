//! Redis caching helpers — typed wrapper over `mytheclipse_cache`.
//!
//! The underlying byte-cache is `mytheclipse_cache::RedisCache` (which
//! implements the library `Cache` trait). This module keeps the ergonomic
//! typed JSON surface (`get`, `set_with_ttl`) plus the [`CachePort`] primitives
//! while delegating the actual Redis commands to the library.

use mytheclipse_cache::{Cache as CacheTrait, CacheError, MemoryCache};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::LazyLock;
use std::time::Duration;
use tracing::debug;

/// Default cache TTL in seconds (5 minutes).
pub const DEFAULT_CACHE_TTL: u64 = 300;

/// In-process L1 in front of Redis.
///
/// Every cached request used to pay a deadpool checkout plus a Redis round trip
/// just to read bytes this process had already serialized moments earlier. The
/// L1 turns a repeat hit into a `HashMap` probe under a shared lock.
///
/// `with_max_entries` bounds memory: entries are evicted oldest-first past the
/// cap, so a scrape of thousands of distinct slugs cannot grow the map without
/// limit. 4096 entries is a few MB at typical response sizes.
static L1: LazyLock<MemoryCache> = LazyLock::new(|| MemoryCache::new().with_max_entries(4096));

/// Longest a value may be served from process memory.
///
/// The effective L1 TTL is always `min(caller_ttl, L1_TTL_CAP)`, so the L1 can
/// only ever expire a value *sooner* than Redis would have dropped it. That
/// makes the layer invisible to callers: it never serves anything the caller
/// had not already declared fresh, and it bounds how far one instance can drift
/// from the shared cache after another instance writes.
const L1_TTL_CAP: Duration = Duration::from_secs(10);

async fn l1_get(key: &str) -> Option<Vec<u8>> {
    CacheTrait::get(&*L1, key).await.ok().flatten()
}

/// Mirror a value into L1 with the caller's TTL capped at [`L1_TTL_CAP`].
async fn l1_set(key: &str, value: Vec<u8>, ttl_secs: u64) {
    let ttl = Duration::from_secs(ttl_secs).min(L1_TTL_CAP);
    let _ = CacheTrait::set(&*L1, key, value, Some(ttl)).await;
}

/// Forget a key in L1 as well as Redis.
async fn l1_invalidate(key: &str) {
    let _ = CacheTrait::invalidate(&*L1, key).await;
}

/// Typed JSON cache helper over the shared mytheclipse `RedisCache`.
///
/// The helper is stateless: every operation checks out a connection from the
/// process-wide deadpool (see
/// [`fresh_redis_cache`](super::mytheclipse::fresh_redis_cache)), so there is
/// nothing to configure and nothing to hold on to.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cache;

impl Cache {
    pub const fn new() -> Self {
        Self
    }

    async fn cache(&self) -> Result<mytheclipse_cache::RedisCache, CacheError> {
        super::mytheclipse::fresh_redis_cache().await
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        if let Some(bytes) = l1_get(key).await {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return Some(value);
            }
            // Undecodable L1 entry (schema change after a deploy): drop it and
            // fall through to Redis rather than serving a stale shape.
            l1_invalidate(key).await;
        }

        match self.cache().await {
            Ok(cache) => match CacheTrait::get(&cache, key).await {
                Ok(Some(bytes)) => {
                    // Backfill L1. The remaining Redis TTL is unknown here, so
                    // the capped TTL is the safe upper bound.
                    l1_set(key, bytes.clone(), L1_TTL_CAP.as_secs()).await;
                    serde_json::from_slice(&bytes).ok()
                }
                Ok(None) => None,
                Err(e) => {
                    debug!("Cache: get error for {}: {}", key, e);
                    None
                }
            },
            Err(e) => {
                debug!("Cache: unavailable for {}: {}", key, e);
                None
            }
        }
    }

    pub async fn mget<T: DeserializeOwned>(&self, keys: &[String]) -> Vec<Option<T>> {
        if keys.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            out.push(self.get::<T>(k).await);
        }
        out
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        self.set_with_ttl(key, value, DEFAULT_CACHE_TTL).await
    }

    pub async fn set_with_ttl<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: u64,
    ) -> Result<(), String> {
        let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        // L1 first: it cannot fail, and it is what the next request will hit.
        l1_set(key, json.clone(), ttl_secs).await;
        let ttl = std::time::Duration::from_secs(ttl_secs);
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        CacheTrait::set(&cache, key, json, Some(ttl))
            .await
            .map_err(|e| e.to_string())?;
        debug!("Cache: set key {} with TTL {}s", key, ttl_secs);
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        l1_invalidate(key).await;
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        CacheTrait::invalidate(&cache, key)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn exists(&self, key: &str) -> bool {
        self.get::<serde_json::Value>(key).await.is_some()
    }
}

/// Create a cache key with prefix.
pub fn cache_key(prefix: &str, id: &str) -> String {
    format!("{}:{}", prefix, id)
}

impl crate::domain::repository::CachePort for Cache {
    async fn get_cached(&self, key: &str) -> Option<String> {
        if let Some(bytes) = l1_get(key).await {
            match String::from_utf8(bytes) {
                Ok(text) => return Some(text),
                Err(err) => {
                    // Undecodable L1 entry: drop it and fall through to Redis,
                    // which may still hold a value written by another instance.
                    l1_invalidate(key).await;
                    debug!("Cache: L1 entry for {} was not UTF-8: {}", key, err);
                }
            }
        }

        let cache = match self.cache().await {
            Ok(cache) => cache,
            Err(e) => {
                debug!("Cache: unavailable for {}: {}", key, e);
                return None;
            }
        };
        match CacheTrait::get(&cache, key).await {
            Ok(Some(bytes)) => {
                // Backfill L1 with the capped TTL, then hand the original
                // buffer to the caller — no second copy for the common case.
                l1_set(key, bytes.clone(), L1_TTL_CAP.as_secs()).await;
                Some(String::from_utf8_lossy(&bytes).into_owned())
            }
            Ok(None) => None,
            Err(e) => {
                debug!("Cache: get error for {}: {}", key, e);
                None
            }
        }
    }

    async fn set_cached(&self, key: &str, value: String, ttl_secs: u64) -> Result<(), String> {
        l1_set(key, value.clone().into_bytes(), ttl_secs).await;
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        let ttl = std::time::Duration::from_secs(ttl_secs);
        CacheTrait::set(&cache, key, value.into_bytes(), Some(ttl))
            .await
            .map_err(|e| e.to_string())?;
        debug!("Cache: set key {} with TTL {}s", key, ttl_secs);
        Ok(())
    }
}
