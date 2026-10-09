//! Komik API handlers.
use axum::routing::get;
use axum::Router;

use axum::extract::Path;
use axum::Json;
use tracing::info;

use crate::application::komik::use_cases::{new_use_cases, KomikUseCases};
use crate::domain::error::AppError;
use crate::infrastructure::repository::KomikRepository;
use crate::presentation::dto::komik::{
    ChapterResponse, DetailResponse, GenreKomikResponse, GenresResponse, SearchKomikResponse,
};

// ============================================================================
// Response DTOs
// ============================================================================

// Response types re-exported from application::komik::use_cases.

// ============================================================================
// Helper
// ============================================================================

fn make_use_cases() -> KomikUseCases<KomikRepository> {
    new_use_cases(KomikRepository::new())
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/komik/genre_list — List all komik genres.
#[utoipa::path(
    get,
    path = "/api/komik/genre_list",
    tag = "komik",
    operation_id = "komik_genre_list",
    responses(
        (status = 200, description = "Genre list", body = GenresResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genre_list() -> Result<Json<GenresResponse>, AppError> {
    info!("Handling request for komik genre list");
    let data = make_use_cases().genre_list().await?;
    Ok(Json(GenresResponse {
        status: "Ok".to_string(),
        data,
    }))
}

/// GET /api/komik/chapter/{slug} — Read a chapter.
#[utoipa::path(
    get,
    path = "/api/komik/chapter/{slug}",
    tag = "komik",
    operation_id = "komik_chapter_slug",
    responses(
        (status = 200, description = "Chapter data", body = ChapterResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn chapter_slug(Path(slug): Path<String>) -> Result<Json<ChapterResponse>, AppError> {
    info!("Handling request for komik chapter slug: {}", slug);
    let data = make_use_cases().chapter_slug(slug).await?;
    Ok(Json(ChapterResponse {
        message: "Ok".to_string(),
        data,
    }))
}

/// GET /api/komik/detail/{slug} — Komik detail by slug.
#[utoipa::path(
    get,
    path = "/api/komik/detail/{slug}",
    tag = "komik",
    operation_id = "komik_detail_slug",
    responses(
        (status = 200, description = "Komik detail", body = DetailResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn detail_slug(Path(slug): Path<String>) -> Result<Json<DetailResponse>, AppError> {
    info!("Handling request for komik detail slug: {}", slug);
    let data = make_use_cases().detail_slug(slug).await?;
    Ok(Json(DetailResponse { status: true, data }))
}

/// GET /api/komik/genre/{slug} — Genre-filtered komik list (first page).
#[utoipa::path(
    get,
    path = "/api/komik/genre/{slug}",
    tag = "komik",
    operation_id = "komik_genre_slug_index",
    responses(
        (status = 200, description = "Genre page", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genre_slug(Path(slug): Path<String>) -> Result<Json<GenreKomikResponse>, AppError> {
    info!("Handling request for komik genre slug: {}", slug);
    let slug_clone = slug.clone();
    let (data, pagination) = make_use_cases().genre_slug(slug).await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: slug_clone,
        data,
        pagination,
    }))
}

/// GET /api/komik/genre/{slug}/{page} — Paginated genre results.
#[utoipa::path(
    get,
    path = "/api/komik/genre/{slug}/{page}",
    tag = "komik",
    operation_id = "komik_genre_slug_page",
    responses(
        (status = 200, description = "Genre page with page", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genre_slug_page(
    Path((slug, page)): Path<(String, String)>,
) -> Result<Json<GenreKomikResponse>, AppError> {
    let page_num = page
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!(
        "Handling request for komik genre slug: {} page: {}",
        slug, page_num
    );
    let (data, pagination) = make_use_cases()
        .genre_slug_page(slug.clone(), page_num)
        .await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: slug,
        data,
        pagination,
    }))
}

/// GET /api/komik/manga/{slug} — Manga list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/komik/manga/{slug}",
    tag = "komik",
    operation_id = "komik_manga_slug",
    responses(
        (status = 200, description = "Manga list", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn manga_slug(Path(slug): Path<String>) -> Result<Json<GenreKomikResponse>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!("Handling request for komik manga page: {}", page);
    let (data, pagination) = make_use_cases().manga_slug(page.to_string()).await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: "manga".to_string(),
        data,
        pagination,
    }))
}

/// GET /api/komik/manhua/{slug} — Manhua list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/komik/manhua/{slug}",
    tag = "komik",
    operation_id = "komik_manhua_slug",
    responses(
        (status = 200, description = "Manhua list", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn manhua_slug(Path(slug): Path<String>) -> Result<Json<GenreKomikResponse>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!("Handling request for komik manhua page: {}", page);
    let (data, pagination) = make_use_cases().manhua_slug(page.to_string()).await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: "manhua".to_string(),
        data,
        pagination,
    }))
}

/// GET /api/komik/manhwa/{slug} — Manhwa list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/komik/manhwa/{slug}",
    tag = "komik",
    operation_id = "komik_manhwa_slug",
    responses(
        (status = 200, description = "Manhwa list", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn manhwa_slug(Path(slug): Path<String>) -> Result<Json<GenreKomikResponse>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!("Handling request for komik manhwa page: {}", page);
    let (data, pagination) = make_use_cases().manhwa_slug(page.to_string()).await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: "manhwa".to_string(),
        data,
        pagination,
    }))
}

/// GET /api/komik/popular/{slug} — Popular komik list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/komik/popular/{slug}",
    tag = "komik",
    operation_id = "komik_popular_slug",
    responses(
        (status = 200, description = "Popular komik list", body = GenreKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn popular_slug(Path(slug): Path<String>) -> Result<Json<GenreKomikResponse>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!("Handling request for komik popular page: {}", page);
    let (data, pagination) = make_use_cases().popular_slug(page.to_string()).await?;
    Ok(Json(GenreKomikResponse {
        status: "Ok".to_string(),
        genre: "popular".to_string(),
        data,
        pagination,
    }))
}

/// GET /api/komik/search/{slug} — Search komik (first page).
#[utoipa::path(
    get,
    path = "/api/komik/search/{slug}",
    tag = "komik",
    operation_id = "komik_search_slug_index",
    responses(
        (status = 200, description = "Search results", body = SearchKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn search_slug(Path(slug): Path<String>) -> Result<Json<SearchKomikResponse>, AppError> {
    info!("Handling request for komik search slug: {}", slug);
    let (data, pagination) = make_use_cases().search_slug(slug).await?;
    Ok(Json(SearchKomikResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

/// GET /api/komik/search/{slug}/{page} — Paginated search results.
#[utoipa::path(
    get,
    path = "/api/komik/search/{slug}/{page}",
    tag = "komik",
    operation_id = "komik_search_slug_page",
    responses(
        (status = 200, description = "Search results with page", body = SearchKomikResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn search_slug_page(
    Path((slug, page)): Path<(String, String)>,
) -> Result<Json<SearchKomikResponse>, AppError> {
    let page_num = page
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError("Invalid page number".to_string()))?;
    info!(
        "Handling request for komik search slug: {} page: {}",
        slug, page_num
    );
    let (data, pagination) = make_use_cases().search_slug_page(slug, page_num).await?;
    Ok(Json(SearchKomikResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/api/komik` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/genre_list", get(genre_list))
        .route("/chapter/{slug}", get(chapter_slug))
        .route("/detail/{slug}", get(detail_slug))
        .route("/genre/{slug}", get(genre_slug))
        .route("/genre/{slug}/{page}", get(genre_slug_page))
        .route("/manga/{slug}", get(manga_slug))
        .route("/manhua/{slug}", get(manhua_slug))
        .route("/manhwa/{slug}", get(manhwa_slug))
        .route("/popular/{slug}", get(popular_slug))
        .route("/search/{slug}", get(search_slug))
        .route("/search/{slug}/{page}", get(search_slug_page))
}
