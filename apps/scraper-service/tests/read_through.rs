//! Behaviour of the read-through cache policy in [`read_through`].
//!
//! The policy is best-effort by contract: a cache that is empty, holds garbage,
//! or cannot be written must never fail the request, and a hit must not
//! recompute.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use scraper_service::application::cache::read_through;
use scraper_service::domain::repository::CachePort;

struct FakeCache {
    entries: Mutex<HashMap<String, String>>,
    fail_writes: bool,
}

impl FakeCache {
    fn new(fail_writes: bool) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            fail_writes,
        }
    }

    fn seed(&self, key: &str, raw: &str) {
        self.entries
            .lock()
            .expect("cache mutex")
            .insert(key.to_string(), raw.to_string());
    }
}

impl CachePort for FakeCache {
    async fn get_cached(&self, key: &str) -> Option<String> {
        self.entries.lock().expect("cache mutex").get(key).cloned()
    }

    async fn set_cached(&self, key: &str, value: String, _ttl_secs: u64) -> Result<(), String> {
        if self.fail_writes {
            return Err("cache unavailable".to_string());
        }
        self.entries
            .lock()
            .expect("cache mutex")
            .insert(key.to_string(), value);
        Ok(())
    }
}

/// Counts how often the compute closure ran, so a hit is observable.
async fn compute_counting(cache: &FakeCache, calls: &AtomicU32) -> u32 {
    read_through(cache, "k", 60, || async {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(42)
    })
    .await
    .expect("compute succeeds")
}

#[tokio::test]
async fn miss_computes_then_hit_reuses_the_stored_value() {
    let cache = FakeCache::new(false);
    let calls = AtomicU32::new(0);

    assert_eq!(compute_counting(&cache, &calls).await, 42);
    assert_eq!(compute_counting(&cache, &calls).await, 42);

    assert_eq!(calls.load(Ordering::SeqCst), 1, "second call must hit");
    assert_eq!(
        cache.get_cached("k").await.as_deref(),
        Some("42"),
        "computed value is stored as JSON"
    );
}

#[tokio::test]
async fn undecodable_entry_recomputes_instead_of_failing() {
    let cache = FakeCache::new(false);
    cache.seed("k", "{not json");
    let calls = AtomicU32::new(0);

    assert_eq!(compute_counting(&cache, &calls).await, 42);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn write_failure_still_returns_the_computed_value() {
    let cache = FakeCache::new(true);
    let calls = AtomicU32::new(0);

    assert_eq!(compute_counting(&cache, &calls).await, 42);
    assert_eq!(compute_counting(&cache, &calls).await, 42);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "no write means no hit");
}

#[tokio::test]
async fn compute_failure_propagates() {
    let cache = FakeCache::new(false);

    let result = read_through::<u32, _, _, _>(&cache, "k", 60, || async {
        Err("upstream down".to_string())
    })
    .await;

    assert_eq!(result, Err("upstream down".to_string()));
}
