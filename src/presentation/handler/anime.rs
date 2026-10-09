//! Anime (Otakudesu) API handlers.
use axum::routing::get;
use axum::Router;

use axum::extract::Path;
use axum::Json;
use serde::Serialize;
use tracing::info;
use utoipa::ToSchema;

use crate::application::anime::use_cases::AnimeUseCases;
use crate::domain::entity::anime::*;
use crate::infrastructure::repository::OtakudesuRepository;
use crate::presentation::error::AppError;

// ============================================================================
// Response DTOs
// ============================================================================

#[derive(Serialize, ToSchema)]
pub struct GenresResponse {
    pub status: String,
    pub data: Vec<Genre>,
}

#[derive(Serialize, ToSchema)]
pub struct DetailResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub data: AnimeDetailData,
}

#[derive(Serialize, ToSchema)]
pub struct ListResponse {
    pub message: String,
    pub data: Vec<CompleteAnimeListItem>,
    pub total: Option<i64>,
    pub pagination: Option<Pagination>,
}

#[derive(Serialize, ToSchema)]
pub struct FullResponse {
    pub status: String,
    pub data: AnimeFullData,
}

#[derive(Serialize, ToSchema)]
pub struct OngoingAnimeResponse {
    pub status: String,
    pub data: Vec<OngoingAnimeListItem>,
    pub pagination: Pagination,
}

#[derive(Serialize, ToSchema)]
pub struct LatestAnimeResponse {
    pub status: String,
    pub data: Vec<LatestAnimeItem>,
    pub pagination: Pagination,
}

#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    pub status: String,
    pub data: Vec<SearchAnimeItem>,
    pub pagination: Pagination,
}

#[derive(Serialize, ToSchema)]
pub struct GenreListResponse {
    pub status: String,
    pub data: Vec<GenreAnimeItem>,
    pub pagination: Pagination,
}

// ============================================================================
// Helper
// ============================================================================

fn make_use_cases() -> AnimeUseCases {
    AnimeUseCases::new(OtakudesuRepository::new())
}

// ============================================================================
// Handlers
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/anime",
    tag = "anime",
    responses(
        (status = 200, description = "Anime index", body = AnimeData),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn anime_index() -> Result<Json<AnimeData>, AppError> {
    info!("Handling request for anime index");
    let data = make_use_cases().get_anime_index().await?;
    Ok(Json(data))
}

#[utoipa::path(
    get,
    path = "/api/anime/genre_list",
    tag = "anime",
    responses(
        (status = 200, description = "Genre list"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genres() -> Result<Json<GenresResponse>, AppError> {
    info!("Handling request for anime genres");
    let data = make_use_cases().get_genres().await?;
    Ok(Json(GenresResponse {
        status: "Ok".to_string(),
        data,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/detail/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Anime detail"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn detail_slug(Path(slug): Path<String>) -> Result<Json<DetailResponse>, AppError> {
    info!("Starting request for detail slug: {}", slug);
    let data = make_use_cases().get_anime_detail(slug).await?;
    Ok(Json(DetailResponse {
        status: Some("Ok".to_string()),
        data,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/complete_anime/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Complete anime page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn complete_anime_slug(Path(slug): Path<String>) -> Result<Json<ListResponse>, AppError> {
    info!("Starting request for complete_anime slug: {}", slug);
    let (data, pagination) = make_use_cases().get_complete_anime_page(slug).await?;
    let total = data.len() as i64;
    Ok(Json(ListResponse {
        message: "Success".to_string(),
        data,
        total: Some(total),
        pagination: Some(pagination),
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/full/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Full episode details"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn full_slug(Path(slug): Path<String>) -> Result<Json<FullResponse>, AppError> {
    info!("Starting request for full slug: {}", slug);
    let data = make_use_cases().get_anime_full(slug).await?;
    Ok(Json(FullResponse {
        status: "Ok".to_string(),
        data,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/ongoing_anime/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Ongoing anime page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn ongoing_anime_slug(
    Path(slug): Path<String>,
) -> Result<Json<OngoingAnimeResponse>, AppError> {
    info!("Starting request for ongoing_anime slug: {}", slug);
    let (data, pagination) = make_use_cases().get_ongoing_anime_page(slug).await?;
    Ok(Json(OngoingAnimeResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/latest/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Latest anime page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn latest_slug(Path(slug): Path<String>) -> Result<Json<LatestAnimeResponse>, AppError> {
    info!("Starting request for latest slug: {}", slug);
    let (data, pagination) = make_use_cases().get_latest_anime_page(slug).await?;
    Ok(Json(LatestAnimeResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/search/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Search results"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn search_slug_index(Path(slug): Path<String>) -> Result<Json<SearchResponse>, AppError> {
    info!("Starting request for search slug: {}", slug);
    let (data, pagination) = make_use_cases()
        .get_search_anime_page(slug, "1".to_string())
        .await?;
    Ok(Json(SearchResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/search/{slug}/{page}",
    tag = "anime",
    responses(
        (status = 200, description = "Search results with page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn search_slug_page(
    Path((slug, page)): Path<(String, String)>,
) -> Result<Json<SearchResponse>, AppError> {
    info!("Starting request for search slug: {} page: {}", slug, page);
    let (data, pagination) = make_use_cases().get_search_anime_page(slug, page).await?;
    Ok(Json(SearchResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/genre/{slug}",
    tag = "anime",
    responses(
        (status = 200, description = "Genre page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genre_slug_index(
    Path(slug): Path<String>,
) -> Result<Json<GenreListResponse>, AppError> {
    info!("Starting request for genre slug: {}", slug);
    let (data, pagination) = make_use_cases()
        .get_genre_anime_page(slug, "1".to_string())
        .await?;
    Ok(Json(GenreListResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

#[utoipa::path(
    get,
    path = "/api/anime/genre/{slug}/{page}",
    tag = "anime",
    responses(
        (status = 200, description = "Genre page with page"),
        (status = 500, description = "Internal Server Error"),
    )
)]
pub async fn genre_slug_page(
    Path((slug, page)): Path<(String, String)>,
) -> Result<Json<GenreListResponse>, AppError> {
    info!("Starting request for genre slug: {} page: {}", slug, page);
    let (data, pagination) = make_use_cases().get_genre_anime_page(slug, page).await?;
    Ok(Json(GenreListResponse {
        status: "Ok".to_string(),
        data,
        pagination,
    }))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/api/anime` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/", get(anime_index))
        .route("/genre_list", get(genres))
        .route("/detail/{slug}", get(detail_slug))
        .route("/complete_anime/{slug}", get(complete_anime_slug))
        .route("/full/{slug}", get(full_slug))
        .route("/ongoing_anime/{slug}", get(ongoing_anime_slug))
        .route("/latest/{slug}", get(latest_slug))
        .route("/search/{slug}", get(search_slug_index))
        .route("/search/{slug}/{page}", get(search_slug_page))
        .route("/genre/{slug}", get(genre_slug_index))
        .route("/genre/{slug}/{page}", get(genre_slug_page))
}
