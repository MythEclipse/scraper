//! HTTP rendering for [`AppError`].
//!
//! The error type itself lives in `application` so that infrastructure can use it
//! without depending on the presentation layer; this module only supplies the
//! translation from a variant to a status code.

use axum::response::IntoResponse;
use http::StatusCode;

use crate::domain::error::AppError;
use crate::presentation::dto::common::ApiResponse;

/// Maps an error variant to its status code and client-safe message.
///
/// Internal failures log the real cause but return a generic message, so an
/// infrastructure detail never leaks into an HTTP body.
fn status_and_message(error: &AppError) -> (StatusCode, String) {
    match error {
        AppError::NotFound(_) => (StatusCode::NOT_FOUND, error.to_string()),
        AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, error.to_string()),
        AppError::ScraperError(_) => (StatusCode::BAD_GATEWAY, error.to_string()),
        AppError::DatabaseError(_) => {
            tracing::error!(%error, "Database error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".into(),
            )
        }
        AppError::Internal(_) => {
            tracing::error!(%error, "Internal error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".into(),
            )
        }
        _ => {
            tracing::error!(%error, "Unhandled error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".into(),
            )
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = status_and_message(&self);
        let body = axum::Json(ApiResponse::<()>::error(error_message));
        (status, body).into_response()
    }
}
