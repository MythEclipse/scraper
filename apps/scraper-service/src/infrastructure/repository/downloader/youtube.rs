//! //! YouTube downloaders: savetube.media for media URLs, yt-dlp for metadata,
//! and an ffmpeg remux that merges separate video+audio streams server-side.

use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use reqwest::header::USER_AGENT;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;

use super::shared::{format_filesize, run_ytdlp_json};
use std::path::Path;

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
