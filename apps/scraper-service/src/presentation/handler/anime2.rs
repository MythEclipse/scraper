//! Anime2 (Alqanime) API handlers.
use crate::presentation::validation::validate;
use axum::Router;
use axum::routing::get;
use zod_rs::prelude::{Schema as _, ZodSchema};

use axum::Json;
use axum::extract::{Path, Query};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::{IntoParams, ToSchema};

use crate::application::anime2::use_cases::{Anime2UseCases, new_use_cases};
use crate::infrastructure::cache::redis::Cache;
use crate::infrastructure::repository::AlqanimeRepository;
use crate::presentation::dto::anime2::{
    Anime2Response, DetailResponse, FilterResponse, FiltersApplied, GenresResponse,
};
use crate::presentation::dto::common::ApiResponse;
use crate::presentation::dto::entities::anime::{
    CompleteAnimeItemDto, GenreAnimeItemDto, LatestAnimeItemDto, OngoingAnimeItemWithScoreDto,
    SearchAnimeItemDto,
};
use crate::presentation::error::AppError;

// ============================================================================
// Request DTOs
// ============================================================================

/// Filter query parameters for the anime2 filter endpoint.
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema, ZodSchema, Serialize)]
pub struct FilterQuery {
    pub page: Option<u32>,
    #[zod(min_length(1), max_length(64))]
    pub genre: Option<String>,
    pub status: Option<String>,
    pub r#type: Option<String>,
    pub order: Option<String>,
}

// ============================================================================
// Helper
// ============================================================================

fn make_use_cases() -> Anime2UseCases<AlqanimeRepository, Cache> {
    new_use_cases(AlqanimeRepository, Cache)
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/anime2 — Anime2 index (ongoing + complete).
#[utoipa::path(
    get,
    path = "/api/anime2",
    tag = "anime2",
    operation_id = "anime2_index",
    responses(
        (status = 200, description = "Anime2 index", body = Anime2Response),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all)]
pub async fn index() -> Result<Json<Anime2Response>, AppError> {
    info!("Handling request for anime2 index");
    let data = make_use_cases().index().await?;
    Ok(Json(Anime2Response {
        status: "Ok".to_string(),
        data: data.into(),
    }))
}

/// GET /api/anime2/genre_list — List all genres.
#[utoipa::path(
    get,
    path = "/api/anime2/genre_list",
    tag = "anime2",
    operation_id = "anime2_genre_list",
    responses(
        (status = 200, description = "Genre list", body = GenresResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all)]
pub async fn genre_list() -> Result<Json<GenresResponse>, AppError> {
    info!("Handling request for anime2 genre list");
    let data = make_use_cases().genre_list().await?;
    Ok(Json(GenresResponse {
        status: "Ok".to_string(),
        data: data.into_iter().map(Into::into).collect(),
    }))
}

/// GET /api/anime2/filter?page=&genre=&status=&type=&order= — Filter anime.
#[utoipa::path(
    get,
    path = "/api/anime2/filter",
    tag = "anime2",
    operation_id = "anime2_filter",
    params(FilterQuery),
    responses(
        (status = 200, description = "Filter results", body = FilterResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn filter(Query(params): Query<FilterQuery>) -> Result<Json<FilterResponse>, AppError> {
    let params = validate(params)?;
    info!("Handling request for anime2 filter");
    let page = params.page.unwrap_or(1);
    let genre = params.genre.clone();
    let status = params.status.clone();
    let anime_type = params.r#type.clone();
    let order = params.order.clone().unwrap_or_else(|| "update".to_string());

    let (data, pagination) = make_use_cases()
        .filter(
            page,
            genre.clone(),
            status.clone(),
            anime_type.clone(),
            order.clone(),
        )
        .await?;
    Ok(Json(FilterResponse {
        success: true,
        data: data.into_iter().map(Into::into).collect(),
        pagination: pagination.into(),
        filters_applied: FiltersApplied {
            genre,
            status,
            r#type: anime_type,
            order: Some(order),
        },
        status: "Ok".to_string(),
    }))
}

/// GET /api/anime2/detail/{slug} — Anime detail by slug.
#[utoipa::path(
    get,
    path = "/api/anime2/detail/{slug}",
    tag = "anime2",
    operation_id = "anime2_detail_slug",
    responses(
        (status = 200, description = "Anime detail", body = DetailResponse),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn detail_slug(Path(slug): Path<String>) -> Result<Json<DetailResponse>, AppError> {
    info!("Handling request for anime2 detail slug: {}", slug);
    let data = make_use_cases().detail(slug).await?;
    Ok(Json(DetailResponse {
        status: "Ok".to_string(),
        data: data.into(),
    }))
}

/// GET /api/anime2/genre/{slug} — First page of genre-filtered results.
#[utoipa::path(
    get,
    path = "/api/anime2/genre/{slug}",
    tag = "anime2",
    operation_id = "anime2_genre_slug_index",
    responses(
        (status = 200, description = "Genre page", body = ApiResponse<Vec<GenreAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn genre_slug_index(
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<GenreAnimeItemDto>>>, AppError> {
    info!("Handling request for anime2 genre slug: {}", slug);
    let data = make_use_cases().genre_slug(slug, 1).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/genre/{slug}/{page} — Paginated genre results.
#[utoipa::path(
    get,
    path = "/api/anime2/genre/{slug}/{page}",
    tag = "anime2",
    operation_id = "anime2_genre_slug_page",
    responses(
        (status = 200, description = "Genre page with page", body = ApiResponse<Vec<GenreAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug, page = %page))]
pub async fn genre_slug_page(
    Path((slug, page)): Path<(String, u32)>,
) -> Result<Json<ApiResponse<Vec<GenreAnimeItemDto>>>, AppError> {
    info!(
        "Handling request for anime2 genre slug: {} page: {}",
        slug, page
    );
    let data = make_use_cases().genre_slug(slug, page).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/search/{slug} — Search anime (first page).
#[utoipa::path(
    get,
    path = "/api/anime2/search/{slug}",
    tag = "anime2",
    operation_id = "anime2_search_slug_index",
    responses(
        (status = 200, description = "Search results", body = ApiResponse<Vec<SearchAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn search_slug_index(
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<SearchAnimeItemDto>>>, AppError> {
    info!("Handling request for anime2 search slug: {}", slug);
    let data = make_use_cases().search(slug, 1).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/search/{slug}/{page} — Paginated search results.
#[utoipa::path(
    get,
    path = "/api/anime2/search/{slug}/{page}",
    tag = "anime2",
    operation_id = "anime2_search_slug_page",
    responses(
        (status = 200, description = "Search results with page", body = ApiResponse<Vec<SearchAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug, page = %page))]
pub async fn search_slug_page(
    Path((slug, page)): Path<(String, u32)>,
) -> Result<Json<ApiResponse<Vec<SearchAnimeItemDto>>>, AppError> {
    info!(
        "Handling request for anime2 search slug: {} page: {}",
        slug, page
    );
    let data = make_use_cases().search(slug, page).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/latest/{slug} — Latest anime (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/anime2/latest/{slug}",
    tag = "anime2",
    operation_id = "anime2_latest_slug",
    responses(
        (status = 200, description = "Latest anime page", body = ApiResponse<Vec<LatestAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn latest_slug(
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<LatestAnimeItemDto>>>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError(format!("Invalid page number: {}", slug)))?;
    info!("Handling request for anime2 latest page: {}", page);
    let data = make_use_cases().latest(page).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/ongoing_anime/{slug} — Ongoing anime list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/anime2/ongoing_anime/{slug}",
    tag = "anime2",
    operation_id = "anime2_ongoing_anime_slug",
    responses(
        (status = 200, description = "Ongoing anime page", body = ApiResponse<Vec<OngoingAnimeItemWithScoreDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn ongoing_anime_slug(
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<OngoingAnimeItemWithScoreDto>>>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError(format!("Invalid page number: {}", slug)))?;
    info!("Handling request for anime2 ongoing page: {}", page);
    let data = make_use_cases().ongoing_anime(page).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// GET /api/anime2/complete_anime/{slug} — Complete anime list (slug is the page number).
#[utoipa::path(
    get,
    path = "/api/anime2/complete_anime/{slug}",
    tag = "anime2",
    operation_id = "anime2_complete_anime_slug",
    responses(
        (status = 200, description = "Complete anime page", body = ApiResponse<Vec<CompleteAnimeItemDto>>),
        (status = 500, description = "Internal Server Error"),
    )
)]
#[instrument(skip_all, fields(slug = %slug))]
pub async fn complete_anime_slug(
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Vec<CompleteAnimeItemDto>>>, AppError> {
    let page = slug
        .parse::<u32>()
        .map_err(|_| AppError::ScraperError(format!("Invalid page number: {}", slug)))?;
    info!("Handling request for anime2 complete page: {}", page);
    let data = make_use_cases().complete_anime(page).await?;
    Ok(Json(ApiResponse::success(
        data.into_iter().map(Into::into).collect::<Vec<_>>(),
    )))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/api/anime2` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/complete_anime/{slug}", get(complete_anime_slug))
        .route("/detail/{slug}", get(detail_slug))
        .route("/filter", get(filter))
        .route("/genre_list", get(genre_list))
        .route("/genre/{slug}", get(genre_slug_index))
        .route("/genre/{slug}/{page}", get(genre_slug_page))
        .route("/latest/{slug}", get(latest_slug))
        .route("/ongoing_anime/{slug}", get(ongoing_anime_slug))
        .route("/search/{slug}", get(search_slug_index))
        .route("/search/{slug}/{page}", get(search_slug_page))
}
