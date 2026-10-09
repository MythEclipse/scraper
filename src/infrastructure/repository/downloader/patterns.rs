//! Compiled-once regexes and CSS selectors for the platform downloaders.
//!
//! Every pattern here is a compile-time literal, so it cannot fail at runtime.
//! Each is held in a [`LazyLock`] so it is built once per process rather than
//! recompiled on every request, and so no call site needs an `unwrap` on a
//! value that is statically known to be valid.

use std::sync::LazyLock;

use regex::Regex;
use scraper::Selector;

/// Build a compiled pattern from a literal, panicking only on a bad literal.
trait FromLiteral: Sized {
    fn from_literal(pattern: &str) -> Self;
}

impl FromLiteral for Regex {
    fn from_literal(pattern: &str) -> Self {
        Regex::new(pattern).expect("invalid regex literal")
    }
}

impl FromLiteral for Selector {
    fn from_literal(pattern: &str) -> Self {
        // Both failures are typos in a literal, caught by the first test run.
        Selector::parse(pattern).expect("invalid CSS selector literal")
    }
}

macro_rules! patterns {
    ($($name:ident: $kind:ident = $pattern:expr;)*) => {
        $(
            pub(super) static $name: LazyLock<$kind> =
                LazyLock::new(|| $kind::from_literal($pattern));
        )*
    };
}

patterns! {
    // ── TikTok ────────────────────────────────────────────────────────────
    TIKTOK_SHORT_PATH: Regex = r"/([A-Za-z0-9_-]+)$";
    TIKTOK_VIDEO_ID: Regex = r"/video/(\d{15,25})";
    TIKTOK_HAS_VIDEO_PATH: Regex = r"/video/\d+";
    TIKTOK_GENERIC_VIDEO_ID: Regex = r"/video/(\d+)";
    TIKTOK_TITLE: Regex = r#"<title[^>]*>([^<]+)</title>"#;
    TIKTOK_AUTHOR: Regex = r#"data-author-name="([^"]+)""#;

    // ── Twitter / X ───────────────────────────────────────────────────────
    TWITTER_STATUS_ID: Regex = r"(?:twitter\.com|x\.com)/[^/]+/status/(\d+)";
    TW_VIDEO: Selector = "div.tw-video";
    TW_TEXT_LINK: Selector = "div.tw-right > div > p:nth-child(1) > a";
    TW_VIDEO_ITEM: Selector = "div.video-data > div > ul > li";
    TW_ITEM_VIDEO_LINK: Selector = "div > div:nth-child(2) > a";
    TW_ORIGIN_ITEM: Selector = "div.origin-top-right > ul > li";
    ANCHOR: Selector = "a";
    TW_ITEM_BODY: Selector = "div > div > div";

    // ── Pinterest / social ────────────────────────────────────────────────
    INSTAGRAM_URL: Regex = r"https?://(www\.)?instagram\.com/[^\s]+";

    // ── File hosts ────────────────────────────────────────────────────────
    MEGA_FILE_ID: Regex = r"file/([^#]+)";
    TERABOX_HOST: Regex = r"^https?://(?:www\.|1024)?terabox(?:app)?\.com";
    PIXELDRAIN_ID: Regex = r"/[de]/([a-zA-Z0-9]+)";
    KRAKENFILES_ID: Regex = r"krakenfiles\.com/v/([A-Za-z0-9_-]+)";
    MEDIAFIRE_CDN: Regex = r"\$\.get\('([^']+)',";
    MEDIAFIRE_VIEWER_DATA: Regex = r"window\.viewer_data\s*=\s*(\{.*?\});";
    MEDIAFIRE_ID_PARAM: Regex = r"[?&]id=([a-zA-Z0-9_-]+)";
    MEDIAFIRE_DOWNLOAD_LINK: Selector = r"a#downloadButton";
    H1: Selector = "h1";

    // ── Misc ──────────────────────────────────────────────────────────────
    DANBOORU_POST_ID: Regex = r"danbooru\.donmai\.us/posts/(\d+)$";

    // ── Shared / detection ────────────────────────────────────────────────
    THUMB_PATH: Regex = r"/thumb(\?|$)";
}
