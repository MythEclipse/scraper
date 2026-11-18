use utoipa::OpenApi;

/// Bridge for the auto-generated OpenAPI documentation.
/// This allows merging manual schema definitions with the auto-discovered handlers and schemas.
#[derive(OpenApi)]
#[openapi(
    // Discovered routes are merged at runtime in bootstrap/mod.rs
    info(
        title = "Scraper API",
        version = "1.0.0",
        description = "High-performance scraping and CDN microservice"
    )
)]
pub struct ApiDoc;

/// The one merged document the router serves at `/api-docs/openapi.json`.
///
/// Both the Swagger UI and the status dashboard read this, so the dashboard's
/// endpoint catalog cannot drift from the documentation: it *is* the
/// documentation, annotated with live counters.
pub fn combined() -> utoipa::openapi::OpenApi {
    let mut openapi = ApiDoc::openapi();
    openapi.merge(super::openapi_modules::ModuleApiDoc::openapi());
    openapi
}
