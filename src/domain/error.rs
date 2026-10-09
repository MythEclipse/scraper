//! Domain-level error types.
//!
//! These are framework-agnostic errors that can be mapped to HTTP errors
//! at the presentation layer. Domain and application layers only use these.

use thiserror::Error;

/// Errors originating from repository operations (DB, HTTP, etc.)
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Not found")]
    NotFound,
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
}

/// Errors originating from scraping/parsing operations
#[derive(Error, Debug)]
pub enum ScrapingError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Empty response")]
    EmptyResponse,
}

/// Generic domain error
#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("Scraping error: {0}")]
    Scraping(#[from] ScrapingError),
}

impl From<String> for RepositoryError {
    fn from(s: String) -> Self {
        RepositoryError::Database(s)
    }
}

impl From<&str> for RepositoryError {
    fn from(s: &str) -> Self {
        RepositoryError::Database(s.to_string())
    }
}

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
        // Map ScrapingError::Http to BAD_GATEWAY (502) — the scraper code
        // correctly returns DownloadResult::error for client-side issues
        // (invalid URL, bad format), so any ScraperError that escapes is
        // an upstream provider failure, not a local bug.
        AppError::ScraperError(e.to_string())
    }
}
