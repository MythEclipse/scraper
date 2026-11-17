use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

use crate::infrastructure::utils::text::contains_ignore_ascii_case;

pub fn common_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));
    headers.insert("Referer", HeaderValue::from_static("https://google.com"));
    headers
}

pub fn is_internet_baik_block_page(content: &str) -> bool {
    // Marker scan is allocation-free: the previous form lowercased the whole
    // body (`content.to_lowercase()`) — a full copy of every fetched page.
    let hit = |marker: &str| contains_ignore_ascii_case(content, marker);

    // Order matters only for cost, not for the result: each marker is its own
    // in-place scan and the chain short-circuits on the first hit.
    if hit("internet sehat")
        || hit("akses di blokir")
        || hit("this site has been blocked")
        || hit("website ini telah diblokir")
    {
        return true;
    }

    // The IndiHome variant is the only marker that needs a second phrase.
    hit("akses ditolak") && hit("indihome")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_block_pages_case_insensitively() {
        assert!(is_internet_baik_block_page("<h1>Internet Sehat</h1>"));
        assert!(is_internet_baik_block_page("THIS SITE HAS BEEN BLOCKED"));
        assert!(is_internet_baik_block_page("akses di blokir oleh provider"));
        assert!(is_internet_baik_block_page(
            "Akses Ditolak — IndiHome memblokir situs ini"
        ));
    }

    #[test]
    fn ignores_normal_pages_and_partial_markers() {
        assert!(!is_internet_baik_block_page(
            "<html><body>anime list</body></html>"
        ));
        // "akses ditolak" without indihome is not the block page.
        assert!(!is_internet_baik_block_page(
            "akses ditolak oleh admin forum"
        ));
        // A marker longer than the haystack must not panic.
        assert!(!is_internet_baik_block_page("x"));
        assert!(!is_internet_baik_block_page(""));
    }
}
