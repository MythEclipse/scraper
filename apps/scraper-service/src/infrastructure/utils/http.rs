use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

pub fn common_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"));
    headers.insert("Referer", HeaderValue::from_static("https://google.com"));
    headers
}

pub fn is_internet_baik_block_page(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("internet sehat")
        || (lower.contains("akses ditolak") && lower.contains("indihome"))
        || lower.contains("akses di blokir")
        || lower.contains("this site has been blocked")
        || lower.contains("website ini telah diblokir")
}
