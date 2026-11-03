//! Health check endpoint.
use axum::Router;
use axum::routing::get;
use tracing::instrument;

use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[instrument(skip_all)]
pub async fn health_check() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    )
}

/// Liveness probe route.
pub fn router() -> Router {
    Router::new().route("/health", get(health_check))
}
