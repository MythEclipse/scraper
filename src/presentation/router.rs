//! Router assembly — the composition root for all HTTP routes.
//!
//! Each module owns the routes it dispatches to (see `handler::*::router`),
//! so this file only decides *where* each module is mounted. Adding an
//! endpoint touches one handler module, never this file.

use axum::Router;
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::observability::metrics::otel_metrics_middleware;
use crate::observability::openapi::ApiDoc;
use crate::observability::openapi_modules::ModuleApiDoc;
use crate::presentation::handler;

/// Build the main application router with all routes, middleware, and Swagger UI.
///
/// Handlers take no shared state: every one of them resolves its own
/// repository from the process-wide singletons, so there is nothing to inject.
pub fn build_router() -> Router {
    let mut openapi = ApiDoc::openapi();
    openapi.merge(ModuleApiDoc::openapi());

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
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", openapi))
        .layer(axum::middleware::from_fn(otel_metrics_middleware))
        .layer(CompressionLayer::new().quality(CompressionLevel::Fastest))
        .layer(CorsLayer::permissive())
}
