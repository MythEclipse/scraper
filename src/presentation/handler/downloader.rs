//! HTTP handlers for media download endpoints.
//!
//! Each handler wraps an application-layer use case, parses request params,
//! and returns a JSON response. Platform is auto-detected from the URL
//! via the all-in-one dispatcher.
use axum::routing::get;
use axum::Router;

use axum::extract::Query;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use utoipa::IntoParams;

use crate::application::downloader as use_cases;
use crate::domain::error::AppError;
use crate::presentation::dto::downloader::DownloadResponse;

/// Request params for downloader endpoints.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DownloadParams {
    pub url: String,
    pub cookies: Option<String>,
    pub api_key: Option<String>,
    pub quality: Option<String>,
    /// When `true`, YouTube downloads are merged server-side (video + audio
    /// into a single MP4) instead of returning separate video/audio URLs.
    pub merge: Option<String>,
}

// ========================================================================
// Handlers
// ========================================================================

/// All-in-one auto-detect downloader.
/// `/download?url=...`
#[utoipa::path(
    get,
    path = "/download",
    tag = "download",
    operation_id = "dl_download",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let merge = params
        .merge
        .as_deref()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let result = if merge {
        // All-in-one with merge=true → force merge for YouTube URLs.
        if crate::application::downloader::detect_platform(&params.url) == "youtube" {
            use_cases::download_youtube_merge(
                &params.url,
                params.quality.as_deref().unwrap_or("720"),
            )
            .await?
        } else {
            use_cases::download_all_in_one(&params.url, params.cookies.as_deref()).await?
        }
    } else {
        use_cases::download_all_in_one(&params.url, params.cookies.as_deref()).await?
    };
    Ok(Json(DownloadResponse::ok(result)))
}

/// Detect platform from URL.
/// `/detect-platform?url=...`
#[utoipa::path(
    get,
    path = "/detect-platform",
    tag = "download",
    operation_id = "dl_detect_platform_handler",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn detect_platform_handler(
    Query(params): Query<DownloadParams>,
) -> Result<Json<Value>, AppError> {
    let platform = use_cases::detect_platform(&params.url);
    Ok(Json(serde_json::json!({
        "platform": platform,
        "url": params.url,
    })))
}

/// Download Instagram media.
/// `/download/instagram?url=...`
#[utoipa::path(
    get,
    path = "/download/instagram",
    tag = "download",
    operation_id = "dl_download_instagram",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_instagram(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_instagram(&params.url, params.cookies.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Facebook video.
/// `/download/facebook?url=...`
#[utoipa::path(
    get,
    path = "/download/facebook",
    tag = "download",
    operation_id = "dl_download_facebook",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_facebook(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_facebook(&params.url, params.cookies.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download TikTok video.
/// `/download/tiktok?url=...`
#[utoipa::path(
    get,
    path = "/download/tiktok",
    tag = "download",
    operation_id = "dl_download_tiktok",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_tiktok(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_tiktok(&params.url, params.cookies.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download YouTube video.
/// `/download/youtube?url=...`
#[utoipa::path(
    get,
    path = "/download/youtube",
    tag = "download",
    operation_id = "dl_download_youtube",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_youtube(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let quality = params.quality.as_deref().unwrap_or("720");
    let merge = params
        .merge
        .as_deref()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let result = if merge {
        use_cases::download_youtube_merge(&params.url, quality).await?
    } else {
        use_cases::download_youtube(&params.url, quality).await?
    };
    Ok(Json(DownloadResponse::ok(result)))
}

/// Convert YouTube to MP3.
/// `/download/youtube/mp3?url=...`
#[utoipa::path(
    get,
    path = "/download/youtube/mp3",
    tag = "download",
    operation_id = "dl_download_youtube_mp3",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_youtube_mp3(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_youtube_mp3(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Spotify track.
/// `/download/spotify?url=...`
#[utoipa::path(
    get,
    path = "/download/spotify",
    tag = "download",
    operation_id = "dl_download_spotify",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_spotify(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_spotify(&params.url, params.api_key.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Twitter/X media.
/// `/download/twitter?url=...`
#[utoipa::path(
    get,
    path = "/download/twitter",
    tag = "download",
    operation_id = "dl_download_twitter",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_twitter(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_twitter(&params.url, params.cookies.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Pinterest pin.
/// `/download/pinterest?url=...`
#[utoipa::path(
    get,
    path = "/download/pinterest",
    tag = "download",
    operation_id = "dl_download_pinterest",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_pinterest(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_pinterest(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from Mega.
/// `/download/mega?url=...`
#[utoipa::path(
    get,
    path = "/download/mega",
    tag = "download",
    operation_id = "dl_download_mega",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_mega(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_mega(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from Terabox.
/// `/download/terabox?url=...`
#[utoipa::path(
    get,
    path = "/download/terabox",
    tag = "download",
    operation_id = "dl_download_terabox",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_terabox(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_terabox(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Stream a TeraBox file through this API.
/// `/proxy/terabox?surl=...`
///
/// The resolver's `/download` endpoint replays the full 302 chain WITH
/// cookies and streams real bytes — this handler forwards those bytes to the
/// caller so the returned URL is a public, downloadable link (not a 403
/// dlink, not a private 127.0.0.1 URL).
#[utoipa::path(
    get,
    path = "/proxy/terabox",
    tag = "download",
    operation_id = "dl_proxy_terabox",
    params(
        ("surl" = String, Query, description = "TeraBox share short URL ID"),
        ("filename" = Option<String>, Query, description = "Download file name hint"),
    ),
    responses(
        (status = 200, description = "File stream"),
        (status = 400, description = "Missing surl"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn proxy_terabox(
    Query(params): Query<TeraboxProxyParams>,
) -> Result<axum::response::Response, AppError> {
    let surl = params.surl;
    if surl.is_empty() {
        return Err(AppError::BadRequest("missing surl parameter".into()));
    }

    let resolver_url = std::env::var("TERABOX_RESOLVER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4092".to_string());
    let fetch_url = format!("{}/download?surl={}", resolver_url, surl);

    // Ask the resolver for the redirect chain (it follows 302s internally with
    // cookies) — but we only want the final CDN stream URL. The resolver
    // already streams; simplest reliable path is to just proxy the resolver's
    // response body as-is.
    let client = crate::infrastructure::utils::http_client::http_client();

    let resp = client
        .client()
        .get(&fetch_url)
        .header("User-Agent", "Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| AppError::ScraperError(format!("terabox proxy fetch failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ScraperError(format!(
            "terabox proxy returned HTTP {status}: {body}"
        )));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let content_disposition = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let fname = params
                .filename
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "terabox-download.bin".to_string());
            format!("attachment; filename=\"{fname}\"")
        });

    let stream = resp.bytes_stream();

    axum::response::Response::builder()
        .header("Content-Type", content_type)
        .header("Content-Disposition", content_disposition)
        .header("X-Accel-Buffering", "no")
        .header("Cache-Control", "no-store")
        .body(axum::body::Body::from_stream(stream))
        .map_err(|e| AppError::Internal(format!("build stream response: {e}")))
}

/// Params for the TeraBox streaming proxy.
#[derive(Debug, Deserialize, IntoParams)]
pub struct TeraboxProxyParams {
    pub surl: String,
    pub filename: Option<String>,
}

/// Download from Google Drive.
/// `/download/gdrive?url=...`
#[utoipa::path(
    get,
    path = "/download/gdrive",
    tag = "download",
    operation_id = "dl_download_gdrive",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_gdrive(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_gdrive(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from MediaFire.
/// `/download/mediafire?url=...`
#[utoipa::path(
    get,
    path = "/download/mediafire",
    tag = "download",
    operation_id = "dl_download_mediafire",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_mediafire(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_mediafire(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from Pixeldrain.
/// `/download/pixeldrain?url=...`
#[utoipa::path(
    get,
    path = "/download/pixeldrain",
    tag = "download",
    operation_id = "dl_download_pixeldrain",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_pixeldrain(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_pixeldrain(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Threads media.
/// `/download/threads?url=...`
#[utoipa::path(
    get,
    path = "/download/threads",
    tag = "download",
    operation_id = "dl_download_threads",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_threads(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_threads(&params.url, params.cookies.as_deref()).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from DoodStream.
/// `/download/doodstream?url=...`
#[utoipa::path(
    get,
    path = "/download/doodstream",
    tag = "download",
    operation_id = "dl_download_doodstream",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_doodstream(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_doodstream(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from KrakenFiles.
/// `/download/krakenfiles?url=...`
#[utoipa::path(
    get,
    path = "/download/krakenfiles",
    tag = "download",
    operation_id = "dl_download_krakenfiles",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_krakenfiles(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_krakenfiles(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download from Danbooru.
/// `/download/danbooru?url=...`
#[utoipa::path(
    get,
    path = "/download/danbooru",
    tag = "download",
    operation_id = "dl_download_danbooru",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_danbooru(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_danbooru(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download SoundCloud track.
/// `/download/soundcloud?url=...`
#[utoipa::path(
    get,
    path = "/download/soundcloud",
    tag = "download",
    operation_id = "dl_download_soundcloud",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_soundcloud(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_soundcloud(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Dailymotion video.
/// `/download/dailymotion?url=...`
#[utoipa::path(
    get,
    path = "/download/dailymotion",
    tag = "download",
    operation_id = "dl_download_dailymotion",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_dailymotion(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_dailymotion(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Reddit media.
/// `/download/reddit?url=...`
#[utoipa::path(
    get,
    path = "/download/reddit",
    tag = "download",
    operation_id = "dl_download_reddit",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_reddit(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_reddit(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Streamable video.
/// `/download/streamable?url=...`
#[utoipa::path(
    get,
    path = "/download/streamable",
    tag = "download",
    operation_id = "dl_download_streamable",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_streamable(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_streamable(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Videy video.
/// `/download/videy?url=...`
#[utoipa::path(
    get,
    path = "/download/videy",
    tag = "download",
    operation_id = "dl_download_videy",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_videy(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_videy(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

/// Download Bilibili video.
/// `/download/bilibili?url=...`
#[utoipa::path(
    get,
    path = "/download/bilibili",
    tag = "download",
    operation_id = "dl_download_bilibili",
    params(DownloadParams),
    responses(
        (status = 200, description = "Download result", body = DownloadResponse),
        (status = 400, description = "Bad URL or platform"),
        (status = 502, description = "Upstream error"),
    )
)]
pub async fn download_bilibili(
    Query(params): Query<DownloadParams>,
) -> Result<Json<DownloadResponse>, AppError> {
    let result = use_cases::download_bilibili(&params.url).await?;
    Ok(Json(DownloadResponse::ok(result)))
}

// ========================================================================
// Merged-file static serving
// ========================================================================

/// Serve a merged YouTube MP4 produced by `/download/youtube?merge=true`.
/// Path is sanitised: only a plain filename (no `/`, `..`) inside the
/// `yt_merge` uploads dir is allowed.
///
/// `/file/yt_merge/{filename}`
#[utoipa::path(
    get,
    path = "/file/yt_merge/{filename}",
    tag = "download",
    operation_id = "dl_serve_merged_file",
    params(
        ("filename" = String, Path, description = "Merged MP4 file name")
    ),
    responses(
        (status = 200, description = "Merged MP4 file", content_type = "video/mp4"),
        (status = 404, description = "File not found or invalid name")
    )
)]
pub async fn serve_merged_file(
    axum::extract::Path(filename): axum::extract::Path<String>,
) -> Result<axum::response::Response, AppError> {
    // Reject any path traversal or nested segments.
    if filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.is_empty()
    {
        return Err(AppError::NotFound(format!("invalid file name: {filename}")));
    }

    let dir = std::path::PathBuf::from("/var/lib/scraper/uploads/yt_merge");
    let path = dir.join(&filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::NotFound(format!("file not found: {filename}")))?;

    let content_type = if filename.ends_with(".mp4") {
        "video/mp4"
    } else if filename.ends_with(".webm") {
        "video/webm"
    } else {
        "application/octet-stream"
    };

    axum::response::Response::builder()
        .header("Content-Type", content_type)
        .header("Content-Length", bytes.len().to_string())
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("build response: {e}")))
}

/// Downloader routes.
///
/// These span three mount points (`/download`, `/proxy`, `/file`), so they
/// are merged as one flat router rather than nested under a single prefix.
pub fn router() -> Router {
    Router::new()
        .route("/download", get(download))
        .route("/download/detect", get(detect_platform_handler))
        .route("/download/instagram", get(download_instagram))
        .route("/download/facebook", get(download_facebook))
        .route("/download/tiktok", get(download_tiktok))
        .route("/download/youtube", get(download_youtube))
        .route("/download/youtube/mp3", get(download_youtube_mp3))
        .route("/file/yt_merge/{filename}", get(serve_merged_file))
        .route("/download/spotify", get(download_spotify))
        .route("/download/twitter", get(download_twitter))
        .route("/download/pinterest", get(download_pinterest))
        .route("/download/mega", get(download_mega))
        .route("/download/terabox", get(download_terabox))
        .route("/proxy/terabox", get(proxy_terabox))
        .route("/download/gdrive", get(download_gdrive))
        .route("/download/mediafire", get(download_mediafire))
        .route("/download/pixeldrain", get(download_pixeldrain))
        .route("/download/threads", get(download_threads))
        .route("/download/dood", get(download_doodstream))
        .route("/download/kraken", get(download_krakenfiles))
        .route("/download/danbooru", get(download_danbooru))
        .route("/download/soundcloud", get(download_soundcloud))
        .route("/download/dailymotion", get(download_dailymotion))
        .route("/download/reddit", get(download_reddit))
        .route("/download/streamable", get(download_streamable))
        .route("/download/videy", get(download_videy))
        .route("/download/bilibili", get(download_bilibili))
}
