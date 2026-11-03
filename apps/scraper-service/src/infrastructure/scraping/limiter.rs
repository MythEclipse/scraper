//! Bounded concurrency for outbound scraping.
//!
//! mytheclipse's `ConcurrencyLimiter` is sync-only (std Mutex + Condvar), so
//! in async context we use a tokio `Semaphore` — the same primitive the
//! mytheclipse queue/backpressure crates build on — exposed as a small
//! RAII guard. This caps concurrent outbound scrapes so burst traffic can't
//! saturate upstream sites.

use std::sync::Arc;
use std::sync::OnceLock;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Default max concurrent outbound fetch-with-proxy operations.
pub const DEFAULT_FETCH_CONCURRENCY: usize = 16;

/// A tokio-semaphore based concurrency limiter (async-safe).
#[derive(Clone)]
pub struct FetchLimiter {
    sem: Arc<Semaphore>,
}

impl FetchLimiter {
    /// Builds a limiter allowing at most `max` concurrent permits.
    pub fn new(max: usize) -> Self {
        Self {
            sem: Arc::new(Semaphore::new(max.max(1))),
        }
    }

    /// Acquires a permit, awaiting if the limiter is saturated.
    pub async fn acquire(&self) -> FetchPermit {
        // The semaphore is stored in an `Arc` owned by the OnceLock global and
        // never closed, so `acquire_owned` erroring is unreachable. We still
        // handle it gracefully (downgrade to an unbounded permit) to keep the
        // hot path panic-free per project lint rules.
        match self.sem.clone().acquire_owned().await {
            Ok(permit) => FetchPermit {
                _permit: Some(permit),
            },
            Err(_) => FetchPermit { _permit: None },
        }
    }

    /// Attempts to acquire without waiting. Returns `None` if saturated.
    pub fn try_acquire(&self) -> Option<FetchPermit> {
        match self.sem.clone().try_acquire_owned() {
            Ok(permit) => Some(FetchPermit {
                _permit: Some(permit),
            }),
            Err(_) => None,
        }
    }

    /// How many permits are currently held.
    pub fn in_use(&self) -> usize {
        self.sem.available_permits()
    }
}

/// RAII guard holding a fetch concurrency permit.
#[must_use = "dropping the permit releases the slot"]
pub struct FetchPermit {
    _permit: Option<OwnedSemaphorePermit>,
}

/// Fetch limiter for the proxy-fetch hot path (global, lazily initialized).
pub fn fetch_limiter() -> &'static FetchLimiter {
    static LIMITER: OnceLock<FetchLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| FetchLimiter::new(DEFAULT_FETCH_CONCURRENCY))
}
