//! Shared helpers used by more than one platform downloader.
//!
//! Kept in one place so the platform modules stay about their own provider.

use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::USER_AGENT;
use url::Url;

use super::patterns::THUMB_PATH;
use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

pub(super) fn format_filesize(bytes: u64) -> String {
    const UNITS: &[&str] = &["bytes", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, UNITS[unit])
}

/// Default headers for outbound HTTP requests to external APIs.
#[allow(dead_code)]
pub(super) fn api_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Linux; Android 15; SM-F958 Build/AP3A.240905.015) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.6723.86 Mobile Safari/537.36",
        ),
    );
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h
}

/// Detect whether a URL points to an image or video, based on content-type,
/// magic bytes, or filename hints. Ports the logic from Shirokami's
/// `instagram.js` `detectType` function.
#[allow(dead_code)]
pub(super) async fn detect_media_type(url: &str) -> MediaType {
    // thumb paths are images
    if THUMB_PATH.is_match(url) {
        return MediaType::Image;
    }

    // Try HEAD request for content-type
    let client = http_client();
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";
    if let Ok(resp) = client
        .client()
        .head(url)
        .header(USER_AGENT, ua)
        .timeout(Duration::from_secs(8))
        .send()
        .await
    {
        if let Some(ct) = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
        {
            let ct = ct.to_lowercase();
            if ct.starts_with("video/") {
                return MediaType::Video;
            }
            if ct.starts_with("image/") {
                return MediaType::Image;
            }
        }
    }

    // Magic bytes range GET (first 1KB)
    if let Ok(resp) = client
        .client()
        .get(url)
        .header("range", "bytes=0-1023")
        .header(USER_AGENT, ua)
        .header("accept", "*/*")
        .send()
        .await
    {
        if let Ok(bytes) = resp.bytes().await {
            if bytes.len() >= 12 {
                // JPEG: FF D8 FF
                if bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
                    return MediaType::Image;
                }
                // PNG
                if bytes[0..8] == [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] {
                    return MediaType::Image;
                }
                // WEBP: RIFF....WEBP
                if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
                    return MediaType::Image;
                }
                // MP4: bytes 4..7 = 'ftyp'
                if &bytes[4..8] == b"ftyp" {
                    return MediaType::Video;
                }
            }
        }
    }

    // filename hints
    if let Ok(parsed) = Url::parse(url) {
        let query = parsed.query().unwrap_or("");
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            if (k == "filename" || k == "file") && v.ends_with(".mp4") {
                return MediaType::Video;
            }
            if (k == "filename" || k == "file") && v.ends_with(".jpg") {
                return MediaType::Image;
            }
        }
    }

    MediaType::Image
}

/// Locate the yt-dlp binary on the system.
/// Checks: PATH → /home/code/hermes-agent/.venv/bin/yt-dlp → common locations
pub(super) fn find_ytdlp() -> Option<String> {
    // Check known locations first
    let candidates = [
        "/home/code/hermes-agent/.venv/bin/yt-dlp",
        "/usr/local/bin/yt-dlp",
        "/usr/bin/yt-dlp",
        "/snap/bin/yt-dlp",
        "/home/code/.local/bin/yt-dlp",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    // Check PATH
    let paths: Vec<_> = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .collect();
    for path in &paths {
        let candidate = path.join("yt-dlp");
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Run yt-dlp --dump-json and return the parsed JSON value.
/// Uses spawn + manual stdout reading to avoid pipe buffer truncation
/// on large outputs (>64KB on Linux default pipe buffer).
pub(super) async fn run_ytdlp_json(
    url: &str,
    extra_args: &[&str],
) -> Result<serde_json::Value, ScrapingError> {
    let ytdlp =
        find_ytdlp().ok_or_else(|| ScrapingError::Http("yt-dlp binary not found".to_string()))?;

    let extra_args_owned: Vec<String> = extra_args.iter().map(|s| s.to_string()).collect();

    let (stdout_str, stderr_str, exit_code) = tokio::task::spawn_blocking({
        let url_owned = url.to_string();
        let ytdlp_owned = ytdlp.clone();
        move || {
            let mut cmd_args: Vec<String> = vec![
                "--dump-json".to_string(),
                "--no-warnings".to_string(),
                "--no-check-certificates".to_string(),
                "--user-agent".to_string(),
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".to_string(),
            ];
            cmd_args.extend(extra_args_owned.iter().cloned());
            cmd_args.push(url_owned);

            // Use Stdio::piped() + read_to_string to handle large stdout (64KB+).
            // Command::output() truncates at pipe buffer size.
            let mut child = std::process::Command::new(&ytdlp_owned)
                .args(&cmd_args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| ScrapingError::Http(format!("yt-dlp spawn failed: {}", e)))?;

            // `Stdio::piped()` above guarantees both handles are present; treat a
            // missing one as an error rather than panicking inside a request.
            let mut stdout_str = String::new();
            let mut stderr_str = String::new();
            {
                use std::io::Read;
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout_str);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut stderr_str);
                }
            }
            let status = child.wait().map_err(|e| ScrapingError::Http(format!("yt-dlp wait failed: {}", e)))?;

            Ok((stdout_str, stderr_str, status.code()))
        }
    })
    .await
    .map_err(|e| ScrapingError::Http(format!("yt-dlp execution failed: {}", e)))??;

    if exit_code != Some(0) {
        return Err(ScrapingError::Http(format!(
            "yt-dlp failed: {}",
            stderr_str.trim().lines().last().unwrap_or("unknown error")
        )));
    }

    // yt-dlp --dump-json outputs one JSON per line per format
    let json_line = stdout_str
        .lines()
        .next()
        .ok_or_else(|| ScrapingError::Http("yt-dlp produced no output".to_string()))?;

    serde_json::from_str(json_line)
        .map_err(|e| ScrapingError::Http(format!("yt-dlp JSON parse failed: {}", e)))
}

/// Run Playwright-based browser scraper as fallback when yt-dlp is blocked.
/// Uses headless Chromium to scrape video URLs from anti-bot-protected sites.
pub(super) async fn run_playwright_scraper(
    url: &str,
    platform: &str,
) -> Result<serde_json::Value, ScrapingError> {
    // Locate scrape_media.py robustly: alongside the running binary (installed
    // by scripts/deploy-direct.sh), the Cargo manifest dir (dev), or a few
    // well-known absolute paths.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let manifest_script = format!("{}/scrape_media.py", env!("CARGO_MANIFEST_DIR"));
    let mut candid = vec![
        exe_dir.map(|d| d.join("scrape_media.py").to_string_lossy().to_string()),
        Some(manifest_script),
        Some("/home/code/scraper/scrape_media.py".to_string()),
    ];
    if let Some(rel) = std::env::var_os("SCRAPER_SCRIPT_DIR") {
        candid.push(Some(format!("{}/scrape_media.py", rel.to_string_lossy())));
    }
    let scraper_script = candid
        .into_iter()
        .flatten()
        .find(|p| std::path::Path::new(p).exists())
        .ok_or_else(|| ScrapingError::Http("scrape_media.py not found".to_string()))?;

    // Find a Python interpreter that has playwright installed.
    // The system `python3` may resolve to a different interpreter for the
    // service user, so probe known venv interpreters first.
    let python_candidates = [
        "/home/code/hermes-agent/.venv/bin/python3",
        "/usr/bin/python3",
        "python3",
    ];
    let python_bin = python_candidates
        .iter()
        .find(|p| {
            std::process::Command::new(p)
                .args(["-c", "import playwright"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| "python3".to_string());

    let (stdout_str, stderr_str, exit_code) = tokio::task::spawn_blocking({
        let url_owned = url.to_string();
        let platform_owned = platform.to_string();
        let script_owned = scraper_script.clone();
        let python_owned = python_bin.clone();
        move || -> Result<(String, String, i32), ScrapingError> {
            let output = std::process::Command::new(&python_owned)
                .arg(&script_owned)
                .arg(&url_owned)
                .arg(&platform_owned)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| ScrapingError::Http(format!("playwright spawn failed: {}", e)))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok((stdout, stderr, output.status.code().unwrap_or(-1)))
        }
    })
    .await
    .map_err(|e| ScrapingError::Http(format!("playwright task error: {}", e)))??;

    if exit_code != 0 {
        return Err(ScrapingError::Http(format!(
            "playwright scraper failed: {}",
            stderr_str.trim().lines().last().unwrap_or("unknown error")
        )));
    }

    serde_json::from_str(&stdout_str)
        .map_err(|e| ScrapingError::Http(format!("playwright JSON parse failed: {}", e)))
}

/// Convert a Playwright scraper JSON result (from scrape_media.py) into DownloadResult.
pub(super) fn playwright_to_download_result(data: &serde_json::Value) -> DownloadResult {
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut result = DownloadResult::success(title);
    result.provider = data
        .get("provider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(medias) = data.get("media").and_then(|v| v.as_array()) {
        for m in medias {
            let item = MediaItem {
                url: m
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                quality: None,
                file_type: m.get("ext").and_then(|v| v.as_str()).map(|e| match e {
                    "mp4" | "m3u8" => MediaType::Video,
                    "mp3" | "m4a" => MediaType::Audio,
                    _ => MediaType::Video,
                }),
                extension: m.get("ext").and_then(|v| v.as_str()).map(|s| s.to_string()),
                thumbnail: None,
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: m
                    .get("content_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            };
            result.media.push(item);
        }
    }

    if result.media.is_empty() {
        result.message = Some("No download URLs found".to_string());
    }
    result
}

/// Convert a yt-dlp --dump-single-json result into a DownloadResult with the
/// direct progressive/adaptive media URLs (only entries that carry a URL).
/// Used by the Facebook downloader as the primary provider.
pub(super) fn ytdlp_to_download_result(data: &serde_json::Value) -> Option<DownloadResult> {
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut result = DownloadResult::success(title);
    result.provider = Some("yt-dlp".to_string());
    result.thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    result.duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let formats = data.get("formats").and_then(|v| v.as_array());
    let requested_formats = data.get("requested_formats").and_then(|v| v.as_array());
    let mut seen = std::collections::HashSet::new();

    // `-f bestvideo+bestaudio/best` puts the merged URLs in requested_formats.
    // `best` single format lands in top-level url + formats[0].
    for list in [requested_formats, formats].into_iter().flatten() {
        for f in list {
            let Some(furl) = f.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            if !furl.starts_with("http") || !seen.insert(furl.to_string()) {
                continue;
            }
            let proto = f.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
            if proto.contains("m3u8")
                || f.get("vcodec").and_then(|v| v.as_str()) == Some("none")
                    && f.get("acodec").and_then(|v| v.as_str()) != Some("none")
            {
                // Skip HLS + audio-only; Facebook's progressive mp4s are direct.
                continue;
            }
            let ext: String = f
                .get("ext")
                .and_then(|v| v.as_str())
                .unwrap_or("mp4")
                .to_string();
            result.media.push(MediaItem {
                url: furl.to_string(),
                quality: f
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .map(|h| format!("{}p", h))
                    .or_else(|| {
                        f.get("format_note")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    }),
                file_type: Some(if ext == "mp4" || ext == "webm" || ext == "mkv" {
                    MediaType::Video
                } else {
                    MediaType::File
                }),
                extension: Some(ext),
                thumbnail: result.thumbnail.clone(),
                file_size: f
                    .get("filesize")
                    .and_then(|v| v.as_u64())
                    .map(format_filesize),
                size_bytes: f.get("filesize").and_then(|v| v.as_u64()),
                frame_width: f
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .map(|w| w.to_string()),
                frame_height: f
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .map(|h| h.to_string()),
                note: None,
            });
        }
    }

    // Fall back to the merged top-level URL (bestvideo+bestaudio sets it).
    if result.media.is_empty() {
        if let Some(dl_url) = data.get("url").and_then(|v| v.as_str()) {
            if dl_url.starts_with("http") {
                result.media.push(MediaItem {
                    url: dl_url.to_string(),
                    quality: None,
                    file_type: Some(MediaType::Video),
                    extension: data
                        .get("ext")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| Some("mp4".to_string())),
                    thumbnail: None,
                    file_size: data
                        .get("filesize")
                        .and_then(|v| v.as_u64())
                        .map(format_filesize),
                    size_bytes: data.get("filesize").and_then(|v| v.as_u64()),
                    frame_width: None,
                    frame_height: None,
                    note: None,
                });
            }
        }
    }

    if result.media.is_empty() {
        None
    } else {
        Some(result)
    }
}
