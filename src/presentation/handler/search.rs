//! Axum handlers for search utilities.
//!
//! Ported from Shirokami-API `scraper/search/*.js`.
use axum::routing::get;
use axum::Router;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::application::search as use_cases;
use crate::presentation::error::AppError;

#[derive(Deserialize)]
pub struct SearchParams {
    pub query: Option<String>,
    pub q: Option<String>,
    pub kota: Option<String>,
    pub city: Option<String>,
}

fn require(v: Option<&String>, name: &str) -> Result<String, AppError> {
    v.cloned()
        .ok_or_else(|| AppError::BadRequest(format!("Missing '{}' parameter", name)))
}

/// GET /search/bmkg
#[utoipa::path(
    get,
    path = "/search/bmkg",
    tag = "search",
    operation_id = "search_bmkg_handler",
    responses(
        (status = 200, description = "OK", body = serde_json::Value),
        (status = 400, description = "Bad Request"),
        (status = 502, description = "Upstream error"),
    )
)]

pub async fn bmkg_handler() -> Result<(StatusCode, Json<Value>), AppError> {
    Ok((StatusCode::OK, Json(use_cases::bmkg().await?)))
}

/// GET /search/jadwal-sholat?kota=jakarta
#[utoipa::path(
    get,
    path = "/search/jadwal-sholat",
    tag = "search",
    operation_id = "search_jadwal_sholat_handler",
    responses(
        (status = 200, description = "OK", body = serde_json::Value),
        (status = 400, description = "Bad Request"),
        (status = 502, description = "Upstream error"),
    )
)]

pub async fn jadwal_sholat_handler(
    Query(p): Query<SearchParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let kota = require(p.kota.as_ref(), "kota")?;
    Ok((StatusCode::OK, Json(use_cases::jadwal_sholat(&kota).await?)))
}

/// GET /search/weather?city=jakarta
#[utoipa::path(
    get,
    path = "/search/weather",
    tag = "search",
    operation_id = "search_weather_handler",
    responses(
        (status = 200, description = "OK", body = serde_json::Value),
        (status = 400, description = "Bad Request"),
        (status = 502, description = "Upstream error"),
    )
)]

pub async fn weather_handler(
    Query(p): Query<SearchParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let city = require(p.city.as_ref().or(p.q.as_ref()), "city")?;
    Ok((StatusCode::OK, Json(use_cases::weather(&city).await?)))
}

/// GET /search/google?query=rust
#[utoipa::path(
    get,
    path = "/search/google",
    tag = "search",
    operation_id = "search_google_handler",
    responses(
        (status = 200, description = "OK", body = serde_json::Value),
        (status = 400, description = "Bad Request"),
        (status = 502, description = "Upstream error"),
    )
)]

pub async fn google_handler(
    Query(p): Query<SearchParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let query = require(p.query.as_ref().or(p.q.as_ref()), "query")?;
    Ok((StatusCode::OK, Json(use_cases::google(&query).await?)))
}

/// GET /search/yt?query=rickroll
#[utoipa::path(
    get,
    path = "/search/yt",
    tag = "search",
    operation_id = "search_yt_handler",
    responses(
        (status = 200, description = "OK", body = serde_json::Value),
        (status = 400, description = "Bad Request"),
        (status = 502, description = "Upstream error"),
    )
)]

pub async fn yt_handler(
    Query(p): Query<SearchParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let query = require(p.query.as_ref().or(p.q.as_ref()), "query")?;
    Ok((StatusCode::OK, Json(use_cases::yt_search(&query).await?)))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/search` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/bmkg", get(bmkg_handler))
        .route("/jadwal-sholat", get(jadwal_sholat_handler))
        .route("/weather", get(weather_handler))
        .route("/google", get(google_handler))
        .route("/yt", get(yt_handler))
}
