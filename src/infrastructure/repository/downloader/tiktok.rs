//! //! TikTok downloaders: embed-page scraping (primary) and douyin.wtf (fallback).

use std::time::Duration;

use reqwest::header::USER_AGENT;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

use super::patterns::{
    TIKTOK_AUTHOR, TIKTOK_GENERIC_VIDEO_ID, TIKTOK_HAS_VIDEO_PATH, TIKTOK_SHORT_PATH, TIKTOK_TITLE,
    TIKTOK_VIDEO_ID,
};
use super::shared::{format_filesize, run_playwright_scraper, run_ytdlp_json};

/// Helper: parse the video-id portion from TikTok/Douyin URL.
fn extract_tiktok_id(url: &str) -> Option<String> {
    if !url.contains("tiktok.com") && !url.contains("douyin.com") {
        return None;
    }
    // Handle short URLs like vm.tiktok.com/ZM8s5qJ6t — resolve redirect first
    if url.contains("vm.tiktok.com") || url.contains("vt.tiktok.com") {
        let re = &*TIKTOK_SHORT_PATH;
        let short_code = re
            .captures(url)
            .and_then(|c| c.get(1))?
            .as_str()
            .to_string();
        return Some(short_code);
    }
    // TikTok URLs contain an 18-20 digit video ID in the path
    let re = &*TIKTOK_VIDEO_ID;
    let caps = re.captures(url)?;
    Some(caps.get(1)?.as_str().to_string())
}

/// Resolve TikTok short URLs (vm/vt.tiktok.com/xxx) to the canonical long URL.
async fn resolve_tiktok_url(url: &str) -> Result<String, ScrapingError> {
    if !url.contains("tiktok.com") {
        return Ok(url.to_string());
    }
    // If it already has /video/<id>, return as-is
    if TIKTOK_HAS_VIDEO_PATH.is_match(url) {
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

/// TikTok via embed-page scraping (primary method).
/// Scrapes `https://www.tiktok.com/embed/v2/{video_id}` HTML and extracts the
/// direct `v16m.tiktokcdn.com` MP4 URL from the `<video data-testid="play-video">` tag.
/// This works server-side (no auth) for active videos while the main site/API are
/// Cloudflare-blocked. Verified: returns a real downloadable MP4 (200/206, video/mp4).
async fn fetch_tiktok_embed(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Resolve short URLs (vm/vt.tiktok.com) to the long form to get a video ID
    let resolved_url = resolve_tiktok_url(url).await?;

    let video_id = &TIKTOK_GENERIC_VIDEO_ID
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
            if let Some(m) = regex::Regex::new(pat).ok().and_then(|r| r.captures(&html)) {
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
        if let Some(m) = TIKTOK_TITLE.captures(&html) {
            let raw = m
                .get(1)
                .map(|s| s.as_str().trim().to_string())
                .unwrap_or_default();
            if !raw.is_empty() && !raw.contains("TikTok") {
                title = Some(raw);
            }
        }
        if let Some(m) = TIKTOK_AUTHOR.captures(&html) {
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
