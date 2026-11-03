//! URL constants and dynamic environment-based URLs.
//!
//! Note: These URLs are kept as dynamic env lookups because they may vary
//! between deployments and are not critical startup dependencies.

use crate::config::CONFIG;
use std::env;

pub const BASE_URL: &str = "http://127.0.0.1:4090";

/// Canonical Otakudesu base URL, stored without a trailing slash because every
/// call site appends its own path (`{base}/anime/{slug}`).
pub const OTAKUDESU_BASE_URL: &str = "https://otakudesu.blog";

/// Get Komik URL from environment config.
pub fn get_komik_url() -> String {
    env::var("KOMIK2_BASE_URL").unwrap_or_else(|_| "https://komiku.org".to_string())
}

/// Get production URL from environment config.
pub fn get_production_url() -> String {
    env::var("NEXT_PUBLIC_PROD").unwrap_or_else(|_| CONFIG.urls.site_url.clone())
}

/// Get Komik API URL from environment config.
///
/// Defaults to `https://api.komiku.org`. This is the API mirror whose
/// sub-paths (`/manga/?tipe=manga`, `/genre/<slug>/`, `/search/<q>/`) serve
/// the `.bge` grid layout the parsers expect. NOTE: `api.komiku.org`'s ROOT
/// path returns an empty body — the komik genre-list endpoint must therefore
/// fetch from `komiku.org` (base URL) instead of this root (see
/// `KomikUseCase::genre_list`, which uses `base_url()`).
pub fn get_komik_api_url() -> String {
    env::var("KOMIK2_API_URL").unwrap_or_else(|_| "https://api.komiku.org".to_string())
}

/// Get Otakudesu URL from environment config.
///
/// Normalized without a trailing slash so `format!("{base}/path")` never emits
/// a double slash (which the site 301s away from, costing an extra round trip
/// on every scrape).
pub fn get_otakudesu_url() -> String {
    let base = env::var("OTAKUDESU_BASE_URL").unwrap_or_else(|_| OTAKUDESU_BASE_URL.to_string());
    normalize_base_url(&base)
}

/// Drops trailing slashes so callers can always append `/{path}`.
fn normalize_base_url(base: &str) -> String {
    base.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otakudesu_default_is_the_blog_domain() {
        assert_eq!(OTAKUDESU_BASE_URL, "https://otakudesu.blog");
    }

    #[test]
    fn base_url_normalization_strips_trailing_slashes() {
        assert_eq!(
            normalize_base_url("https://otakudesu.blog/"),
            OTAKUDESU_BASE_URL
        );
        assert_eq!(
            normalize_base_url("https://otakudesu.blog//"),
            OTAKUDESU_BASE_URL
        );
        assert_eq!(normalize_base_url(OTAKUDESU_BASE_URL), OTAKUDESU_BASE_URL);
    }
}
