//! //! Assorted downloaders: Spotify, Videy, Danbooru, Threads, and the universal
//! downr.org fallback.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;
use url::Url;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

use super::shared::run_ytdlp_json;
use super::social::fetch_snapsave;

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
