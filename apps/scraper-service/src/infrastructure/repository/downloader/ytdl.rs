//! //! Platform downloaders implemented via the `yt-dlp` CLI.

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;

use super::shared::{format_filesize, run_ytdlp_json};

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
            let Some(url) = fmt.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            {
                result.media.push(MediaItem {
                    url: url.to_string(),
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
