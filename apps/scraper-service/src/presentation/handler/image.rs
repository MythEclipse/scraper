//! Axum handlers for image generation endpoints.
//!
//! Returns raw image bytes with the proper Content-Type.
use crate::presentation::validation::validate;
use axum::Router;
use axum::routing::get;
use tracing::instrument;
use zod_rs::prelude::{Schema as _, ZodSchema};

use axum::body::Body;
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;

use crate::presentation::error::AppError;
use crate::presentation::image_wiring as use_cases;

#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct BratParams {
    #[zod(min_length(1), max_length(500))]
    pub text: Option<String>,
}

/// GET /image/brat?text=Hello
#[utoipa::path(
    get,
    path = "/image/brat",
    tag = "image",
    operation_id = "image_brat",
    params(BratParams),
    responses(
        (status = 200, description = "Static brat image (PNG)", body = Vec<u8>),
        (status = 400, description = "Missing text"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn brat_handler(Query(params): Query<BratParams>) -> Result<Response, AppError> {
    let params = validate(params)?;
    let text = params
        .text
        .ok_or_else(|| AppError::BadRequest("text parameter is required".into()))?;
    let bytes = use_cases::brat(&text).await?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("build response: {e}")))
}

/// GET /image/brat/animated?text=Hello
#[utoipa::path(
    get,
    path = "/image/brat/animated",
    tag = "image",
    operation_id = "image_brat_animated",
    params(BratParams),
    responses(
        (status = 200, description = "Animated brat image (GIF)", body = Vec<u8>),
        (status = 400, description = "Missing text"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn brat_animated_handler(Query(params): Query<BratParams>) -> Result<Response, AppError> {
    let params = validate(params)?;
    let text = params
        .text
        .ok_or_else(|| AppError::BadRequest("text parameter is required".into()))?;
    let bytes = use_cases::brat_animated(&text).await?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/gif")
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("build response: {e}")))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/image` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/brat", get(brat_handler))
        .route("/brat/animated", get(brat_animated_handler))
}
