//! Instagram / Facebook / Threads downloaders backed by snapsave.app.

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;

use super::misc::fetch_all_in_one;
use super::patterns::INSTAGRAM_URL;
use super::shared::{run_playwright_scraper, run_ytdlp_json, ytdlp_to_download_result};

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
        .map(|re| re.is_match(url))
        .unwrap_or(false);
    let valid_ig =
        url.contains("instagram.com") || url.contains("threads.net") || INSTAGRAM_URL.is_match(url);

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
