//! Axum handlers for stalk (profile/user lookups).
//!
//! Ported from Shirokami-API `scraper/stalk/*.js`.
use crate::presentation::validation::validate;
use axum::Router;
use axum::routing::get;
use tracing::instrument;
use zod_rs::prelude::{Schema as _, ZodSchema};

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::IntoParams;

use crate::presentation::error::AppError;
use crate::presentation::stalk_wiring as use_cases;

/// Query params for stalk endpoints.
#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct StalkParams {
    #[zod(min_length(1), max_length(128))]
    pub username: Option<String>,
    pub user_id: Option<String>,
    pub userid: Option<String>,
    pub zone_id: Option<String>,
    pub zoneid: Option<String>,
}

/// GET /stalk/github?username=asepharyana
#[utoipa::path(
    get,
    path = "/stalk/github",
    tag = "stalk",
    operation_id = "stalk_github",
    params(StalkParams),
    responses(
        (status = 200, description = "GitHub user profile", body = Value),
        (status = 400, description = "Missing username"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn github_handler(
    Query(params): Query<StalkParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let username = params
        .username
        .ok_or_else(|| AppError::BadRequest("Missing 'username' parameter".to_string()))?;
    let result = use_cases::github(&username).await?;
    Ok((StatusCode::OK, Json(result)))
}

/// GET /stalk/youtube?username=<handle>
#[utoipa::path(
    get,
    path = "/stalk/youtube",
    tag = "stalk",
    operation_id = "stalk_youtube",
    params(StalkParams),
    responses(
        (status = 200, description = "YouTube channel profile", body = Value),
        (status = 400, description = "Missing username"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn youtube_handler(
    Query(params): Query<StalkParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let username = params
        .username
        .ok_or_else(|| AppError::BadRequest("Missing 'username' parameter".to_string()))?;
    let result = use_cases::youtube(&username).await?;
    Ok((StatusCode::OK, Json(result)))
}

/// GET /stalk/twitter?username=...
#[utoipa::path(
    get,
    path = "/stalk/twitter",
    tag = "stalk",
    operation_id = "stalk_twitter",
    params(StalkParams),
    responses(
        (status = 200, description = "Twitter/X user profile", body = Value),
        (status = 400, description = "Missing username"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn twitter_handler(
    Query(params): Query<StalkParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let username = params
        .username
        .ok_or_else(|| AppError::BadRequest("Missing 'username' parameter".to_string()))?;
    let result = use_cases::twitter(&username).await?;
    Ok((StatusCode::OK, Json(result)))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/stalk` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/github", get(github_handler))
        .route("/youtube", get(youtube_handler))
        .route("/twitter", get(twitter_handler))
}
