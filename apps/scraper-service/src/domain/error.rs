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
