//! Router assembly — the composition root for all HTTP routes.
//!
//! Each module owns the routes it dispatches to (see `handler::*::router`),
//! so this file only decides *where* each module is mounted. Adding an
//! endpoint touches one handler module, never this file.

use axum::Router;
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::cors::CorsLayer;
use utoipa_swagger_ui::SwaggerUi;

use crate::observability::metrics::otel_metrics_middleware;
use crate::observability::openapi;
use crate::observability::telemetry::telemetry_middleware;
use crate::presentation::handler;

/// Build the main application router with all routes, middleware, and Swagger UI.
///
/// Handlers take no shared state: every one of them resolves its own
/// repository from the process-wide singletons, so there is nothing to inject.
///
/// Layer order matters. `telemetry_middleware` is added *last* so it is the
/// outermost layer: it then wraps the 404 handler as well as every real route,
/// and its `MatchedPath` lookup happens after routing has resolved the template.
pub fn build_router() -> Router {
    let openapi = openapi::combined();

    Router::new()
        .nest("/api/anime", handler::anime::router())
        .nest("/api/anime2", handler::anime2::router())
        .nest("/api/komik", handler::komik::router())
        .merge(handler::downloader::router())
        .nest("/misc", handler::misc::router())
        .nest("/stalk", handler::stalk::router())
        .nest("/search", handler::search::router())
        .nest("/tool", handler::tools::router())
        .nest("/image", handler::image::router())
        .merge(handler::health::router())
        // Homepage + public status dashboard. Merged last among the routes so a
        // module route always wins over the catch-all `/` of the dashboard.
        .merge(handler::dashboard::router())
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(axum::middleware::from_fn(otel_metrics_middleware))
        .layer(CompressionLayer::new().quality(CompressionLevel::Fastest))
        .layer(CorsLayer::permissive())
        .layer(axum::middleware::from_fn(telemetry_middleware))
}
