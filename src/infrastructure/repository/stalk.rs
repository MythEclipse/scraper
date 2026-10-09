//! Infrastructure — Stalk utilities (github, youtube, twitter).
//!
//! Ported from Shirokami-API `scraper/stalk/*.js`.

use async_trait::async_trait;

use crate::domain::error::ScrapingError;
use crate::domain::repository::StalkRepository;

use crate::infrastructure::utils::http_client::http_client;
use regex::Regex;
use reqwest::header::USER_AGENT;
use serde_json::{json, Value};

/// GitHub user stalk via public API.
pub async fn fetch_github_stalk(username: &str) -> Result<Value, String> {
    let url = format!("https://api.github.com/users/{}", username);

    let resp = http_client()
        .client()
        .get(&url)
        .header(USER_AGENT, "Scraper/1.0")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        return Err(format!("GitHub user not found (HTTP {})", status));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse: {}", e))?;

    let pick = |k: &str| -> Value { data.get(k).cloned().unwrap_or(Value::Null) };

    Ok(json!({
        "username": username,
        "login": pick("login"),
        "id": pick("id"),
        "node_id": pick("node_id"),
        "avatar_url": pick("avatar_url"),
        "html_url": pick("html_url"),
        "type": pick("type"),
        "site_admin": pick("site_admin"),
        "name": pick("name"),
        "company": pick("company"),
        "blog": pick("blog"),
        "location": pick("location"),
        "email": pick("email"),
        "hireable": pick("hireable"),
        "bio": pick("bio"),
        "twitter_username": pick("twitter_username"),
        "public_repos": pick("public_repos"),
        "public_gists": pick("public_gists"),
        "followers": pick("followers"),
        "following": pick("following"),
        "created_at": pick("created_at"),
        "updated_at": pick("updated_at"),
    }))
}

/// YouTube channel stalk: parse ytInitialData JSON from channel page.
/// Fetches the `/<handle>/videos` subpage (richGridRenderer with lockupViewModel).
pub async fn fetch_youtube_stalk(username: &str) -> Result<Value, String> {
    let url = format!("https://youtube.com/@{username}/videos?hl=en&gl=US");

    let resp = http_client()
        .client()
        .get(&url)
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .header("Accept-Language", "en-US")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from youtube", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    // Extract var ytInitialData = {...} via brace matching (JSON may contain ";").
    let start = body
        .find("var ytInitialData = ")
        .ok_or_else(|| "ytInitialData not found".to_string())?;
    let brace = body[start..]
        .find('{')
        .ok_or_else(|| "ytInitialData parse".to_string())?
        + start;
    let json_str = extract_balanced_json(&body, brace)?;
    let parsed: Value =
        serde_json::from_str(&json_str).map_err(|e| format!("JSON parse: {}", e))?;

    // Channel metadata from metadata.channelMetadataRenderer
    let mut channel = json!({
        "username": username,
        "subscriberCount": Value::Null,
        "videoCount": Value::Null,
        "avatarUrl": Value::Null,
        "channelUrl": Value::Null,
        "externalId": Value::Null,
        "description": Value::Null,
        "isFamilySafe": Value::Null,
    });

    if let Some(meta) = parsed
        .get("metadata")
        .and_then(|m| m.get("channelMetadataRenderer"))
    {
        let mut set = |dst: &str, src: &str| {
            if let Some(v) = meta.get(src) {
                channel[dst] = v.clone();
            }
        };
        set("description", "description");
        set("externalId", "externalId");
        set("channelUrl", "channelUrl");
        set("isFamilySafe", "isFamilySafe");
    }

    // Subscriber/video count from header.pageHeaderRenderer
    if let Some(header) = parsed
        .get("header")
        .and_then(|h| h.get("pageHeaderRenderer"))
    {
        if let Some(rows) = header
            .pointer("/content/pageHeaderViewModel/metadata/contentMetadataViewModel/metadataRows")
        {
            if let Some(arr) = rows.as_array() {
                for row in arr {
                    if let Some(parts) = row.get("metadataParts").and_then(|p| p.as_array()) {
                        for part in parts {
                            let text = part
                                .pointer("/text/content")
                                .and_then(|t| t.as_str())
                                .unwrap_or("");
                            if text.contains("subscriber") {
                                channel["subscriberCount"] = json!(text.trim());
                            } else if text.contains(" video") || text.ends_with(" videos") {
                                channel["videoCount"] = json!(text.trim());
                            }
                        }
                    }
                }
            }
        }
        // Avatar
        if let Some(sources) = header
            .pointer("/content/pageHeaderViewModel/image/decoratedAvatarViewModel/avatar/avatarViewModel/image/sources")
            .and_then(|s| s.as_array())
        {
            if !sources.is_empty() {
                channel["avatarUrl"] = sources[0].get("url").cloned().unwrap_or(Value::Null);
            }
        }
    }

    // Videos list (up to 5). The `/videos` subpage uses richGridRenderer with
    // lockupViewModel (new) or videoRenderer (legacy); we handle both.
    let mut videos: Vec<Value> = Vec::new();

    if let Some(tabs) = parsed
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
        .and_then(|t| t.as_array())
    {
        for tab in tabs {
            walk_video_grid(tab, &mut videos);
            if videos.len() >= 5 {
                break;
            }
        }
    }

    // Fallback: source's shelfRenderer/horizontalListRenderer approach.
    if videos.is_empty() {
        if let Some(tabs) = parsed
            .pointer("/contents/twoColumnBrowseResultsRenderer/tabs")
            .and_then(|t| t.as_array())
        {
            if let Some(contents) = tabs
                .first()
                .and_then(|t| t.pointer("/tabRenderer/content/sectionListRenderer/contents"))
                .and_then(|c| c.as_array())
            {
                for item in contents {
                    if let Some(section) = item.get("itemSectionRenderer") {
                        if let Some(inner) = section.get("contents").and_then(|c| c.as_array()) {
                            for content in inner {
                                if let Some(shelf) = content.get("shelfRenderer") {
                                    if let Some(items) = shelf
                                        .pointer("/content/horizontalListRenderer/items")
                                        .and_then(|i| i.as_array())
                                    {
                                        for video in items {
                                            if let Some(gv) = video.get("gridVideoRenderer") {
                                                videos.push(video_from_grid_video(gv));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if videos.len() >= 5 {
                        break;
                    }
                }
            }
        }
    }

    Ok(json!({
        "channelMetadata": channel,
        "videoDataList": videos.iter().take(5).cloned().collect::<Vec<_>>(),
    }))
}

/// Extract a balanced JSON object string starting at `open_brace` (index of '{').
fn extract_balanced_json(body: &str, open_brace: usize) -> Result<String, String> {
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut close = None;
    for (i, b) in bytes.iter().enumerate().skip(open_brace) {
        let c = *b as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let close = close.ok_or_else(|| "unbalanced JSON in ytInitialData".to_string())?;
    Ok(body[open_brace..=close].to_string())
}

/// Recursively find `richItemRenderer` entries and convert their content to video objects.
fn walk_video_grid(node: &Value, out: &mut Vec<Value>) {
    match node {
        Value::Object(map) => {
            if let Some(item) = map.get("richItemRenderer") {
                if let Some(content) = item.get("content") {
                    if let Some(v) = content.get("videoRenderer") {
                        out.push(video_from_video_renderer(v));
                    } else if let Some(lv) = content.get("lockupViewModel") {
                        out.push(video_from_lockup(lv));
                    } else if let Some(sh) = content.get("shortsLockupViewModel") {
                        out.push(video_from_shorts(sh));
                    }
                }
            }
            for v in map.values() {
                if out.len() >= 5 {
                    return;
                }
                walk_video_grid(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                if out.len() >= 5 {
                    return;
                }
                walk_video_grid(v, out);
            }
        }
        _ => {}
    }
}

/// Parse a legacy `videoRenderer` into our video shape.
fn video_from_video_renderer(vd: &Value) -> Value {
    let mut v = json!({
        "videoId": vd.get("videoId").cloned().unwrap_or(Value::Null),
        "title": vd.pointer("/title/runs/0/text").cloned().unwrap_or(Value::Null),
        "thumbnail": vd.pointer("/thumbnail/thumbnails/0/url").cloned().unwrap_or(Value::Null),
        "publishedTime": vd.pointer("/publishedTimeText/simpleText").cloned().unwrap_or(Value::Null),
        "viewCount": vd.pointer("/viewCountText/simpleText").cloned().unwrap_or(Value::Null),
        "navigationUrl": Value::Null,
        "duration": Value::Null,
    });
    if let Some(url) = vd.pointer("/navigationEndpoint/commandMetadata/webCommandMetadata/url") {
        v["navigationUrl"] = url.clone();
    }
    if let Some(dur) = vd
        .pointer("/thumbnailOverlays")
        .and_then(|o| o.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|ov| {
                ov.pointer("/thumbnailOverlayTimeStatusRenderer/text/simpleText")
                    .cloned()
            })
        })
    {
        v["duration"] = dur;
    }
    v
}

/// Parse a `lockupViewModel` (newer YouTube renderer) into our video shape.
fn video_from_lockup(lv: &Value) -> Value {
    // videoId from thumbnail URL `i.ytimg.com/vi/<ID>/...`
    let thumb = lv
        .pointer("/contentImage/thumbnailViewModel/image/sources/0/url")
        .and_then(|u| u.as_str())
        .unwrap_or("");
    let video_id = Regex::new(r"vi/([A-Za-z0-9_-]{11})/")
        .ok()
        .and_then(|re| re.captures(thumb))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    let title = lv
        .pointer("/metadata/lockupMetadataViewModel/title/content")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    let mut view_count = Value::Null;
    let mut published = Value::Null;
    if let Some(parts) = lv
        .pointer("/metadata/lockupMetadataViewModel/metadata/contentMetadataViewModel/metadataRows/0/metadataParts")
        .and_then(|p| p.as_array())
    {
        if let Some(p0) = parts.first() {
            view_count = p0
                .pointer("/text/content")
                .cloned()
                .unwrap_or(Value::Null);
        }
        if let Some(p1) = parts.get(1) {
            published = p1
                .pointer("/text/content")
                .cloned()
                .unwrap_or(Value::Null);
        }
    }

    let duration = lv
        .pointer("/contentImage/thumbnailViewModel/image/overlays/0/thumbnailBottomOverlayViewModel/badges/0/thumbnailBadgeViewModel/text")
        .cloned()
        .unwrap_or(Value::Null);

    json!({
        "videoId": json!(video_id),
        "title": json!(title),
        "thumbnail": json!(thumb),
        "publishedTime": published,
        "viewCount": view_count,
        "navigationUrl": Value::Null,
        "duration": duration,
    })
}

/// Parse a `shortsLockupViewModel` (Shorts) into our video shape (best-effort).
fn video_from_shorts(sh: &Value) -> Value {
    let video_id = sh
        .pointer("/overlayMetadata/primaryText/content")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    json!({
        "videoId": json!(video_id),
        "title": Value::Null,
        "thumbnail": Value::Null,
        "publishedTime": Value::Null,
        "viewCount": Value::Null,
        "navigationUrl": Value::Null,
        "duration": Value::Null,
    })
}

/// Parse a legacy `gridVideoRenderer` into our video shape.
fn video_from_grid_video(gv: &Value) -> Value {
    let mut v = json!({
        "videoId": gv.get("videoId").cloned().unwrap_or(Value::Null),
        "title": gv.pointer("/title/simpleText").cloned().unwrap_or(Value::Null),
        "thumbnail": gv.pointer("/thumbnail/thumbnails/0/url").cloned().unwrap_or(Value::Null),
        "publishedTime": gv.pointer("/publishedTimeText/simpleText").cloned().unwrap_or(Value::Null),
        "viewCount": gv.pointer("/viewCountText/simpleText").cloned().unwrap_or(Value::Null),
        "navigationUrl": Value::Null,
        "duration": Value::Null,
    });
    if let Some(nav) = gv.pointer("/navigationEndpoint/commandMetadata/webCommandMetadata/url") {
        v["navigationUrl"] = nav.clone();
    }
    if let Some(dur) = gv
        .pointer("/thumbnailOverlays")
        .and_then(|o| o.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|ov| {
                ov.pointer("/thumbnailOverlayTimeStatusRenderer/text/simpleText")
                    .cloned()
            })
        })
    {
        v["duration"] = dur;
    }
    v
}

// ---------------------------------------------------------------------------
// Twitter — fxtwitter API
// ---------------------------------------------------------------------------

pub async fn fetch_twitter_stalk(username: &str) -> Result<Value, String> {
    let url = format!("https://api.fxtwitter.com/{}", username);
    let resp = http_client()
        .client()
        .get(&url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    let data: Value = resp.json().await.map_err(|e| format!("JSON: {}", e))?;

    // Transform to the source's shape
    let u = &data["user"];
    Ok(json!({
        "message": data.get("message").cloned().unwrap_or(Value::Null),
        "user": {
            "id": u.get("id").cloned().unwrap_or(Value::Null),
            "url": u.get("url").cloned().unwrap_or(Value::Null),
            "screen_name": u.get("screen_name").cloned().unwrap_or(Value::Null),
            "name": u.get("name").cloned().unwrap_or(Value::Null),
            "location": u.get("location").cloned().unwrap_or(Value::Null),
            "description": u.get("description").cloned().unwrap_or(Value::Null),
            "followers": u.get("followers").cloned().unwrap_or(Value::Null),
            "following": u.get("following").cloned().unwrap_or(Value::Null),
            "likes": u.get("likes").cloned().unwrap_or(Value::Null),
            "banner_url": u.get("banner_url").cloned().unwrap_or(Value::Null),
            "avatar_url": u.get("avatar_url").cloned().unwrap_or(Value::Null),
            "joined_at": u.get("joined").cloned().unwrap_or(Value::Null),
            "website": u.get("website").cloned().unwrap_or(Value::Null),
        }
    }))
}

// ============================================================================
// Port implementation — [`StalkRepository`]
// ============================================================================

/// Adapter for the [`StalkRepository`] port. Stateless: the module functions above are
/// already the whole implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct StalkRepositoryImpl;

#[async_trait]
impl StalkRepository for StalkRepositoryImpl {
    async fn github(&self, username: &str) -> Result<Value, ScrapingError> {
        fetch_github_stalk(username)
            .await
            .map_err(ScrapingError::Http)
    }
    async fn youtube(&self, username: &str) -> Result<Value, ScrapingError> {
        fetch_youtube_stalk(username)
            .await
            .map_err(ScrapingError::Http)
    }
    async fn twitter(&self, username: &str) -> Result<Value, ScrapingError> {
        fetch_twitter_stalk(username)
            .await
            .map_err(ScrapingError::Http)
    }
}
