//! Media downloader repository — ports HTTP API results to domain types.
//!
//! Each method corresponds to a downloader from Shirokami-API.
//! All use the shared `http_client()` and proxy-fetch infrastructure.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use aes::cipher::{BlockDecrypt, KeyInit};
use base64::Engine;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::entity::platform::Platform;
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use url::Url;

/// Default headers for outbound HTTP requests to external APIs.
#[allow(dead_code)]
fn api_headers() -> HeaderMap {
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

/// Helper: extract a YouTube video ID from a URL.
fn extract_youtube_id(url: &str) -> Option<String> {
    let patterns = [
        r"youtube\.com/watch\?v=([a-zA-Z0-9_-]{11})",
        r"youtu\.be/([a-zA-Z0-9_-]{11})",
        r"youtube\.com/shorts/([a-zA-Z0-9_-]{11})",
        r"youtube\.com/embed/([a-zA-Z0-9_-]{11})",
        r"youtube\.com/v/([a-zA-Z0-9_-]{11})",
    ];
    for pat in &patterns {
        if let Some(caps) = regex::Regex::new(pat).ok().and_then(|r| r.captures(url)) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }
    None
}

/// Helper: parse the video-id portion from TikTok/Douyin URL.
fn extract_tiktok_id(url: &str) -> Option<String> {
    if !url.contains("tiktok.com") && !url.contains("douyin.com") {
        return None;
    }
    // Handle short URLs like vm.tiktok.com/ZM8s5qJ6t — resolve redirect first
    if url.contains("vm.tiktok.com") || url.contains("vt.tiktok.com") {
        let re = regex::Regex::new(r"/([A-Za-z0-9_-]+)$").ok()?;
        let short_code = re
            .captures(url)
            .and_then(|c| c.get(1))?
            .as_str()
            .to_string();
        return Some(short_code);
    }
    // TikTok URLs contain an 18-20 digit video ID in the path
    let re = regex::Regex::new(r"/video/(\d{15,25})").ok()?;
    let caps = re.captures(url)?;
    Some(caps.get(1)?.as_str().to_string())
}

/// Locate the yt-dlp binary on the system.
/// Checks: PATH → /home/code/hermes-agent/.venv/bin/yt-dlp → common locations
fn find_ytdlp() -> Option<String> {
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
async fn run_ytdlp_json(
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

            let stdout_file = child.stdout.take().unwrap();
            let stderr_file = child.stderr.take().unwrap();
            let mut stdout_str = String::new();
            let mut stderr_str = String::new();
            use std::io::Read;
            let mut stdout_handle = stdout_file;
            stdout_handle.read_to_string(&mut stdout_str).unwrap_or_default();
            let mut stderr_handle = stderr_file;
            stderr_handle.read_to_string(&mut stderr_str).unwrap_or_default();
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
async fn run_playwright_scraper(
    url: &str,
    platform: &str,
) -> Result<serde_json::Value, ScrapingError> {
    // Locate scrape_media.py robustly: alongside the running binary (Nix store),
    // the Cargo manifest dir (dev), or a few well-known absolute paths.
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

/// Convert a Playwright scraper JSON result (from scrape_media.py) into DownloadResult.
fn playwright_to_download_result(data: &serde_json::Value) -> DownloadResult {
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
                file_type: m.get("ext").and_then(|v| v.as_str()).and_then(|e| match e {
                    "mp4" | "m3u8" => Some(MediaType::Video),
                    "mp3" | "m4a" => Some(MediaType::Audio),
                    _ => Some(MediaType::Video),
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

pub struct DownloaderRepository;

impl DownloaderRepository {
    pub fn new() -> Self {
        Self
    }

    /// Shared client getter — uses the global 30s-timeout client.
    #[allow(dead_code)]
    fn client(&self) -> &'static crate::infrastructure::utils::http_client::HttpClient {
        http_client()
    }

    /// All-in-one: auto-detect platform from URL and delegate to specialized
    /// downloader. Falls back to `downr.org` universal scraper.
    pub async fn download_all_in_one(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        let platform = Platform::detect(url);
        match platform {
            // Instagram and Facebook share the SnapSave backend: try the
            // Instagram entrypoint first, then the Facebook one.
            Platform::Instagram | Platform::Facebook => {
                match Self::download_snapsave(url, cookies).await {
                    ok @ Ok(_) => ok,
                    Err(_) => Self::download_snapsave(url, cookies).await,
                }
            }
            Platform::TikTok => Self::download_tiktok(url, cookies).await,
            Platform::Videy => Self::download_videy(url).await,
            // YouTube: try the video downloader, fall back to MP3 extraction
            // so a blocked yt-dlp path still yields audio.
            Platform::YouTube => match Self::download_youtube(url, "720").await {
                ok @ Ok(_) => ok,
                Err(_) => Self::download_youtube_mp3(url).await,
            },
            Platform::Spotify => Self::download_spotify(url, cookies).await,
            Platform::Twitter => Self::download_twitter(url, cookies).await,
            Platform::Pinterest => Self::download_pinterest(url).await,
            Platform::Reddit => Self::download_reddit(url).await,
            Platform::Mega => Self::download_mega(url).await,
            Platform::TeraBox => Self::download_terabox(url).await,
            Platform::GoogleDrive => Self::download_gdrive(url).await,
            Platform::MediaFire => Self::download_mediafire(url).await,
            Platform::PixelDrain => Self::download_pixeldrain(url).await,
            Platform::Threads => Self::download_threads(url, cookies).await,
            Platform::DoodStream => Self::download_doodstream(url).await,
            Platform::KrakenFiles => Self::download_krakenfiles(url).await,
            Platform::Danbooru => Self::download_danbooru(url).await,
            Platform::SoundCloud => Self::download_soundcloud(url).await,
            Platform::Dailymotion => Self::download_dailymotion(url).await,
            Platform::Streamable => Self::download_streamable(url).await,
            Platform::Bilibili => Self::download_bilibili(url).await,
            Platform::Unknown => fetch_all_in_one(url).await,
        }
    }

    /// Shared SnapSave backend for Instagram / Facebook / Threads.
    ///
    /// Both platforms are served by the same snapsave.app extraction, so the
    /// two public entrypoints are thin aliases over this one implementation.
    async fn download_snapsave(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_snapsave(url).await
    }

    /// Instagram / Facebook via SnapSave.
    /// Cookies param accepted for API consistency but not required (SnapSave
    /// fetches its own).
    pub async fn download_instagram(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        Self::download_snapsave(url, cookies).await
    }

    /// Facebook via SnapSave.
    pub async fn download_facebook(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        Self::download_snapsave(url, cookies).await
    }

    /// TikTok via tikwm.com.
    pub async fn download_tiktok(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_tiktok(url).await
    }

    /// YouTube video via savetube.media.
    pub async fn download_youtube(
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_youtube_mp4(url, quality).await
    }

    /// YouTube video + audio merged server-side into a single MP4.
    pub async fn fetch_youtube_merge(
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_youtube_merge_free(url, quality).await
    }

    /// YouTube to MP3 via ydlp.yard.id.
    pub async fn download_youtube_mp3(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_youtube_mp3(url).await
    }

    /// Spotify via Spotify API (requires api_key).
    pub async fn download_spotify(
        url: &str,
        api_key: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        let _key = match api_key {
            Some(k) => k.to_string(),
            None => std::env::var("SPOTIFY_API_KEY").unwrap_or_default(),
        };
        fetch_spotify(url).await
    }

    /// Twitter/X media via api.lrm.tube.
    pub async fn download_twitter(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_twitter(url).await
    }

    /// Pinterest media via PinterestDownloader.
    pub async fn download_pinterest(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_pinterest(url).await
    }

    /// MEGA.nz file link resolution.
    pub async fn download_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_mega(url).await
    }

    /// TeraBox / TeraFile direct link extraction.
    pub async fn download_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_terabox(url).await
    }

    /// Google Drive direct download link.
    pub async fn download_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_gdrive(url).await
    }

    /// MediaFire direct download link.
    pub async fn download_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_mediafire(url).await
    }

    /// PixelDrain file direct link.
    pub async fn download_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_pixeldrain(url).await
    }

    /// Meta Threads media extraction.
    pub async fn download_threads(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_threads(url, _cookies).await
    }

    /// DoodStream direct link extraction.
    pub async fn download_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_doodstream(url).await
    }

    /// KrakenFiles direct download link.
    pub async fn download_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_krakenfiles(url).await
    }

    /// Danbooru image post extraction.
    pub async fn download_danbooru(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_danbooru(url).await
    }

    /// SoundCloud track via ydlp converter.
    pub async fn download_soundcloud(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_soundcloud(url).await
    }

    /// Dailymotion video via yt-dlp.
    pub async fn download_dailymotion(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_dailymotion(url).await
    }

    /// Reddit video via yt-dlp.
    pub async fn download_reddit(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_reddit(url).await
    }

    /// Streamable video via yt-dlp.
    pub async fn download_streamable(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_streamable(url).await
    }

    /// Videy video (direct CDN link build; ported from Shirokami-API videy.js).
    pub async fn download_videy(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_videy(url).await
    }

    /// Bilibili video via b23.tv short link expansion.
    pub async fn download_bilibili(url: &str) -> Result<DownloadResult, ScrapingError> {
        fetch_bilibili(url).await
    }
}

// ============================================================================
// All-in-One downloader (downr.org)
// ============================================================================

pub async fn fetch_all_in_one(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let headers = {
        let mut h = HeaderMap::new();
        h.insert("user-agent", HeaderValue::from_static(
            "Mozilla/5.0 (Linux; Android 15; SM-F958 Build/AP3A.240905.015) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.6723.86 Mobile Safari/537.36"
        ));
        h.insert("referer", HeaderValue::from_static("https://downr.org/"));
        h
    };

    // Step 1: get analytics to obtain cookies
    let analytics_resp = client
        .client()
        .get("https://downr.org/.netlify/functions/analytics")
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Analytics fetch failed: {}", e)))?;

    let cookies: HashMap<String, String> = analytics_resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| {
            let s = v.to_str().ok()?;
            let parts: Vec<&str> = s.split(';').next()?.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect();

    let cookie_header = cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");

    // Step 2: post download request
    let body = serde_json::json!({ "url": url });
    let mut req_headers = headers.clone();
    if !cookie_header.is_empty() {
        req_headers.insert(
            "cookie",
            HeaderValue::from_str(&cookie_header).unwrap_or_else(|_| HeaderValue::from_static("")),
        );
    }
    req_headers.insert("content-type", HeaderValue::from_static("application/json"));
    req_headers.insert("origin", HeaderValue::from_static("https://downr.org"));

    let resp = client
        .client()
        .post("https://downr.org/.netlify/functions/download")
        .headers(req_headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Download request failed: {}", e)))?;

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ScrapingError::Http(format!("JSON parse failed: {}", e)))?;

    let mut result = DownloadResult::success(
        data.get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    result.author = data.get("author").and_then(|v| {
        v.get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    });
    result.thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    result.provider = Some("downr".to_string());

    if let Some(medias) = data.get("medias").and_then(|v| v.as_array()) {
        for m in medias {
            let item = MediaItem {
                url: m
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                quality: m
                    .get("quality")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_type: m
                    .get("type")
                    .and_then(|v| v.as_str())
                    .and_then(|t| match t {
                        "video" => Some(MediaType::Video),
                        "audio" => Some(MediaType::Audio),
                        "image" => Some(MediaType::Image),
                        _ => Some(MediaType::File),
                    }),
                extension: m
                    .get("extension")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                thumbnail: m
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            };
            result.media.push(item);
        }
    }

    Ok(result)
}

// ============================================================================
// Instagram downloader (snapsave.app)
// ============================================================================

/// Detect whether a URL points to an image or video, based on content-type,
/// magic bytes, or filename hints. Ports the logic from Shirokami's
/// `instagram.js` `detectType` function.
#[allow(dead_code)]
async fn detect_media_type(url: &str) -> MediaType {
    // thumb paths are images
    if regex::Regex::new(r"\/thumb(\?|$)")
        .ok()
        .map(|r| r.is_match(url))
        .unwrap_or(false)
    {
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

/// SnapSave parser — extracts Instagram/Facebook media via snapsave.app.
/// Uses downr.org as fallback for robustness, then Playwright browser scraping.
pub(crate) async fn fetch_snapsave(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Validate Instagram/Facebook URL.
    //
    // Facebook link shapes (all must pass):
    //   https://www.facebook.com/watch/?v=123
    //   https://fb.watch/abc123/
    //   https://m.facebook.com/video.php?v=123
    //   https://www.facebook.com/100012345678901/videos/1234567890
    //   https://www.facebook.com/reel/123
    //   https://www.facebook.com/story.php?story_fbid=123
    //   https://web.facebook.com/...
    // NOTE: the old regex required a `www.|m.|web.` subdomain AND ended the
    // host capture with `(facebook|fb)\.(com|watch)` — that rejected
    // `facebook.com/watch...` (no subdomain) and `fb.watch/...` entirely.
    let fb_host = r"(?:[a-z0-9-]*\.)?(?:facebook|fb)\.(?:com|watch)";
    let valid_fb = regex::Regex::new(&format!(r"https?://{}(?:/|$)", fb_host))
        .unwrap()
        .is_match(url);
    let valid_ig = url.contains("instagram.com")
        || url.contains("threads.net")
        || regex::Regex::new(r"https?://(www\.)?instagram\.com/[^\s]+")
            .unwrap()
            .is_match(url);

    if !valid_fb && !valid_ig {
        return Ok(DownloadResult::error(
            "Link Url not valid — only Instagram and Facebook URLs are supported",
        ));
    }

    // Try well-known scrapers in order — Playwright is a LAST resort (heavy,
    // slow, and its naive response-harvesting returns many broken/404 links
    // mixed with the one good URL, which is exactly the "link exists but can't
    // download" bug we've seen).
    //
    // 1) yt-dlp facebook extractor — authoritative, returns direct fbcdn
    //    progressive URLs with REAL title/format metadata and only the working
    //    scales (sd + hd, no junk). Also handles /reel, /watch, /videos,
    //    story.php, video.php, fb.watch URL shapes.
    if let Ok(yt) = run_ytdlp_json(url, &["-f", "bestvideo+bestaudio/best"]).await {
        if let Some(res) = ytdlp_to_download_result(&yt) {
            if !res.media.is_empty() {
                return Ok(res);
            }
        }
    }

    // 2) downr.org all-in-one (same provider as Shirokami's /fbdl)
    match fetch_all_in_one(url).await {
        Ok(result) if !result.media.is_empty() => return Ok(result),
        Ok(_) => {}
        Err(e) => {
            eprintln!(
                "downr.org failed for {}: {}, trying Playwright fallback",
                url, e
            );
        }
    }

    // 3) Playwright browser scraping to extract video URLs
    let platform = if valid_ig { "instagram" } else { "facebook" };
    let data = run_playwright_scraper(url, platform).await?;

    // Build result from Playwright scraper output
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut result = DownloadResult::success(title);
    result.provider = data
        .get("provider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let media_arr = data.get("media").and_then(|v| v.as_array());
    if let Some(medias) = media_arr {
        for m in medias {
            let u = m
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Drop Playwright-harvested links that are actually broken
            // byte-range chunks (fbcdn URLs with bytestart=…byteend=… that
            // return tiny 200 responses — they 404 or return an HTML error
            // page when fetched as a whole), and the empty-string url.
            if u.is_empty() || u.contains("bytestart=") {
                continue;
            }
            let item = MediaItem {
                url: u,
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
                note: None,
            };
            result.media.push(item);
        }
    }

    // De-duplicate by URL (Playwright harvests the same CDN file at many
    // byte-ranges, and the primary URL appears multiple times).
    {
        let mut seen = std::collections::HashSet::new();
        result.media.retain(|m| seen.insert(m.url.clone()));
    }

    if result.media.is_empty() {
        return Ok(DownloadResult::error(format!(
            "Failed to extract media from {} — server IP may be blocked by anti-bot protection",
            platform
        )));
    }

    Ok(result)
}

/// Convert a yt-dlp --dump-single-json result into a DownloadResult with the
/// direct progressive/adaptive media URLs (only entries that carry a URL).
/// Used by the Facebook downloader as the primary provider.
fn ytdlp_to_download_result(data: &serde_json::Value) -> Option<DownloadResult> {
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
/// TikTok via embed-page scraping (primary method).
/// Scrapes `https://www.tiktok.com/embed/v2/{video_id}` HTML and extracts the
/// direct `v16m.tiktokcdn.com` MP4 URL from the `<video data-testid="play-video">` tag.
/// This works server-side (no auth) for active videos while the main site/API are
/// Cloudflare-blocked. Verified: returns a real downloadable MP4 (200/206, video/mp4).
async fn fetch_tiktok_embed(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Resolve short URLs (vm/vt.tiktok.com) to the long form to get a video ID
    let resolved_url = resolve_tiktok_url(url).await?;

    let video_id = regex::Regex::new(r"/video/(\d+)")
        .unwrap()
        .captures(&resolved_url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ScrapingError::Http("Invalid TikTok URL, no video ID".to_string()))?;

    let client = http_client();
    let html = client
        .client()
        .get(format!("https://www.tiktok.com/embed/v2/{}", video_id))
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .header("accept-language", "en-US,en;q=0.9")
        .timeout(Duration::from_secs(25))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok embed fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok embed read failed: {}", e)))?;

    // Extract <video data-testid="play-video" src="..."> (prefer the clean one)
    let mp4_url = {
        let mut found: Option<String> = None;
        for pat in [
            r#"<video[^>]*data-testid="play-video"[^>]*src="([^"]+)""#,
            r#"<video[^>]*src="([^"]+)"[^>]*data-testid="play-video""#,
            r#"<video[^>]*src="([^"]+)""#,
        ] {
            if let Some(m) = regex::Regex::new(pat).unwrap().captures(&html) {
                if let Some(url) = m.get(1) {
                    // prefer a *.mp4 URL over thumbnails/images
                    let cand = url.as_str().replace("&amp;", "&");
                    if cand.contains("tiktokcdn") || cand.ends_with(".mp4") {
                        found = Some(cand);
                        break;
                    }
                    if found.is_none() {
                        found = Some(cand);
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    };

    let mp4_url =
        mp4_url.ok_or_else(|| ScrapingError::Http("No video URL in TikTok embed".to_string()))?;

    // Extract title + author from <title> and oEmbed-like metadata if available
    let (title, author) = {
        let mut title = None;
        let mut author = None;
        if let Some(m) = regex::Regex::new(r#"<title[^>]*>([^<]+)</title>"#)
            .unwrap()
            .captures(&html)
        {
            let raw = m
                .get(1)
                .map(|s| s.as_str().trim().to_string())
                .unwrap_or_default();
            if !raw.is_empty() && !raw.contains("TikTok") {
                title = Some(raw);
            }
        }
        if let Some(m) = regex::Regex::new(r#"data-author-name="([^"]+)""#)
            .unwrap()
            .captures(&html)
        {
            author = m.get(1).map(|s| s.as_str().to_string());
        }
        (title, author)
    };

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.provider = Some("tiktok-embed".to_string());

    result.media.push(MediaItem {
        url: mp4_url.clone(),
        quality: Some("hd".to_string()),
        file_type: Some(MediaType::Video),
        extension: Some("mp4".to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: None,
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

/// Resolve TikTok short URLs (vm/vt.tiktok.com/xxx) to the canonical long URL.
async fn resolve_tiktok_url(url: &str) -> Result<String, ScrapingError> {
    if !url.contains("tiktok.com") {
        return Ok(url.to_string());
    }
    // If it already has /video/<id>, return as-is
    if regex::Regex::new(r"/video/\d+").unwrap().is_match(url) {
        return Ok(url.to_string());
    }
    // Short URL: follow redirects to get the canonical URL
    let client = http_client();
    let resp = client
        .client()
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok redirect resolve failed: {}", e)))?;
    Ok(resp.url().to_string())
}

pub async fn fetch_tiktok(url: &str) -> Result<DownloadResult, ScrapingError> {
    if extract_tiktok_id(url).is_none() {
        return Ok(DownloadResult::error("Invalid URL"));
    }

    // Primary: scrape the embed page for the direct MP4 (works server-side)
    if let Ok(embed_result) = fetch_tiktok_embed(url).await {
        if !embed_result.media.is_empty() {
            return Ok(embed_result);
        }
    }

    // Try tikwm API first; fall back to yt-dlp if blocked
    let resp = http_client()
        .client()
        .get("https://www.tikwm.com/api/")
        .query(&[("url", url), ("hd", "1")])
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok fetch failed: {}", e)))?;

    let resp_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok JSON parse failed: {}", e)))?;

    // Check if tikwm returned an error (e.g. Cloudflare blocked)
    let tikwm_code = resp_json.get("code");
    let tikwm_msg = resp_json.get("msg").and_then(|v| v.as_str());

    if tikwm_code == Some(&serde_json::Value::Number(serde_json::Number::from(0)))
        && tikwm_msg != Some("Url parsing is failed! Please check url.")
    {
        let mut result = DownloadResult::success(
            resp_json
                .get("data")
                .and_then(|d| d.get("title"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        );
        result.author = resp_json
            .get("data")
            .and_then(|d| d.get("author"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        result.provider = Some("tikwm".to_string());
        result.thumbnail = resp_json
            .get("data")
            .and_then(|d| d.get("cover"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(plays) = resp_json
            .get("data")
            .and_then(|d| d.get("plays").and_then(|v| v.as_array()))
        {
            for play in plays {
                if let Some(play_url) = play.get("url").and_then(|v| v.as_str()) {
                    result.media.push(MediaItem {
                        url: play_url.to_string(),
                        quality: play
                            .get("quality")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        file_type: Some(MediaType::Video),
                        extension: Some("mp4".to_string()),
                        thumbnail: None,
                        file_size: play
                            .get("size")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        size_bytes: play
                            .get("size")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.parse().ok()),
                        frame_width: None,
                        frame_height: None,
                        note: None,
                    });
                }
            }
        }

        if result.media.is_empty() {
            result.message = Some("No download URLs found".to_string());
            return Ok(result);
        }
        return Ok(result);
    }

    // Fallback: use yt-dlp
    match run_ytdlp_json(url, &[]).await {
        Ok(data) => {
            let title = data
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let author = data
                .get("uploader")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let thumbnail = data
                .get("thumbnail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut result = DownloadResult::success(title);
            result.author = author;
            result.thumbnail = thumbnail;
            result.provider = Some("yt-dlp".to_string());

            if let Some(formats) = data.get("formats").and_then(|v| v.as_array()) {
                for fmt in formats {
                    if let Some(fmt_url) = fmt.get("url").and_then(|v| v.as_str()) {
                        result.media.push(MediaItem {
                            url: fmt_url.to_string(),
                            quality: fmt
                                .get("format_note")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    fmt.get("height")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                }),
                            file_type: fmt
                                .get("vcodec")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty() && *s != "none")
                                .map(|_| MediaType::Video),
                            extension: fmt
                                .get("ext")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            thumbnail: None,
                            file_size: fmt
                                .get("filesize")
                                .and_then(|v| v.as_u64())
                                .map(format_filesize),
                            size_bytes: fmt.get("filesize").and_then(|v| v.as_u64()),
                            frame_width: fmt
                                .get("width")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            frame_height: fmt
                                .get("height")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            note: None,
                        });
                    }
                }
            }

            if result.media.is_empty() {
                result.message = Some("No download URLs found".to_string());
            }
            return Ok(result);
        }
        Err(e) => {
            eprintln!("yt-dlp failed for TikTok, trying Playwright: {}", e);

            // Fallback: use Playwright browser scraping
            match run_playwright_scraper(url, "tiktok").await {
                Ok(data) => {
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
                                file_type: m.get("ext").and_then(|v| v.as_str()).and_then(|e| {
                                    match e {
                                        "mp4" | "m3u8" => Some(MediaType::Video),
                                        "mp3" | "m4a" => Some(MediaType::Audio),
                                        _ => Some(MediaType::Video),
                                    }
                                }),
                                extension: m
                                    .get("ext")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                thumbnail: None,
                                file_size: None,
                                size_bytes: None,
                                frame_width: None,
                                frame_height: None,
                                note: None,
                            };
                            result.media.push(item);
                        }
                    }

                    if result.media.is_empty() {
                        result.message = Some("No download URLs found".to_string());
                    }
                    return Ok(result);
                }
                Err(_) => {
                    // Last resort: douyin.wtf hybrid API (v2). If it also fails,
                    // report all methods exhausted.
                    match fetch_tiktok_v2(url).await {
                        Ok(v2_result) if !v2_result.media.is_empty() => return Ok(v2_result),
                        _ => {
                            return Err(ScrapingError::Http(
                                "All TikTok download methods failed (tikwm, yt-dlp, Playwright, embed, douyin.wtf)".to_string()
                            ));
                        }
                    }
                }
            }
        }
    }
}

/// TikTok v2 — uses douyin.wtf API
pub async fn fetch_tiktok_v2(url: &str) -> Result<DownloadResult, ScrapingError> {
    let resp = http_client()
        .client()
        .get("https://douyin.wtf/api/hybrid/video_data")
        .query(&[("url", url), ("minimal", "true")])
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok v2 fetch failed: {}", e)))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| ScrapingError::Http(format!("TikTok v2 JSON parse failed: {}", e)))?;

    let data = &resp["data"];
    let mut result = DownloadResult::success(
        data.get("desc")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    result.author = data
        .get("author")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    result.provider = Some("douyin.wtf".to_string());

    if let Some(video_data) = data.get("video_data") {
        if let Some(url) = video_data.get("play").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: url.to_string(),
                quality: video_data
                    .get("quality")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_type: Some(MediaType::Video),
                extension: Some("mp4".to_string()),
                thumbnail: video_data
                    .get("cover")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_size: video_data
                    .get("size")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            });
        }
        if let Some(url) = video_data.get("music").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: url.to_string(),
                quality: Some("audio".to_string()),
                file_type: Some(MediaType::Audio),
                extension: Some("mp3".to_string()),
                thumbnail: None,
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            });
        }
    }

    Ok(result)
}

// ============================================================================
// YouTube downloaders (via savetube.media)
// ============================================================================

/// YouTube audio via savetube.media — returns metadata + direct download link.
pub async fn fetch_youtube_mp3(url: &str) -> Result<DownloadResult, ScrapingError> {
    extract_youtube_id(url)
        .ok_or_else(|| ScrapingError::Http("Invalid YouTube URL".to_string()))?;

    let data = run_ytdlp_json(url, &["-f", "bestaudio/best"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    let download_url = data
        .get("url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            data.get("formats")
                .and_then(|v| v.as_array())
                .and_then(|f| f.first())
                .and_then(|f| f.get("url").and_then(|v| v.as_str()))
        })
        .ok_or_else(|| ScrapingError::Http("No audio format URL from yt-dlp".to_string()))?;

    result.media.push(MediaItem {
        url: download_url.to_string(),
        quality: Some(format!(
            "{}kbps",
            data.get("abr").and_then(|v| v.as_u64()).unwrap_or(128)
        )),
        file_type: Some(MediaType::Audio),
        extension: data
            .get("ext")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some("webm".to_string())),
        thumbnail: data
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        file_size: data
            .get("filesize")
            .and_then(|v| v.as_u64())
            .map(format_filesize),
        size_bytes: data.get("filesize").and_then(|v| v.as_u64()),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

/// YouTube video via yt-dlp
pub async fn fetch_youtube_mp4(url: &str, quality: &str) -> Result<DownloadResult, ScrapingError> {
    extract_youtube_id(url)
        .ok_or_else(|| ScrapingError::Http("Invalid YouTube URL".to_string()))?;

    let q = quality.trim_end_matches('p');

    // ---------------------------------------------------------------
    // 1) PREFER a PROGRESSIVE format (video + audio in ONE file).
    //    The `android` player client exposes progressive itags (18/22) that
    //    are NOT rate-limited from datacenter IPs (verified 2026-09-05:
    //    itag 18 downloads at full speed, while audio-only streams crawl).
    //    Fall back to adaptive (bestvideo+bestaudio) below when no
    //    progressive format exists at the requested height.
    // ---------------------------------------------------------------
    let prog_fmt = format!("best[height<={}][ext=mp4]/best[height<={}]", q, q);
    if let Ok(prog_data) = run_ytdlp_json(
        url,
        &[
            "--extractor-args",
            "youtube:player_client=android",
            "-f",
            &prog_fmt,
        ],
    )
    .await
    {
        let prog_url = prog_data
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let prog_vcodec = prog_data
            .get("vcodec")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let prog_acodec = prog_data
            .get("acodec")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let has_audio = prog_acodec != "none" && !prog_acodec.is_empty();
        let has_video = prog_vcodec != "none" && !prog_vcodec.is_empty();
        if let Some(purl) = prog_url {
            if has_audio && has_video && purl.starts_with("http") {
                let height = prog_data
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(360);
                // Only use the progressive format if it actually meets the
                // requested quality. `best[height<=Q][ext=mp4]` can return a
                // lower-height progressive (e.g. 360p when 720p was requested)
                // because no 720p progressive exists; in that case fall through
                // to the adaptive (bestvideo+bestaudio) path so the caller
                // still gets the requested resolution.
                let q_num: u64 = q.parse().unwrap_or(0);
                if q_num != 0 && height < q_num {
                    // fall through to adaptive below
                } else {
                    let ext = prog_data
                        .get("ext")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mp4");
                    let mut result = DownloadResult::success(
                        prog_data
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    );
                    result.author = prog_data
                        .get("uploader")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    result.thumbnail = prog_data
                        .get("thumbnail")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    result.duration = prog_data
                        .get("duration")
                        .and_then(|v| v.as_u64())
                        .map(|d| format!("{}s", d));
                    result.provider = Some("yt-dlp".to_string());
                    result.media.push(MediaItem {
                        url: purl,
                        quality: Some(format!("{}p", height)),
                        file_type: Some(MediaType::Video),
                        extension: Some(ext.to_string()),
                        thumbnail: prog_data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        file_size: prog_data
                            .get("filesize")
                            .and_then(|v| v.as_u64())
                            .map(format_filesize),
                        size_bytes: prog_data.get("filesize").and_then(|v| v.as_u64()),
                        frame_width: prog_data
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .map(|w| w.to_string()),
                        frame_height: prog_data
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .map(|h| h.to_string()),
                        note: Some("Progressive MP4 — video and audio in one file".to_string()),
                    });
                    return Ok(result);
                }
            }
        }
    }

    let fmt = format!(
        "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
        q, q
    );
    let data = run_ytdlp_json(url, &["-f", &fmt, "--merge-output-format", "mp4"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    let formats = data.get("formats").and_then(|v| v.as_array());
    let requested_formats = data.get("requested_formats").and_then(|v| v.as_array());
    let mut seen = std::collections::HashSet::new();

    // When yt-dlp uses a merged format (bestvideo+bestaudio), the actual
    // download URLs live in `requested_formats` (video + audio separate URLs).
    // The top-level `url` is empty and `formats[]` is the FULL list of all
    // individual format IDs, not just the selected ones.
    if let Some(rf) = requested_formats {
        for f in rf {
            if let (Some(furl), Some(ext_val)) = (
                f.get("url").and_then(|v| v.as_str()),
                f.get("ext").and_then(|v| v.as_str()),
            ) {
                let proto = f.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                // Skip mhtml/storyboard and m3u8/hls streams
                if ext_val == "mhtml" || proto.contains("m3u8") {
                    continue;
                }
                let url_string = furl.to_string();
                if !url_string.starts_with("http") {
                    continue;
                }
                if seen.insert(url_string.clone()) {
                    let is_video = f
                        .get("vcodec")
                        .and_then(|v| v.as_str())
                        .map(|c| c != "none")
                        .unwrap_or(false);
                    let is_audio_only = f
                        .get("acodec")
                        .and_then(|v| v.as_str())
                        .map(|c| c != "none")
                        .unwrap_or(false);
                    let fmt_type = if is_video && is_audio_only {
                        MediaType::Video
                    } else if is_audio_only {
                        MediaType::Audio
                    } else if is_video {
                        MediaType::Video
                    } else {
                        MediaType::File
                    };
                    let q = format!("{}p", f.get("height").and_then(|v| v.as_u64()).unwrap_or(0));
                    result.media.push(MediaItem {
                        url: url_string,
                        quality: Some(q),
                        file_type: Some(fmt_type),
                        extension: Some(ext_val.to_string()),
                        thumbnail: data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
        }
    } else if let Some(fmts) = formats {
        for f in fmts {
            if let (Some(furl), Some(ext_val)) = (
                f.get("url").and_then(|v| v.as_str()),
                f.get("ext").and_then(|v| v.as_str()),
            ) {
                // Skip storyboard, mhtml, m3u8/hls, and audio-only formats.
                // We only want formats that have both video and audio.
                let proto = f.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                if ext_val == "mhtml" || proto.contains("m3u8") {
                    continue;
                }
                // Skip audio-only formats (vcodec == "none", acodec != "none")
                let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
                let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
                if vcodec == "none" && acodec != "none" {
                    continue; // audio-only
                }
                let url_string = furl.to_string();
                if !url_string.starts_with("http") {
                    continue;
                }
                if seen.insert(url_string.clone()) {
                    let fmt_type = if ext_val == "mp4" || ext_val == "webm" || ext_val == "mkv" {
                        MediaType::Video
                    } else {
                        MediaType::File
                    };
                    let q = format!("{}p", f.get("height").and_then(|v| v.as_u64()).unwrap_or(0));
                    result.media.push(MediaItem {
                        url: url_string,
                        quality: Some(q),
                        file_type: Some(fmt_type),
                        extension: Some(ext_val.to_string()),
                        thumbnail: data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
        }
    }

    if result.media.is_empty() {
        if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: url.to_string(),
                quality: Some(format!("{}p", quality.trim_end_matches('p'))),
                file_type: Some(MediaType::Video),
                extension: Some(
                    data.get("ext")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mp4")
                        .to_string(),
                ),
                thumbnail: data
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
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

    Ok(result)
}

// ============================================================================
// YouTube video + audio SERVER-SIDE MERGE (produces a single MP4 with audio).
//
// Modern YouTube videos (AV1/VP9, 720p+) only expose separate video-only and
// audio-only streams. `fetch_youtube_mp4` returns those two URLs — the caller
// would have to merge them manually. This endpoint downloads both streams and
// remuxes them into ONE MP4 with `ffmpeg`, stored under the uploads dir and
// served via GET /file/yt_merge/{name}. Files older than 2h are cleaned up on
// each new merge request to bound disk usage.
// ============================================================================
pub async fn fetch_youtube_merge_free(
    url: &str,
    quality: &str,
) -> Result<DownloadResult, ScrapingError> {
    let video_id = extract_youtube_id(url)
        .ok_or_else(|| ScrapingError::Http("Invalid YouTube URL".to_string()))?;

    let q = quality.trim_end_matches('p');
    let fmt = format!(
        "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
        q, q
    );
    let data = run_ytdlp_json(url, &["-f", &fmt, "--merge-output-format", "mp4"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    // Locate the video-only and audio-only URLs from requested_formats.
    let requested_formats = data
        .get("requested_formats")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ScrapingError::Http("No requested_formats from yt-dlp".to_string()))?;

    let mut video_url: Option<String> = None;
    let mut audio_url: Option<String> = None;
    let mut video_ext = "mp4";
    let mut audio_ext = "webm";
    for f in requested_formats {
        let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
        let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
        let f_url = f.get("url").and_then(|v| v.as_str());
        let proto = f.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
        if proto.contains("m3u8") {
            continue; // HLS streams are not directly downloadable via reqwest
        }
        if vcodec != "none" && acodec == "none" {
            if let Some(u) = f_url {
                video_url = Some(u.to_string());
                video_ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("mp4");
            }
        } else if vcodec == "none" && acodec != "none" {
            if let Some(u) = f_url {
                audio_url = Some(u.to_string());
                audio_ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("webm");
            }
        }
    }

    let video_url = video_url
        .ok_or_else(|| ScrapingError::Http("No video stream URL from yt-dlp".to_string()))?;
    let audio_url = audio_url
        .ok_or_else(|| ScrapingError::Http("No audio stream URL from yt-dlp".to_string()))?;

    // --- download both streams to the merge temp dir ---
    let merge_dir = PathBuf::from("/var/lib/scraper/uploads/yt_merge");
    tokio::fs::create_dir_all(&merge_dir)
        .await
        .map_err(|e| ScrapingError::Http(format!("create merge dir failed: {e}")))?;

    // Cleanup stale merged files (> 2h) before writing anything new.
    cleanup_stale_merges(&merge_dir).await;

    let job_id = uuid::Uuid::new_v4().simple().to_string();
    let vpath = merge_dir.join(format!("{video_id}_{job_id}_v.{video_ext}"));
    let apath = merge_dir.join(format!("{video_id}_{job_id}_a.{audio_ext}"));
    let outpath = merge_dir.join(format!("{video_id}_{job_id}_merged.mp4"));

    download_to_file(&video_url, &vpath).await?;
    download_to_file(&audio_url, &apath).await?;

    // --- merge with ffmpeg ---
    let merge_result = tokio::task::spawn_blocking({
        let vpath = vpath.clone();
        let apath = apath.clone();
        let outpath = outpath.clone();
        move || merge_with_ffmpeg(&vpath, &apath, &outpath)
    })
    .await
    .map_err(|e| ScrapingError::Http(format!("ffmpeg join failed: {e}")))?;

    match merge_result {
        Ok(()) => {}
        Err(e) => {
            let _ = tokio::fs::remove_file(&vpath).await;
            let _ = tokio::fs::remove_file(&apath).await;
            return Err(ScrapingError::Http(format!("ffmpeg merge failed: {e}")));
        }
    }

    // Clean up the transient inputs now that the merged file exists.
    let _ = tokio::fs::remove_file(&vpath).await;
    let _ = tokio::fs::remove_file(&apath).await;

    let file_size = tokio::fs::metadata(&outpath).await.ok().map(|m| m.len());

    let file_name = outpath
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let public_url = format!("/file/yt_merge/{file_name}");

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail.clone();
    result.duration = duration;
    result.provider = Some("yt-dlp+ffmpeg".to_string());
    result.media.push(MediaItem {
        url: public_url,
        quality: Some(format!("{}p merged", q)),
        file_type: Some(MediaType::Video),
        extension: Some("mp4".to_string()),
        thumbnail: thumbnail.clone(),
        file_size: file_size.map(format_filesize),
        size_bytes: file_size,
        frame_width: None,
        frame_height: None,
        note: Some(
            "Merged server-side: single MP4 file containing both video and audio".to_string(),
        ),
    });

    Ok(result)
}

/// Download `url` to `path` with a generous timeout. The yt CDN stream URLs are
/// signed and short-lived, so we must download promptly and cannot retry after
/// the URL expires.
async fn download_to_file(url: &str, path: &Path) -> Result<(), ScrapingError> {
    // Dedicated client with a longer timeout than the shared 30s one: merged
    // files are often tens of MB and the yt CDN can be slow from a datacenter.
    // .no_gzip()/.no_brotli()/.no_deflate() disable reqwest's auto-decompress
    // so we can write raw bytes — some CDN responses are mis-labelled and fail
    // the auto-decode step.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(20))
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .build()
        .map_err(|e| ScrapingError::Http(format!("stream client build failed: {e}")))?;
    let resp = client
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("stream download failed: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(ScrapingError::Http(format!(
            "stream download HTTP {status}"
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ScrapingError::Http(format!("stream body read failed: {e}")))?;

    tokio::fs::write(path, bytes)
        .await
        .map_err(|e| ScrapingError::Http(format!("stream write failed: {e}")))?;
    Ok(())
}

/// Run ffmpeg to remux the video and audio streams into a single MP4.
/// `-c:v copy` avoids a full re-encode (fast); audio is transcoded to AAC so
/// the output plays everywhere. `+faststart` moves the moov atom to the front
/// so the file streams immediately.
fn merge_with_ffmpeg(video: &Path, audio: &Path, out: &Path) -> Result<(), String> {
    let status = Command::new("/bin/ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            video.to_str().unwrap_or_default(),
            "-i",
            audio.to_str().unwrap_or_default(),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
            "-shortest",
            out.to_str().unwrap_or_default(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("spawn ffmpeg failed: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg exited with {status:?}"))
    }
}

/// Remove merged output + transient files older than 2 hours. Bounded, best
/// effort — never fails the request.
async fn cleanup_stale_merges(dir: &Path) {
    let deadline = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < deadline {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

// Spotify downloaders
// ============================================================================

pub async fn fetch_spotify(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Validate Spotify URL
    let re = regex::Regex::new(
        r"https?://open\.spotify\.com/(?:intl-[a-zA-Z0-9-]+/)?(track|album|playlist)/([a-zA-Z0-9]+)",
    )
    .unwrap();
    let captures = re
        .captures(url)
        .ok_or_else(|| ScrapingError::Http("Invalid Spotify URL".to_string()))?;
    let _resource_type = captures.get(1).map(|m| m.as_str()).unwrap_or("track");
    let resource_id = captures.get(2).map(|m| m.as_str()).unwrap_or("");

    // Downtify-style flow: Spotify is DRM-protected, so we can't download from
    // Spotify directly. Instead we (1) resolve the track title via the Spotify
    // oEmbed endpoint (works with no auth), then (2) search YouTube Music via
    // yt-dlp and return the matched audio URL. This scraping-based approach
    // yields a real, playable YouTube media URL without Premium.
    //
    // Step 1: resolve track metadata via oEmbed.
    let oembed_url = format!(
        "https://open.spotify.com/oembed?url=https://open.spotify.com/track/{}",
        resource_id
    );
    let oembed_title: Option<String> = {
        let client = http_client();
        match client
            .client()
            .get(&oembed_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(v) => v
                        .get("title")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string()),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    };

    // Step 2: search YouTube Music via yt-dlp for the track.
    let search_query = match &oembed_title {
        Some(t) if !t.trim().is_empty() => t.clone(),
        // Fallback: if oEmbed failed, try a web search for the title via
        // yt-dlp using the track ID as a last resort.
        _ => format!("spotify:track:{}", resource_id),
    };

    let search = format!("ytsearch:{}", search_query);
    let data = match run_ytdlp_json(&search, &["--default-search", "auto"]).await {
        Ok(d) => d,
        Err(e) => {
            // Both oEmbed and yt-dlp failed. Return a graceful message.
            let mut result = DownloadResult::success(None);
            result.provider = Some("spotify".to_string());
            result.media.push(MediaItem {
                url: format!("https://open.spotify.com/track/{}", resource_id),
                quality: Some("metadata".to_string()),
                file_type: Some(MediaType::File),
                extension: Some("json".to_string()),
                thumbnail: None,
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: Some(format!(
                    "Spotify is DRM-protected; could not resolve a matching source on YouTube. {}",
                    e
                )),
            });
            return Ok(result);
        }
    };

    // Extract title/author/thumbnail from the matched YouTube track.
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or(oembed_title);
    let author = data
        .get("artist")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));
    let webpage_url = data
        .get("webpage_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut result = DownloadResult::success(title.clone());
    result.author = author;
    result.thumbnail = thumbnail.clone();
    result.duration = duration;
    result.provider = Some("spotify-oembed+ytsearch".to_string());

    // Extract the best audio format URL.
    let download_url = data.get("url").and_then(|v| v.as_str()).or_else(|| {
        data.get("formats")
            .and_then(|v| v.as_array())
            .and_then(|f| {
                f.iter()
                    .filter(|fmt| {
                        fmt.get("acodec")
                            .and_then(|c| c.as_str())
                            .map(|c| c != "none")
                            .unwrap_or(false)
                    })
                    .max_by_key(|fmt| fmt.get("abr").and_then(|v| v.as_u64()).unwrap_or(0))
            })
            .and_then(|f| f.get("url").and_then(|v| v.as_str()))
    });

    let note = Some(
        "Resolved this Spotify track to a YouTube Music source via oEmbed + yt-dlp search (Spotify itself is DRM-protected)."
            .to_string(),
    );

    if let Some(dl_url) = download_url {
        result.media.push(MediaItem {
            url: dl_url.to_string(),
            quality: Some("high".to_string()),
            file_type: Some(MediaType::Audio),
            extension: Some("mp3".to_string()),
            thumbnail,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note,
        });
    } else {
        result.media.push(MediaItem {
            url: webpage_url
                .clone()
                .unwrap_or_else(|| format!("https://open.spotify.com/track/{}", resource_id)),
            quality: Some("metadata".to_string()),
            file_type: Some(MediaType::File),
            extension: Some("html".to_string()),
            thumbnail,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note: Some(
                "Matched YouTube source found but no direct audio URL was extractable.".to_string(),
            ),
        });
    }
    Ok(result)
}

// Twitter downloaders
// ============================================================================

/// Twitter/X via the Syndication API (primary method, tokenless, works server-side).
/// Scrapes `https://cdn.syndication.twimg.com/tweet-result?id={tweet_id}&lang=en&token=0`
/// which returns JSON with the tweet's video `variants[]` (MP4 URLs at multiple bitrates).
/// Verified: returns real downloadable video.twimg.com MP4s (200/206, video/mp4) — no auth.
async fn fetch_twitter_syndication(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Extract tweet ID from twitter.com/x.com URL
    let tweet_id = regex::Regex::new(r"(?:twitter\.com|x\.com)/[^/]+/status/(\d+)")
        .unwrap()
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| ScrapingError::Http("Invalid Twitter URL, no status ID".to_string()))?;

    let api_url = format!(
        "https://cdn.syndication.twimg.com/tweet-result?id={}&lang=en&token=0",
        tweet_id
    );

    let client = http_client();
    let resp = client
        .client()
        .get(&api_url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Twitter syndication fetch failed: {}", e)))?;

    if resp.status() != reqwest::StatusCode::OK {
        return Err(ScrapingError::Http(format!(
            "Twitter syndication returned HTTP {}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        ScrapingError::Http(format!("Twitter syndication JSON parse failed: {}", e))
    })?;

    let text = data
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut result = DownloadResult::success(text);
    result.author = data
        .get("user")
        .and_then(|v| v.get("screen_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    result.provider = Some("twitter-syndication".to_string());

    // Recursively find media with video_info.variants (video/mp4)
    let mut variants: Vec<(u64, String)> = Vec::new();
    let collect = |node: &serde_json::Value, variants: &mut Vec<(u64, String)>| {
        fn walk(n: &serde_json::Value, out: &mut Vec<(u64, String)>, depth: usize) {
            if depth > 6 {
                return;
            }
            if let Some(obj) = n.as_object() {
                if let Some(video_info) = obj.get("video_info") {
                    if let Some(vars) = video_info.get("variants").and_then(|v| v.as_array()) {
                        for v in vars {
                            if v.get("content_type").and_then(|c| c.as_str()) == Some("video/mp4") {
                                if let Some(u) = v.get("url").and_then(|x| x.as_str()) {
                                    let bitrate =
                                        v.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0);
                                    out.push((bitrate, u.to_string()));
                                }
                            }
                        }
                    }
                }
                for (_, v) in obj {
                    walk(v, out, depth + 1);
                }
            } else if let Some(arr) = n.as_array() {
                for it in arr {
                    walk(it, out, depth + 1);
                }
            }
        }
        walk(node, variants, 0);
    };
    collect(&data, &mut variants);

    // Sort by bitrate desc (highest quality first)
    variants.sort_by(|a, b| b.0.cmp(&a.0));

    for (bitrate, video_url) in variants {
        result.media.push(MediaItem {
            url: video_url,
            quality: Some(format!("{}k", bitrate / 1000)),
            file_type: Some(MediaType::Video),
            extension: Some("mp4".to_string()),
            thumbnail: None,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note: None,
        });
    }

    Ok(result)
}

pub async fn fetch_twitter(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Primary: use the tokenless Syndication API (works server-side, no auth)
    if let Ok(synd_result) = fetch_twitter_syndication(url).await {
        if !synd_result.media.is_empty() {
            return Ok(synd_result);
        }
    }

    let client = http_client();
    let ua = "PostmanRuntime/7.32.2";

    let resp = client
        .client()
        .post("https://savetwitter.net/api/ajaxSearch")
        .header(USER_AGENT, ua)
        .header("accept", "*/*")
        .header("content-type", "application/x-www-form-urlencoded")
        .form(&[("q", url), ("lang", "en")])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Twitter fetch failed: {}", e)))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| ScrapingError::Http(format!("Twitter JSON parse failed: {}", e)))?;

    let html = resp.get("data").and_then(|v| v.as_str()).ok_or_else(|| {
        let msg = resp
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("No data in Twitter response");
        ScrapingError::Http(msg.to_string())
    })?;

    // Parse savetwitter HTML inside a block so all scraper types
    // (Html/Selector/ElementRef are NOT Send) go out of scope before the
    // Playwright fallback .await below keeps the future Send.
    let mut parsed_media: Vec<(String, Option<String>, Option<MediaType>, Option<String>)> =
        Vec::new();
    {
        let document = scraper::Html::parse_document(html);
        let tw_video_sel = scraper::Selector::parse("div.tw-video").unwrap();

        if document.select(&tw_video_sel).next().is_some() {
            if let Ok(item_sel) =
                scraper::Selector::parse("div.tw-right > div > p:nth-child(1) > a")
            {
                for item in document.select(&item_sel) {
                    let quality_text = item.text().collect::<String>();
                    let quality = if quality_text.contains("(") {
                        quality_text
                            .split("(")
                            .nth(1)
                            .and_then(|s| s.split("p").next())
                            .unwrap_or(&quality_text)
                            .trim()
                            .to_string()
                    } else {
                        quality_text.trim().to_string()
                    };
                    let href = item.value().attr("href").unwrap_or("").to_string();
                    parsed_media.push((
                        href,
                        Some(quality),
                        Some(MediaType::Video),
                        Some("mp4".to_string()),
                    ));
                }
            }
        } else {
            if let Ok(item_sel) = scraper::Selector::parse("div.video-data > div > ul > li") {
                for item in document.select(&item_sel) {
                    let href = item
                        .select(&scraper::Selector::parse("div > div:nth-child(2) > a").unwrap())
                        .next()
                        .and_then(|a| a.value().attr("href"))
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    if !href.is_empty() {
                        parsed_media.push((
                            href,
                            None,
                            Some(MediaType::Image),
                            Some("jpg".to_string()),
                        ));
                    }
                }
            }
        }
    } // document + selectors dropped here

    let mut result = DownloadResult::success(None);
    result.provider = Some("savetwitter".to_string());
    for (href, quality, file_type, extension) in parsed_media {
        result.media.push(MediaItem {
            url: href,
            quality,
            file_type,
            extension,
            thumbnail: None,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note: None,
        });
    }

    if result.media.is_empty() {
        // Fallback: Playwright browser scraping for Twitter video URLs
        match run_playwright_scraper(url, "twitter").await {
            Ok(data) => {
                let pw_result = playwright_to_download_result(&data);
                if !pw_result.media.is_empty() {
                    return Ok(pw_result);
                }
            }
            Err(_e) => {}
        }
        // Last resort: twitsave.com scrape (v2). If it also fails, report.
        match fetch_twitter_v2(url).await {
            Ok(v2_result) if !v2_result.media.is_empty() => return Ok(v2_result),
            _ => {
                return Ok(DownloadResult::error(
                    "Tidak dapat menemukan video (savetwitter, Playwright, twitsave semua gagal)",
                ));
            }
        }
    }

    // Sort by resolution desc
    result.media.sort_by(|a, b| {
        let qa = a
            .quality
            .as_ref()
            .and_then(|q| q.parse::<u32>().ok())
            .unwrap_or(0);
        let qb = b
            .quality
            .as_ref()
            .and_then(|q| q.parse::<u32>().ok())
            .unwrap_or(0);
        qb.cmp(&qa)
    });

    Ok(result)
}

/// Twitter v2 — uses twitsave.com
pub async fn fetch_twitter_v2(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let html = client
        .client()
        .get(format!("https://twitsave.com/info?url={}", url))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Twitter v2 fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("Twitter v2 response read failed: {}", e)))?;

    let document = scraper::Html::parse_document(&html);
    let mut result = DownloadResult::success(None);
    result.provider = Some("twitsave".to_string());

    if let Ok(item_sel) = scraper::Selector::parse("div.origin-top-right > ul > li") {
        for item in document.select(&item_sel) {
            if let Some(a) = item.select(&scraper::Selector::parse("a").unwrap()).next() {
                let resolution_text = item
                    .select(&scraper::Selector::parse("div > div > div").unwrap())
                    .next()
                    .map(|d| d.text().collect::<String>())
                    .unwrap_or_default();
                if resolution_text.contains("Resolution: ") {
                    let parts: Vec<&str> = resolution_text
                        .trim_start_matches("Resolution: ")
                        .splitn(2, 'x')
                        .collect();
                    let width = parts.get(0).unwrap_or(&"").to_string();
                    let height = parts
                        .get(1)
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    let video_url = a
                        .value()
                        .attr("href")
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    result.media.push(MediaItem {
                        url: video_url,
                        quality: Some(width),
                        file_type: Some(MediaType::Video),
                        extension: Some("mp4".to_string()),
                        thumbnail: None,
                        file_size: None,
                        size_bytes: None,
                        frame_width: None,
                        frame_height: Some(height.to_string()),
                        note: None,
                    });
                }
            }
        }
    }

    result.media.sort_by(|a, b| {
        let ha = a
            .frame_height
            .as_ref()
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        let hb = b
            .frame_height
            .as_ref()
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);
        hb.cmp(&ha)
    });

    if let Some(highest) = result.media.first().and_then(|m| m.frame_width.clone()) {
        result
            .media
            .retain(|m| m.frame_width.as_deref() == Some(&highest));
    }

    if result.media.is_empty() {
        return Ok(DownloadResult::error("Tidak dapat menemukan video"));
    }

    Ok(result)
}

// ============================================================================
// Bilibili downloader (cobalt-api)
// ============================================================================

pub async fn fetch_bilibili(url: &str) -> Result<DownloadResult, ScrapingError> {
    // yt-dlp supports both AV (/video/av123) and BV (/video/BV1xxx) IDs
    let data = run_ytdlp_json(url, &["-f", "bv*+ba/b"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    // Extract media from formats array
    if let Some(formats) = data.get("formats").and_then(|v| v.as_array()) {
        for fmt in formats {
            let url = fmt.get("url").and_then(|v| v.as_str());
            if url.is_some() {
                result.media.push(MediaItem {
                    url: url.unwrap().to_string(),
                    quality: fmt
                        .get("format_note")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            fmt.get("height")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        }),
                    file_type: fmt
                        .get("vcodec")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty() && *s != "none")
                        .map(|_| MediaType::Video),
                    extension: fmt
                        .get("ext")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    thumbnail: None,
                    file_size: fmt
                        .get("filesize")
                        .and_then(|v| v.as_u64())
                        .map(format_filesize),
                    size_bytes: fmt.get("filesize").and_then(|v| v.as_u64()),
                    frame_width: fmt
                        .get("width")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    frame_height: fmt
                        .get("height")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    note: None,
                });
            }
        }
    }

    Ok(result)
}

// ============================================================================
// SoundCloud downloader
// ============================================================================

pub async fn fetch_soundcloud(url: &str) -> Result<DownloadResult, ScrapingError> {
    // yt-dlp --extract-audio requires a download; use --dump-json for direct URL
    let data = run_ytdlp_json(url, &[]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    let download_url = data.get("url").and_then(|v| v.as_str()).or_else(|| {
        data.get("formats")
            .and_then(|v| v.as_array())
            .and_then(|f| f.first())
            .and_then(|f| f.get("url").and_then(|v| v.as_str()))
    });

    if let Some(dl_url) = download_url {
        result.media.push(MediaItem {
            url: dl_url.to_string(),
            quality: Some(format!(
                "{}kbps",
                data.get("abr").and_then(|v| v.as_u64()).unwrap_or(128)
            )),
            file_type: Some(MediaType::Audio),
            extension: Some("mp3".to_string()),
            thumbnail: data
                .get("thumbnail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
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

    Ok(result)
}
// ============================================================================
// Dailymotion downloader (via yt-dlp)
// ============================================================================

pub async fn fetch_dailymotion(url: &str) -> Result<DownloadResult, ScrapingError> {
    if !url.contains("dailymotion.com") {
        return Err(ScrapingError::Http("Invalid Dailymotion URL".to_string()));
    }

    let data = run_ytdlp_json(url, &["-f", "bestvideo+bestaudio/best"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    let formats = data.get("formats").and_then(|v| v.as_array());
    let mut seen = std::collections::HashSet::new();

    if let Some(fmts) = formats {
        for f in fmts {
            if let (Some(furl), Some(ext_val)) = (
                f.get("url").and_then(|v| v.as_str()),
                f.get("ext").and_then(|v| v.as_str()),
            ) {
                if ext_val == "mhtml" {
                    continue;
                }
                let url_string = furl.to_string();
                if seen.insert(url_string.clone()) {
                    let fmt_type = if ext_val == "mp4" || ext_val == "webm" || ext_val == "mkv" {
                        MediaType::Video
                    } else if ext_val == "mp3" || ext_val == "m4a" {
                        MediaType::Audio
                    } else {
                        MediaType::File
                    };
                    let q = format!("{}p", f.get("height").and_then(|v| v.as_u64()).unwrap_or(0));
                    result.media.push(MediaItem {
                        url: url_string,
                        quality: Some(q),
                        file_type: Some(fmt_type),
                        extension: Some(ext_val.to_string()),
                        thumbnail: data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
        }
    }

    // If no formats listed, fall back to the top-level url
    if result.media.is_empty() {
        if let Some(dl_url) = data.get("url").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: dl_url.to_string(),
                quality: Some(format!(
                    "{}p",
                    data.get("height").and_then(|v| v.as_u64()).unwrap_or(0)
                )),
                file_type: Some(MediaType::Video),
                extension: Some(
                    data.get("ext")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mp4")
                        .to_string(),
                ),
                thumbnail: data
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            });
        }
    }

    Ok(result)
}

// ============================================================================
// Reddit downloader (via yt-dlp)
// ============================================================================

pub async fn fetch_reddit(url: &str) -> Result<DownloadResult, ScrapingError> {
    if !url.contains("reddit.com") && !url.contains("redd.it") {
        return Err(ScrapingError::Http("Invalid Reddit URL".to_string()));
    }

    let data = run_ytdlp_json(url, &["-f", "bestvideo+bestaudio/best"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let author = data
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|d| format!("{}s", d));

    let mut result = DownloadResult::success(title);
    result.author = author;
    result.thumbnail = thumbnail;
    result.duration = duration;
    result.provider = Some("yt-dlp".to_string());

    let formats = data.get("formats").and_then(|v| v.as_array());
    let mut seen = std::collections::HashSet::new();

    if let Some(fmts) = formats {
        for f in fmts {
            if let (Some(furl), Some(ext_val)) = (
                f.get("url").and_then(|v| v.as_str()),
                f.get("ext").and_then(|v| v.as_str()),
            ) {
                if ext_val == "mhtml" {
                    continue;
                }
                let url_string = furl.to_string();
                if seen.insert(url_string.clone()) {
                    let fmt_type = if ext_val == "mp4" || ext_val == "webm" || ext_val == "mkv" {
                        MediaType::Video
                    } else if ext_val == "mp3" || ext_val == "m4a" {
                        MediaType::Audio
                    } else {
                        MediaType::File
                    };
                    let q = format!("{}p", f.get("height").and_then(|v| v.as_u64()).unwrap_or(0));
                    result.media.push(MediaItem {
                        url: url_string,
                        quality: Some(q),
                        file_type: Some(fmt_type),
                        extension: Some(ext_val.to_string()),
                        thumbnail: data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
        }
    }

    // If no formats listed, fall back to the top-level url
    if result.media.is_empty() {
        if let Some(dl_url) = data.get("url").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: dl_url.to_string(),
                quality: Some(format!(
                    "{}p",
                    data.get("height").and_then(|v| v.as_u64()).unwrap_or(0)
                )),
                file_type: Some(MediaType::Video),
                extension: Some(
                    data.get("ext")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mp4")
                        .to_string(),
                ),
                thumbnail: data
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            });
        }
    }

    Ok(result)
}

// ============================================================================
// Streamable downloader (via yt-dlp)
// ============================================================================

pub async fn fetch_streamable(url: &str) -> Result<DownloadResult, ScrapingError> {
    if !url.contains("streamable.com") {
        return Err(ScrapingError::Http("Invalid Streamable URL".to_string()));
    }

    let data = run_ytdlp_json(url, &["-f", "best"]).await?;

    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut result = DownloadResult::success(title);
    result.thumbnail = thumbnail;
    result.provider = Some("yt-dlp".to_string());

    let mut seen = std::collections::HashSet::new();
    if let Some(fmts) = data.get("formats").and_then(|v| v.as_array()) {
        for f in fmts {
            if let (Some(furl), Some(ext_val)) = (
                f.get("url").and_then(|v| v.as_str()),
                f.get("ext").and_then(|v| v.as_str()),
            ) {
                if ext_val == "mhtml" {
                    continue;
                }
                let url_string = furl.to_string();
                if seen.insert(url_string.clone()) {
                    result.media.push(MediaItem {
                        url: url_string,
                        quality: f
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .map(|h| format!("{}p", h)),
                        file_type: Some(MediaType::Video),
                        extension: Some(ext_val.to_string()),
                        thumbnail: data
                            .get("thumbnail")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
        }
    }

    if result.media.is_empty() {
        if let Some(dl_url) = data.get("url").and_then(|v| v.as_str()) {
            result.media.push(MediaItem {
                url: dl_url.to_string(),
                quality: None,
                file_type: Some(MediaType::Video),
                extension: Some(
                    data.get("ext")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mp4")
                        .to_string(),
                ),
                thumbnail: data
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: None,
            });
        }
    }

    Ok(result)
}

// ============================================================================
// Pinterest downloader
// ============================================================================

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

// ============================================================================
// Others: PixelDrain, DoodStream, TeraBox, Video, KrakenFiles, Mega, Danbooru
// ============================================================================

pub async fn fetch_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let html = client
        .client()
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("PixelDrain fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("PixelDrain read failed: {}", e)))?;

    let re = regex::Regex::new(r"window\.viewer_data\s*=\s*(\{.*?\});").unwrap();
    let m = re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str());
    let json_str =
        m.ok_or_else(|| ScrapingError::Http("Failed to retrieve viewer data".to_string()))?;

    let viewer_data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ScrapingError::Http(format!("PixelDrain JSON parse failed: {}", e)))?;

    let file_data = &viewer_data["api_response"];
    let file_id = file_data["id"].as_str().unwrap_or("");

    let mut result = DownloadResult::success(file_data["name"].as_str().map(|s| s.to_string()));
    result.provider = Some("pixeldrain".to_string());

    result.media.push(MediaItem {
        url: format!("https://pixeldrain.com/api/file/{}?download", file_id),
        quality: None,
        file_type: Some(MediaType::File),
        extension: None,
        thumbnail: None,
        file_size: Some(format_filesize(file_data["size"].as_u64().unwrap_or(0))),
        size_bytes: file_data["size"].as_u64(),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
    let id = regex::Regex::new(r"/[de]/([a-zA-Z0-9]+)")
        .unwrap()
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| ScrapingError::Http("Linknya tidak bisa diproses".to_string()))?;

    let proxy = "https://rv.lil-hacker.workers.dev/proxy?mirror=dood&url=";
    let client = http_client();

    // Get metadata from dood.li page
    let page_html = client
        .client()
        .get(format!("https://dood.li/d/{}", id))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream page fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream page read failed: {}", e)))?;

    // HTML parsing — extract all needed text BEFORE any .await to keep future Send
    // Scraper types (Html/HtmlElementRef/Selector) are NOT Send, so they must be
    // dropped before async points.
    let (page_title, page_length, page_uploadate) = {
        let document = scraper::Html::parse_document(&page_html);
        let text = |sel: &str| {
            document
                .select(&scraper::Selector::parse(sel).unwrap())
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
        };
        (text(".title-wrap h4"), text(".length"), text(".uploadate"))
    };

    // Get embedded player page
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36";
    let headers = {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(ua));
        h.insert("referer", HeaderValue::from_static("https://d000d.com/"));
        h
    };

    let embed_resp = client
        .client()
        .get(format!("{}{}/e/{}", proxy, "https://d000d.com", id))
        .headers(headers.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream embed fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream embed read failed: {}", e)))?;

    let cdn_match = regex::Regex::new(r"\$\.get\('([^']+)',")
        .unwrap()
        .captures(&embed_resp)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let cdn_path = cdn_match.ok_or_else(|| {
        ScrapingError::Http("Link Pass MD5 tidak ditemukan! Coba lagi nanti.".to_string())
    })?;

    // Generate random token (like JS crypto.randomBytes)
    let chars: String = (0..10)
        .map(|_| {
            let idx = fastrand::usize(..62);
            let c = if idx < 26 {
                (b'A' + idx as u8) as char
            } else if idx < 52 {
                (b'a' + (idx - 26) as u8) as char
            } else {
                (b'0' + (idx - 52) as u8) as char
            };
            c
        })
        .collect();

    let ds_resp = client
        .client()
        .get(format!("{}{}{}", proxy, "https://d000d.com", cdn_path))
        .headers(headers.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream DS fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream DS read failed: {}", e)))?;

    let _cm = cdn_path.split('/').last().unwrap_or("");

    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let direct_link = format!(
        "{}{}{}?token={}&expiry={}",
        proxy,
        "https://d000d.com",
        ds_resp.trim(),
        chars,
        expiry,
    );

    // Title/length/uploadate were extracted inside the scope block above
    // as owned strings to keep scraper::Html from crossing .await
    let (title, length_str, uploadate) = (page_title, page_length, page_uploadate);
    let mut result = DownloadResult::success(Some(title));
    result.provider = Some("doodstream".to_string());
    result.duration = Some(length_str);

    result.description = Some(uploadate);

    result.media.push(MediaItem {
        url: direct_link,
        quality: None,
        file_type: Some(MediaType::Video),
        extension: Some("mp4".to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: None,
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
    let pattern = regex::Regex::new(
        r"^https?://(?:www\.|1024)?terabox(?:app)?\.com/.*[?&]?surl=([a-zA-Z0-9_-]+)",
    )
    .unwrap();

    let mut surl = pattern
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    if surl.is_none() {
        // Try resolving redirect
        if !regex::Regex::new(r"^https?://(?:www\.|1024)?terabox(?:app)?\.com")
            .unwrap()
            .is_match(url)
        {
            return Ok(DownloadResult::error("Invalid TeraBox URL."));
        }

        let client = http_client();
        let resp = client
            .client()
            .get(url)
            .header(USER_AGENT, "Mozilla/5.0")
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| ScrapingError::Http(format!("TeraBox redirect fetch failed: {}", e)))?;

        let request_url = resp.url().to_string();
        surl = pattern
            .captures(&request_url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());
    }

    let surl = surl.ok_or_else(|| ScrapingError::Http("SURL not found.".to_string()))?;

    // Call local TeraBox resolver (Playwright + residential proxy + cookies)
    let resolver_url = std::env::var("TERABOX_RESOLVER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4092".to_string());
    let resolve_url = format!("{}/resolve?surl={}", resolver_url, surl);

    let client = http_client();
    let resp = client
        .client()
        .get(&resolve_url)
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TeraBox resolver fetch failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Ok(DownloadResult::error(format!(
            "TeraBox resolver returned HTTP {}: {}",
            status, body
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ScrapingError::Http(format!("TeraBox resolver JSON parse failed: {}", e)))?;

    if let Some(error) = data.get("error").and_then(|v| v.as_str()) {
        return Ok(DownloadResult::error(error));
    }

    let file_name = data
        .get("file_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut result = DownloadResult::success(file_name.clone());
    result.provider = Some("terabox".to_string());
    result.thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Primary download link.
    //
    // IMPORTANT: the resolver's download_link is a raw terabox dlink that
    // 403s with {"error_code":31045,"user not exists"} when fetched WITHOUT
    // cookies — even from residential IPs. So we rewrite the public URL to
    // OUR OWN /proxy/terabox streaming endpoint (which forwards the resolver's
    // cookie-authenticated stream through the public API). This is the only
    // way users get a link that actually downloads.
    // The public API base that users can reach. `CONFIG.urls.site_url` is the
    // main site (hub → 4003) — NOT the scraper API domain. Use an explicit
    // env override (TERABOX_PUBLIC_BASE_URL), falling back to the scraper's
    // own public domain via site_url only when it's clearly the API host.
    let public_base = std::env::var("TERABOX_PUBLIC_BASE_URL").unwrap_or_else(|_| {
        let site = crate::config::CONFIG.urls.site_url.clone();
        if site.contains("scraper") || site.contains("api") {
            site
        } else {
            // default to the scraper API subdomain (this API's public host)
            "https://api.asepharyana.my.id".to_string()
        }
    });
    let proxy_dl = format!("{}/proxy/terabox?surl={}", public_base, surl);
    if let Some(download_url) = data.get("download_link").and_then(|v| v.as_str()) {
        let size_bytes = data.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);

        // PRIMARY = the cookie-authenticated public proxy. The raw terabox
        // dlink is only a secondary entry — it 403s without cookies, so it
        // must NOT be the first link users see.
        result.media.push(MediaItem {
            url: proxy_dl.clone(),
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: Some(format_filesize(size_bytes)),
            size_bytes: Some(size_bytes),
            frame_width: None,
            frame_height: None,
            note: Some("Direct download via cookie-authenticated proxy (works).".into()),
        });
        result.media.push(MediaItem {
            url: download_url.to_string(),
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: Some(format_filesize(size_bytes)),
            size_bytes: Some(size_bytes),
            frame_width: None,
            frame_height: None,
            note: Some("Raw CDN link (may require a TeraBox session cookie).".into()),
        });

        // Directory: add child file links
        if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
            for f in files {
                if let Some(furl) = f.get("download_link").and_then(|v| v.as_str()) {
                    let fsize = f.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
                    result.media.push(MediaItem {
                        url: furl.to_string(),
                        quality: None,
                        file_type: Some(MediaType::File),
                        extension: None,
                        thumbnail: None,
                        file_size: Some(format_filesize(fsize)),
                        size_bytes: Some(fsize),
                        frame_width: None,
                        frame_height: None,
                        note: f
                            .get("file_name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
    }

    Ok(result)
}

pub async fn fetch_videy(url: &str) -> Result<DownloadResult, ScrapingError> {
    let id = Url::parse(url)
        .map_err(|e| ScrapingError::Http(format!("Invalid URL: {}", e)))?
        .query_pairs()
        .find(|(k, _)| k == "id")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| ScrapingError::Http("Invalid URL, missing \"id\" parameter".to_string()))?;

    let file_type = if id.len() == 9 && id.as_bytes().get(8) == Some(&b'2') {
        ".mov"
    } else {
        ".mp4"
    };

    let direct_url = format!("https://cdn.videy.co/{}{}", id, file_type);

    let mut result = DownloadResult::success(Some(format!("Videy video {}", id)));
    result.provider = Some("videy".to_string());

    result.media.push(MediaItem {
        url: direct_url,
        quality: None,
        file_type: Some(MediaType::Video),
        extension: Some(file_type.trim_start_matches('.').to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: None,
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Parse file ID from krakenfiles.com URL
    let file_id = regex::Regex::new(r"krakenfiles\.com/v/([A-Za-z0-9_-]+)")
        .unwrap()
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| ScrapingError::Http("Invalid KrakenFiles URL".to_string()))?;

    let client = http_client();
    let resp = client
        .client()
        .post(format!("https://krakenfiles.com/v/{}", file_id))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("KrakenFiles fetch failed: {}", e)))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| ScrapingError::Http(format!("KrakenFiles JSON parse failed: {}", e)))?;

    let mut result = DownloadResult::success(
        resp.get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    result.provider = Some("krakenfiles".to_string());

    result.media.push(MediaItem {
        url: resp
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        quality: None,
        file_type: Some(MediaType::File),
        extension: resp
            .get("ext")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: resp.get("size_b").and_then(|v| v.as_u64()),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

/// MEGA.nz — requires AES-128-CBC decryption of the file attributes.
/// Ported from Shirokami's `mega.js` crypto logic.
pub async fn fetch_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
    let url_fixed = url.replace('#', "%23");
    let _parts: Vec<&str> = url_fixed.splitn(2, "#").collect();
    // Actually need to handle the # separator differently
    let cleaned = url.replace("#", "%23");
    let decoded_url = urlencoding::decode(&cleaned)
        .unwrap_or(Cow::Borrowed(&cleaned))
        .into_owned();

    let file_id = regex::Regex::new(r"file/([^#]+)")
        .unwrap()
        .captures(&decoded_url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let file_id = file_id.ok_or_else(|| ScrapingError::Http("Not found".to_string()))?;

    let parts: Vec<&str> = decoded_url.splitn(2, '#').collect();
    let file_key = parts.get(1).copied().unwrap_or("");

    if file_key.is_empty() || file_key.len() != 43 {
        return Ok(DownloadResult::error(
            if file_key.is_empty() {
                "File key tidak ditemukan"
            } else if file_key.len() < 43 {
                "Not enough character"
            } else {
                "Too many character"
            }
            .to_string(),
        ));
    }

    let client = http_client();
    let headers = {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        h.insert("origin", HeaderValue::from_static("https://mega.nz"));
        h.insert("referer", HeaderValue::from_static("https://mega.nz"));
        h
    };

    let payload = serde_json::json!([{ "a": "g", "g": 1, "p": file_id }]);

    // Retry loop (3 attempts)
    let mut data = None;
    for attempt in 0..3 {
        match client
            .client()
            .post("https://g.api.mega.co.nz/cs")
            .headers(headers.clone())
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                data = Some(
                    resp.json::<Vec<serde_json::Value>>()
                        .await
                        .unwrap_or_default(),
                );
                break;
            }
            Err(_) if attempt < 2 => continue,
            Err(e) => {
                return Ok(DownloadResult::error(e.to_string()));
            }
        }
    }

    let data = data.unwrap_or_default();
    let first = data.first().unwrap_or(&serde_json::Value::Null);

    let mut attrs = None;
    if let Some(at) = first.get("at") {
        if let Some(dec) = decrypt_mega_attr(at.as_str().unwrap_or(""), file_key) {
            attrs = Some(dec);
        }
    }

    let mut result = DownloadResult::success(
        attrs
            .as_ref()
            .and_then(|a| a.get("n").and_then(|v| v.as_str()).map(|s| s.to_string())),
    );
    result.provider = Some("mega".to_string());
    result.title = attrs
        .as_ref()
        .and_then(|a| a.get("n").and_then(|v| v.as_str()).map(|s| s.to_string()));

    result.media.push(MediaItem {
        url: first
            .get("g")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        quality: None,
        file_type: Some(MediaType::File),
        extension: None,
        thumbnail: None,
        file_size: first.get("s").and_then(|v| v.as_u64()).map(format_filesize),
        size_bytes: first.get("s").and_then(|v| v.as_u64()),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

/// MEGA attribute decryption — AES-128-CBC with the derived key.
fn decrypt_mega_attr(enc: &str, file_key: &str) -> Option<serde_json::Value> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

    let fixed_key = fix_base64_key(file_key);
    let key_buffer = BASE64.decode(&fixed_key).ok()?;
    if key_buffer.len() < 32 {
        return None;
    }

    // Derive 16-byte key from 32-byte file key
    let key_ints: Vec<u32> = (0..8)
        .map(|i| {
            u32::from_le_bytes([
                key_buffer[i * 4],
                key_buffer[i * 4 + 1],
                key_buffer[i * 4 + 2],
                key_buffer[i * 4 + 3],
            ])
        })
        .collect();

    let key_out: [u8; 16] = {
        let ints = [
            key_ints[0] ^ key_ints[4],
            key_ints[1] ^ key_ints[5],
            key_ints[2] ^ key_ints[6],
            key_ints[3] ^ key_ints[7],
        ];
        let mut buf = [0u8; 16];
        for (i, &val) in ints.iter().enumerate() {
            buf[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
        }
        buf
    };

    let iv = [0u8; 16];
    decrypt_aes_128_cbc(&key_out, &iv, enc)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
}

fn fix_base64_key(s: &str) -> String {
    let rem = s.len() % 4;
    if rem > 0 {
        format!("{}{}", s, "=".repeat(4 - rem))
    } else {
        s.to_string()
    }
}

fn decrypt_aes_128_cbc(key: &[u8], _iv: &[u8], enc: &str) -> Option<String> {
    use aes::Aes128;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

    let data = BASE64.decode(enc).ok()?;
    let cipher = Aes128::new_from_slice(key).ok()?;

    let mut result = Vec::new();
    for chunk in data.chunks_exact(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        if let Ok(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cipher.decrypt_block((&mut block).into())
        })) {
            result.extend_from_slice(&block);
        } else {
            return None;
        }
    }

    let s = String::from_utf8_lossy(&result).into_owned();
    let s = s.trim_start_matches("MEGA");
    Some(s.to_string())
}

/// Danbooru — returns direct image URL from post
pub async fn fetch_danbooru(url: &str) -> Result<DownloadResult, ScrapingError> {
    let post_id = regex::Regex::new(r"danbooru\.donmai\.us/posts/(\d+)$")
        .unwrap()
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| ScrapingError::Http("Invalid URL".to_string()))?;

    let client = http_client();
    let resp = client
        .client()
        .get(format!("https://danbooru.donmai.us/posts/{}.json", post_id))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("Danbooru fetch failed: {}", e)))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| ScrapingError::Http(format!("Danbooru JSON parse failed: {}", e)))?;

    let file_url = resp["file_url"]
        .as_str()
        .or_else(|| resp["large_file_url"].as_str())
        .ok_or_else(|| ScrapingError::Http("No file URL found".to_string()))?;

    let mut result = DownloadResult::success(resp["tag_string"].as_str().map(|s| s.to_string()));
    result.provider = Some("danbooru".to_string());

    let full_url = if file_url.starts_with("http") {
        file_url.to_string()
    } else {
        format!("https://danbooru.donmai.us{}", file_url)
    };

    let ext = full_url.rsplit('.').next().unwrap_or("jpg").to_string();
    let mut file_type = MediaType::Image;
    if ext == "mp4" || ext == "webm" || ext == "gif" {
        file_type = if ext == "gif" {
            MediaType::Image
        } else {
            MediaType::Video
        };
    }

    result.media.push(MediaItem {
        url: full_url,
        quality: None,
        file_type: Some(file_type),
        extension: Some(ext),
        thumbnail: resp["preview_file_url"].as_str().map(|s| s.to_string()),
        file_size: resp["file_size"].as_str().map(|s| s.to_string()),
        size_bytes: None,
        frame_width: resp["image_width"].as_str().map(|s| s.to_string()),
        frame_height: resp["image_height"].as_str().map(|s| s.to_string()),
        note: Some(resp["md5"].as_str().unwrap_or("").to_string()),
    });

    Ok(result)
}

// ============================================================================
// Helper functions
// ============================================================================

fn format_filesize(bytes: u64) -> String {
    const UNITS: &[&str] = &["bytes", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, UNITS[unit])
}

// ============================================================================
// Google Drive direct download link — extracts the fileId from the share URL
// and builds a download URL via Google's uc handler.
// ============================================================================

pub async fn fetch_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
    let mut result = DownloadResult::error("Invalid Google Drive URL");
    let id_re = regex::Regex::new(r"[?&]id=([a-zA-Z0-9_-]+)").unwrap();
    if let Some(caps) = id_re.captures(url) {
        let file_id = caps.get(1).unwrap().as_str();
        let download_url = format!("https://drive.google.com/uc?id={}&export=download", file_id);
        result = DownloadResult::success(None);
        result.provider = Some("gdrive".to_string());
        result.media.push(MediaItem {
            url: download_url,
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note: None,
        });
    }
    Ok(result)
}

// ============================================================================
// MediaFire direct download link — parses the MediaFire page for the
// direct download anchor.
// ============================================================================

pub async fn fetch_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let html = client
        .client()
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("accept", "text/html,application/xhtml+xml")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("MediaFire fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("MediaFire response read failed: {}", e)))?;

    let document = scraper::Html::parse_document(&html);
    let dl_sel = scraper::Selector::parse(r#"a#downloadButton"#).unwrap();
    let download_url = document
        .select(&dl_sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.to_string());

    let title = {
        let title_sel = scraper::Selector::parse("h1").unwrap();
        document
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<String>())
    };

    Ok(DownloadResult {
        title,
        status: crate::domain::entity::downloader::DownloadStatus::Success,
        media: download_url
            .map(|u| {
                vec![MediaItem {
                    url: u,
                    quality: None,
                    file_type: Some(MediaType::File),
                    extension: None,
                    thumbnail: None,
                    file_size: None,
                    size_bytes: None,
                    frame_width: None,
                    frame_height: None,
                    note: None,
                }]
            })
            .unwrap_or_default(),
        provider: Some("mediafire".to_string()),
        author: None,
        thumbnail: None,
        description: None,
        duration: None,
        message: None,
    })
}

// ============================================================================
// Threads media extraction — reuses the SnapSave flow (Instagram backend
// already handles Threads via the same snapsave.app endpoint).
// ============================================================================

pub async fn fetch_threads(
    url: &str,
    _cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    // Threads URLs go through SnapSave (same engine as Instagram)
    let mut result = fetch_snapsave(url).await?;
    if result.provider.is_none() {
        result.provider = Some("threads".to_string());
    }
    Ok(result)
}
