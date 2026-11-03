//! HTTP retry utilities — delegated to mytheclipse `retry` primitives.
//!
//! The scraper uses mytheclipse's retry machinery (`RetryConfig` +
//! `mytheclipse::retry`). These helpers keep the old backoff-style call
//! sites ergonomic while delegating the actual backoff/sleep/jitter logic
//! to the library.

use mytheclipse::{JitterKind, RetryConfig};
use std::time::Duration;

/// Default retry configuration for HTTP requests.
pub fn default_backoff() -> RetryConfig {
    RetryConfig {
        max_attempts: 4,
        base_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(10),
        factor: 2.0,
        jitter: JitterKind::Full,
    }
}

/// Create a custom exponential backoff.
pub fn custom_backoff(
    initial_ms: u64,
    max_secs: u64,
    multiplier: f64,
    max_elapsed_secs: u64,
) -> RetryConfig {
    RetryConfig {
        max_attempts: compute_attempts(initial_ms, max_secs, multiplier, max_elapsed_secs).max(1),
        base_delay: Duration::from_millis(initial_ms),
        max_delay: Duration::from_secs(max_secs),
        factor: multiplier,
        jitter: JitterKind::Full,
    }
}

/// Estimate the number of attempts that fit in `max_elapsed_secs` given the
/// exponential backoff curve: solve `sum(base * factor^i) ≈ max_elapsed`.
fn compute_attempts(initial_ms: u64, max_secs: u64, multiplier: f64, max_elapsed_secs: u64) -> u32 {
    if initial_ms == 0 || multiplier <= 1.0 {
        return 1;
    }
    let mut elapsed_ms = 0u64;
    let mut attempt = 0u64;
    let max_ms = max_secs.saturating_mul(1000);
    let budget_ms = max_elapsed_secs.saturating_mul(1000);
    let mut delay_ms = initial_ms;
    while elapsed_ms < budget_ms {
        elapsed_ms = elapsed_ms.saturating_add(delay_ms);
        attempt += 1;
        delay_ms = ((delay_ms as f64) * multiplier).min(max_ms as f64) as u64;
    }
    attempt as u32
}

/// Quick backoff for fast retries (3 attempts, 100ms initial).
pub fn quick_backoff() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        base_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(1),
        factor: 2.0,
        jitter: JitterKind::Full,
    }
}

/// Slow backoff for long operations (10 attempts, 1s initial).
pub fn slow_backoff() -> RetryConfig {
    RetryConfig {
        max_attempts: 10,
        base_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(30),
        factor: 2.0,
        jitter: JitterKind::Full,
    }
}

/// Re-export mytheclipse retry for convenience.
pub use mytheclipse::retry;

/// Re-export transient/permanent helpers for API compatibility.
///
/// The legacy `backoff` crate distinguished transient vs permanent errors at
/// the error-type level; mytheclipse uses a retry predicate instead. All
/// scraped-HTTP failures are transient by nature (network/5xx), so both
/// helpers return the error unchanged and every call site retries everything
/// (`|_| true`). `permanent` is kept as a no-op alias for source
/// compatibility.
pub fn transient<E>(e: E) -> E {
    e
}

/// No-op alias for source compatibility (see [`transient`]).
pub fn permanent<E>(e: E) -> E {
    e
}

/// Predicate used by all scraper retry loops: retry every error.
pub fn retry_all<E>(_: &E) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_configs_build() {
        assert_eq!(default_backoff().max_attempts, 4);
        assert_eq!(quick_backoff().max_attempts, 3);
        assert_eq!(slow_backoff().max_attempts, 10);
    }

    #[test]
    fn compute_attempts_curve() {
        // 100ms base, 2x, 1s max, 5s budget → roughly 6 attempts.
        let n = compute_attempts(100, 1, 2.0, 5);
        assert!(n >= 4 && n <= 8, "got {n}");
    }

    #[test]
    fn retry_all_retries() {
        assert!(retry_all::<std::io::Error>(&std::io::Error::other("x")));
    }
}
