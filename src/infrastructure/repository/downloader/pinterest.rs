//! //! Pinterest downloader via the `pinterest-dl` CLI, with browser scraping as fallback.

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;

use super::shared::{playwright_to_download_result, run_playwright_scraper};

/// Run the `pinterest-dl` CLI to scrape a Pinterest pin, returning the parsed
/// JSON. pinterest-dl handles Pinterest's guest-token/cookie dance and returns
/// real media URLs (HLS video streams or images) for public pins — no login.
fn run_pinterest_dl(url: &str) -> Result<serde_json::Value, ScrapingError> {
    // Locate the pinterest-dl executable: prefer the scraper's venv python's
    // script dir, then PATH.
    let candidates = [
        "/home/code/hermes-agent/.venv/bin/pinterest-dl",
        "/usr/local/bin/pinterest-dl",
        "pinterest-dl",
    ];
    let bin = candidates
        .iter()
        .find(|p| {
            if p.contains('/') {
                std::path::Path::new(p).exists()
            } else {
                std::process::Command::new("sh")
                    .args(["-c", "command -v"])
                    .arg(p)
                    .stdout(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }
        })
        .ok_or_else(|| {
            ScrapingError::Http(
                "pinterest-dl not found; install it in the scraper venv".to_string(),
            )
        })?;

    let output = std::process::Command::new(bin)
        .args(["scrape", url, "--json", "-n", "1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| ScrapingError::Http(format!("pinterest-dl spawn failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(ScrapingError::Http(format!(
            "pinterest-dl failed: {}",
            stderr.trim().lines().last().unwrap_or("unknown error")
        )));
    }
    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str(&stdout_str)
        .map_err(|e| ScrapingError::Http(format!("pinterest-dl JSON parse failed: {}", e)))
}

/// Convert a pinterest-dl JSON result (from `scrape --json`) into DownloadResult.
/// Extracts image `src` URLs and, for video pins, the `media_stream.video.url`
/// (HLS m3u8) plus the poster image.
fn pinterest_dl_to_download_result(data: &serde_json::Value) -> DownloadResult {
    let mut result = DownloadResult::success(None);
    result.provider = Some("pinterest-dl".to_string());
    let mut media_list: Vec<MediaItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
        for res in results {
            if let Some(items) = res.get("items").and_then(|v| v.as_array()) {
                for it in items {
                    // Poster/alt image (i.pinimg.com) — always add.
                    if let Some(src) = it.get("src").and_then(|v| v.as_str()) {
                        if !src.is_empty() && !seen.contains(src) {
                            seen.insert(src.to_string());
                            media_list.push(MediaItem {
                                url: src.to_string(),
                                quality: Some("poster".to_string()),
                                file_type: Some(MediaType::Image),
                                extension: Some("jpg".to_string()),
                                thumbnail: Some(src.to_string()),
                                file_size: None,
                                size_bytes: None,
                                frame_width: None,
                                frame_height: None,
                                note: None,
                            });
                        }
                    }
                    // Video HLS stream (v1.pinimg.com/videos).
                    if let Some(ms) = it.get("media_stream").and_then(|v| v.get("video")) {
                        if let Some(url) = ms.get("url").and_then(|v| v.as_str()) {
                            if !url.is_empty() && !seen.contains(url) {
                                seen.insert(url.to_string());
                                let (w, h) = match (
                                    ms.get("resolution").and_then(|v| v.as_array()),
                                    ms.get("resolution"),
                                ) {
                                    (Some(arr), _) if arr.len() >= 2 => (
                                        arr[0].as_u64().map(|v| v.to_string()),
                                        arr[1].as_u64().map(|v| v.to_string()),
                                    ),
                                    _ => (None, None),
                                };
                                let dur = ms.get("duration").and_then(|v| v.as_u64());
                                media_list.push(MediaItem {
                                    url: url.to_string(),
                                    quality: Some(format!(
                                        "{}x{}",
                                        w.clone().unwrap_or_else(|| "?".into()),
                                        h.clone().unwrap_or_else(|| "?".into())
                                    )),
                                    file_type: Some(MediaType::Video),
                                    extension: Some("m3u8".to_string()),
                                    thumbnail: it
                                        .get("src")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                    file_size: None,
                                    size_bytes: None,
                                    frame_width: w,
                                    frame_height: h,
                                    note: dur.map(|d| format!("{}s", d)),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    result.media = media_list;
    result
}

pub async fn fetch_pinterest(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Use the `pinterest-dl` CLI (guest-token handling + parsing) which works
    // from the VPS and returns real image/video URLs for public pins.
    // Run it in spawn_blocking since it does blocking subprocess + network I/O.
    let media_result: Option<DownloadResult> = tokio::task::spawn_blocking({
        let url_owned = url.to_string();
        move || {
            run_pinterest_dl(&url_owned)
                .map(|data| pinterest_dl_to_download_result(&data))
                .ok()
        }
    })
    .await
    .unwrap_or(None);

    let mut result = match media_result {
        Some(mut r) => {
            r.media.sort_by(|a, b| {
                let wa = a
                    .frame_width
                    .clone()
                    .and_then(|w| w.parse::<u64>().ok())
                    .unwrap_or(0);
                let wb = b
                    .frame_width
                    .clone()
                    .and_then(|w| w.parse::<u64>().ok())
                    .unwrap_or(0);
                wb.cmp(&wa)
            });
            r
        }
        None => DownloadResult::success(None),
    };

    // If pinterest-dl returned nothing, fall back to Playwright scraping.
    if result.media.is_empty() {
        if let Ok(data) = run_playwright_scraper(url, "pinterest").await {
            let pw_result = playwright_to_download_result(&data);
            if !pw_result.media.is_empty() {
                return Ok(pw_result);
            }
        }
    }
    if result.media.is_empty() {
        result.message = Some("No download URLs found".to_string());
    }
    Ok(result)
}
