//! HTTP-facing application error.
//!
//! [`AppError`] is a presentation concern: it names HTTP status classes and
//! knows how to render itself, so it lives here rather than in `domain`.
//! Infrastructure reports failures with domain errors (`ScrapingError`,
//! `RepositoryError`) and the [`From`] impls below translate them.

use axum::response::IntoResponse;
use http::StatusCode;
use thiserror::Error;

use crate::domain::error::{DomainError, RepositoryError, ScrapingError};
use crate::presentation::dto::common::ApiResponse;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Scraping error: {0}")]
    ScraperError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Http error: {0}")]
    HttpError(#[from] http::Error),
    #[error("Url parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),
    #[error("Json error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Internal(s.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<deadpool_redis::PoolError> for AppError {
    fn from(err: deadpool_redis::PoolError) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound(msg) => AppError::NotFound(msg),
            DomainError::Validation(msg) => AppError::BadRequest(msg),
            DomainError::Repository(repo_err) => match repo_err {
                RepositoryError::NotFound => AppError::NotFound("Resource not found".into()),
                RepositoryError::Conflict(msg) => {
                    AppError::BadRequest(format!("Conflict: {}", msg))
                }
                RepositoryError::Database(msg) => AppError::DatabaseError(msg),
                RepositoryError::Network(msg) => AppError::ScraperError(msg),
            },
            DomainError::Scraping(scrape_err) => match scrape_err {
                ScrapingError::Http(msg) => AppError::ScraperError(msg),
                ScrapingError::Parse(msg) => AppError::BadRequest(format!("Parse error: {}", msg)),
                ScrapingError::EmptyResponse => {
                    AppError::NotFound("Empty response from source".into())
                }
            },
        }
    }
}

impl From<ScrapingError> for AppError {
    fn from(e: ScrapingError) -> Self {
        // Scraping failures are upstream provider failures, not local bugs, so
        // they surface as 502 rather than 500.
        AppError::ScraperError(e.to_string())
    }
}

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
