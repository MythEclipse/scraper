//! Health check endpoint.
use axum::Router;
use axum::routing::get;
use tracing::instrument;
use utoipa::ToSchema;

use axum::{Json, http::StatusCode};
use serde::Serialize;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// GET /health — liveness probe.
///
/// Answers only "this process is up and routing": it touches no upstream, no
/// Redis and no database, so a supervisor restarting on this signal never
/// restarts the service because a third party is down.
#[utoipa::path(
    get,
    path = "/health",
    tag = "service",
    operation_id = "health_check",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse),
    )
)]
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
