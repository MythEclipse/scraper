//! Rate limiting middleware — backed by `mytheclipse::RateLimiter`.
//!
//! The limiter is a token bucket from the mytheclipse core crate. The
//! middleware keeps the same axum shape (State<Arc<RateLimiter>>) and
//! `check()` semantics, but delegates token accounting to the library.

use axum::{
    Json,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use mytheclipse::RateLimiter;
use std::sync::Arc;

use crate::presentation::dto::common::ApiResponse;

/// Convenience alias so callers don't need the mytheclipse import.
pub type AppRateLimiter = RateLimiter;

/// Build a rate limiter (rate = requests/sec, burst = max burst capacity).
pub fn new_rate_limiter(rate_per_sec: f64, burst: u64) -> Arc<RateLimiter> {
    Arc::new(RateLimiter::new(rate_per_sec, burst))
}

/// Compatibility constructor matching the old (max_requests, window_secs) API.
/// Converts a fixed window into an equivalent token-bucket rate.
pub fn new_window_rate_limiter(max_requests: u64, window_secs: u64) -> Arc<RateLimiter> {
    let rate_per_sec = max_requests as f64 / window_secs.max(1) as f64;
    new_rate_limiter(rate_per_sec, max_requests)
}

pub async fn rate_limit_middleware(
    state: axum::extract::State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    if state.try_acquire().is_ok() {
        next.run(request).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<()>::error("Rate limit exceeded".to_string())),
        )
            .into_response()
    }
}
