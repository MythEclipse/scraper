//! Axum handlers for misc utility endpoints.
//!
//! Ported from Shirokami-API `scraper/misc/*.js`.
use axum::routing::get;
use axum::Router;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use utoipa::IntoParams;

use crate::domain::error::AppError;
use crate::presentation::misc_wiring as use_cases;

/// Query params for currency converter.
#[derive(Deserialize, IntoParams)]
pub struct CurrencyParams {
    pub amount: Option<f64>,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// GET /misc/currency-converter?amount=100&from=USD&to=IDR
#[utoipa::path(
    get,
    path = "/misc/currency-converter",
    tag = "misc",
    operation_id = "currency_converter",
    params(CurrencyParams),
    responses(
        (status = 200, description = "Currency conversion result", body = Value),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn currency_converter_handler(
    Query(params): Query<CurrencyParams>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let amount = params.amount.unwrap_or(1.0);
    let from = params.from.unwrap_or_else(|| "USD".to_string());
    let to = params.to.unwrap_or_else(|| "IDR".to_string());

    let result = use_cases::currency_converter(amount, &from, &to).await?;
    Ok((StatusCode::OK, Json(result)))
}

/// GET /misc/harga-emas
#[utoipa::path(
    get,
    path = "/misc/harga-emas",
    tag = "misc",
    operation_id = "harga_emas",
    responses(
        (status = 200, description = "Gold prices (Antam)", body = Value),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn harga_emas_handler() -> Result<(StatusCode, Json<Value>), AppError> {
    let result = use_cases::harga_emas().await?;
    Ok((StatusCode::OK, Json(result)))
}

/// GET /misc/kurs-bca
#[utoipa::path(
    get,
    path = "/misc/kurs-bca",
    tag = "misc",
    operation_id = "kurs_bca",
    responses(
        (status = 200, description = "BCA exchange rates", body = Value),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn kurs_bca_handler() -> Result<(StatusCode, Json<Value>), AppError> {
    let result = use_cases::kurs_bca().await?;
    Ok((StatusCode::OK, Json(result)))
}

/// GET /misc/server-info
#[utoipa::path(
    get,
    path = "/misc/server-info",
    tag = "misc",
    operation_id = "server_info",
    responses(
        (status = 200, description = "Server info", body = Value),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn server_info_handler() -> Result<(StatusCode, Json<Value>), AppError> {
    let result = use_cases::server_info().await?;
    Ok((StatusCode::OK, Json(result)))
}

/// Routes served by this module, relative to its mount point.
/// Mounted at `/misc` by [`crate::presentation::router`].
pub fn router() -> Router {
    Router::new()
        .route("/currency-converter", get(currency_converter_handler))
        .route("/harga-emas", get(harga_emas_handler))
        .route("/kurs-bca", get(kurs_bca_handler))
        .route("/server-info", get(server_info_handler))
}
