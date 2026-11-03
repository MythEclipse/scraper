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
