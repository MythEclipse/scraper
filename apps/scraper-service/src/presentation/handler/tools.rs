//! Axum handlers for tool endpoints.
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
use crate::presentation::tools_wiring as use_cases;

#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct DomainParams {
    #[zod(min_length(1), max_length(253))]
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct IpParams {
    #[zod(min_length(1), max_length(45))]
    pub ip: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct UrlParams {
    #[zod(min_length(1), max_length(2048))]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ZodSchema, Serialize)]
pub struct ResiParams {
    #[zod(min_length(1), max_length(64))]
    pub resi: Option<String>,
    #[zod(min_length(1), max_length(32))]
    pub ekspedisi: Option<String>,
}

/// GET /tool/whois?domain=example.com
#[utoipa::path(
    get,
    path = "/tool/whois",
    tag = "tool",
    operation_id = "tool_whois",
    params(DomainParams),
    responses(
        (status = 200, description = "WHOIS info", body = Value),
        (status = 400, description = "Missing domain"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn whois_handler(
    Query(params): Query<DomainParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let domain = params
        .domain
        .ok_or_else(|| AppError::BadRequest("domain parameter is required".into()))?;
    Ok((StatusCode::OK, Json(use_cases::whois(&domain).await?)))
}

/// GET /tool/ip-location?ip=1.1.1.1
#[utoipa::path(
    get,
    path = "/tool/ip-location",
    tag = "tool",
    operation_id = "tool_ip_location",
    params(IpParams),
    responses(
        (status = 200, description = "IP geolocation", body = Value),
        (status = 400, description = "Missing IP"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn ip_location_handler(
    Query(params): Query<IpParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let ip = params
        .ip
        .ok_or_else(|| AppError::BadRequest("ip parameter is required".into()))?;
    Ok((StatusCode::OK, Json(use_cases::ip_location(&ip).await?)))
}

/// GET /tool/tinyurl?url=https://example.com
#[utoipa::path(
    get,
    path = "/tool/tinyurl",
    tag = "tool",
    operation_id = "tool_tinyurl",
    params(UrlParams),
    responses(
        (status = 200, description = "Shortened URL", body = Value),
        (status = 400, description = "Missing URL"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn tinyurl_handler(
    Query(params): Query<UrlParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let url = params
        .url
        .ok_or_else(|| AppError::BadRequest("url parameter is required".into()))?;
    Ok((StatusCode::OK, Json(use_cases::tinyurl(&url).await?)))
}

/// GET /tool/check-hosting?domain=example.com
#[utoipa::path(
    get,
    path = "/tool/check-hosting",
    tag = "tool",
    operation_id = "tool_check_hosting",
    params(DomainParams),
    responses(
        (status = 200, description = "Hosting info", body = Value),
        (status = 400, description = "Missing domain"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn check_hosting_handler(
    Query(params): Query<DomainParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let domain = params
        .domain
        .ok_or_else(|| AppError::BadRequest("domain parameter is required".into()))?;
    Ok((
        StatusCode::OK,
        Json(use_cases::check_hosting(&domain).await?),
    ))
}

/// GET /tool/hargapangan
#[utoipa::path(
    get,
    path = "/tool/hargapangan",
    tag = "tool",
    operation_id = "tool_hargapangan",
    responses(
        (status = 200, description = "Food prices", body = Value),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all)]
pub async fn hargapangan_handler() -> Result<(StatusCode, Json<Value>), AppError> {
    Ok((StatusCode::OK, Json(use_cases::hargapangan().await?)))
}

/// GET /tool/cek-resi?resi=...&ekspedisi=...
#[utoipa::path(
    get,
    path = "/tool/cek-resi",
    tag = "tool",
    operation_id = "tool_cek_resi",
    params(ResiParams),
    responses(
        (status = 200, description = "Shipment tracking", body = Value),
        (status = 400, description = "Missing resi"),
        (status = 502, description = "Upstream error"),
    )
)]
#[instrument(skip_all, fields(params = ?params))]
pub async fn cek_resi_handler(
    Query(params): Query<ResiParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let params = validate(params)?;
    let ResiParams { resi, ekspedisi } = params;
    let resi = resi.ok_or_else(|| AppError::BadRequest("resi parameter is required".into()))?;
    Ok((
        StatusCode::OK,
        Json(use_cases::cek_resi(resi, ekspedisi).await?),
    ))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/tool` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/whois", get(whois_handler))
        .route("/iplocation", get(ip_location_handler))
        .route("/tinyurl", get(tinyurl_handler))
        .route("/check-hosting", get(check_hosting_handler))
        .route("/hargapangan", get(hargapangan_handler))
        .route("/cek-resi", get(cek_resi_handler))
}
