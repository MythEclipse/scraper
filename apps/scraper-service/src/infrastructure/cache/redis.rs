//! Redis caching helpers — typed wrapper over `mytheclipse_cache`.
//!
//! The underlying byte-cache is `mytheclipse_cache::RedisCache` (which
//! implements the library `Cache` trait). This module keeps the ergonomic
//! typed JSON surface (`get`, `set_with_ttl`) plus the [`CachePort`] primitives
//! while delegating the actual Redis commands to the library.

use mytheclipse_cache::{Cache as CacheTrait, CacheError};
use serde::{Serialize, de::DeserializeOwned};
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
}

/// Create a cache key with prefix.
pub fn cache_key(prefix: &str, id: &str) -> String {
    format!("{}:{}", prefix, id)
}

impl crate::domain::repository::CachePort for Cache {
    async fn get_cached(&self, key: &str) -> Option<String> {
        let cache = match self.cache().await {
            Ok(cache) => cache,
            Err(e) => {
                debug!("Cache: unavailable for {}: {}", key, e);
                return None;
            }
        };
        match CacheTrait::get(&cache, key).await {
            Ok(Some(bytes)) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            Ok(None) => None,
            Err(e) => {
                debug!("Cache: get error for {}: {}", key, e);
                None
            }
        }
    }

    async fn set_cached(&self, key: &str, value: String, ttl_secs: u64) -> Result<(), String> {
        let cache = self.cache().await.map_err(|e| e.to_string())?;
        let ttl = std::time::Duration::from_secs(ttl_secs);
        CacheTrait::set(&cache, key, value.into_bytes(), Some(ttl))
            .await
            .map_err(|e| e.to_string())?;
        debug!("Cache: set key {} with TTL {}s", key, ttl_secs);
        Ok(())
    }
}
