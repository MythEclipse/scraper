//! HTML scraping helpers — re-exports from retry and parsing_utils modules.
//!
//! This module consolidates common scraping utilities for convenient single-path imports.
//! Downstream code should use `crate::infrastructure::scraping::html_fetcher::*`.

// Re-export retry utilities
pub use crate::infrastructure::scraping::retry::{
    custom_backoff, default_backoff, permanent, quick_backoff, retry, slow_backoff, transient,
};
// Re-export common scraping helpers (fetch_html_with_retry, parse_html, selector, text, attr, etc.)
pub use crate::infrastructure::scraping::parsing_utils::*;
