//! //! Twitter/X downloaders: the tokenless Syndication API (primary) and twitsave (fallback).

use std::time::Duration;

use reqwest::header::USER_AGENT;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

use super::shared::{playwright_to_download_result, run_playwright_scraper};

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
