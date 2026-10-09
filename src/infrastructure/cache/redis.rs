//! Redis caching helpers — typed wrapper over `mytheclipse_cache`.
//!
//! The underlying byte-cache is `mytheclipse_cache::RedisCache` (which
//! implements the library `Cache` trait). This module keeps the ergonomic
//! typed JSON surface the use cases rely on (`get_or_set`, `set_with_ttl`)
//! while delegating the actual Redis commands to the library.

use mytheclipse_cache::{Cache as CacheTrait, CacheError};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

/// Default cache TTL in seconds (5 minutes).
pub const DEFAULT_CACHE_TTL: u64 = 300;

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
        match self.cache().await {
            Ok(cache) => match CacheTrait::get(&cache, key).await {
                Ok(Some(bytes)) => serde_json::from_slice(&bytes).ok(),
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
        let ttl = std::time::Duration::from_secs(ttl_secs);
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        CacheTrait::set(&cache, key, json, Some(ttl))
            .await
            .map_err(|e| e.to_string())?;
        debug!("Cache: set key {} with TTL {}s", key, ttl_secs);
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        CacheTrait::invalidate(&cache, key)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn exists(&self, key: &str) -> bool {
        self.get::<serde_json::Value>(key).await.is_some()
    }

    /// Get or set: returns cached value or computes and caches new value.
    ///
    /// The cache is best-effort: if the post-compute write fails (e.g. a
    /// transient Redis outage or a broken pooled connection), the freshly
    /// computed value is still returned rather than propagating a 500. Only a
    /// cache *read* failure is silently tolerated; a compute failure still
    /// propagates.
    pub async fn get_or_set<T, F, Fut>(
        &self,
        key: &str,
        ttl_secs: u64,
        compute: F,
    ) -> Result<T, String>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        if let Some(cached) = self.get::<T>(key).await {
            debug!("Cache hit: {}", key);
            return Ok(cached);
        }

        debug!("Cache miss: {}", key);
        let value = compute().await?;
        // Best-effort write: a failure here must not fail the request — the
        // value is already valid. Log and continue (read path swallows errors
        // too, so a broken cache degrades to cache-less, never to 500).
        if let Err(e) = self.set_with_ttl(key, &value, ttl_secs).await {
            debug!("Cache: failed to write {} (non-fatal): {}", key, e);
        }
        Ok(value)
    }
}

/// Create a cache key with prefix.
pub fn cache_key(prefix: &str, id: &str) -> String {
    format!("{}:{}", prefix, id)
}

#[async_trait::async_trait]
impl crate::domain::repository::CachePort for Cache {
    async fn get_or_set<T, F, Fut>(&self, key: &str, ttl_secs: u64, compute: F) -> Result<T, String>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, String>> + Send,
        Self: Sized,
    {
        Cache::get_or_set(self, key, ttl_secs, compute).await
    }
}
