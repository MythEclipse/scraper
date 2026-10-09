//! Application-layer use cases for media downloading.
//!
//! Thin coordinators that delegate to `DownloaderRepository` and return
//! `DownloadResult` domain entities. Each use case corresponds to one
//! downloader endpoint defined in the spec (Shirokami-API reference).

use crate::domain::entity::downloader::DownloadResult;
use crate::domain::entity::platform::Platform;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::DownloaderRepository;

/// Use case: unified dispatcher — auto-detects platform from URL pattern
/// and delegates to the appropriate downloader.
pub async fn download_all_in_one(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_all_in_one(url, cookies).await
}

pub async fn download_instagram(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_instagram(url, cookies).await
}

pub async fn download_facebook(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_facebook(url, cookies).await
}

pub async fn download_tiktok(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_tiktok(url, cookies).await
}

pub async fn download_youtube(url: &str, quality: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_youtube(url, quality).await
}

pub async fn download_youtube_merge(
    url: &str,
    quality: &str,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::fetch_youtube_merge(url, quality).await
}

pub async fn download_youtube_mp3(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_youtube_mp3(url).await
}

pub async fn download_spotify(
    url: &str,
    api_key: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_spotify(url, api_key).await
}

pub async fn download_twitter(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_twitter(url, cookies).await
}

pub async fn download_pinterest(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_pinterest(url).await
}

pub async fn download_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_mega(url).await
}

pub async fn download_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_terabox(url).await
}

pub async fn download_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_gdrive(url).await
}

pub async fn download_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_mediafire(url).await
}

pub async fn download_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_pixeldrain(url).await
}

pub async fn download_threads(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_threads(url, cookies).await
}

pub async fn download_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_doodstream(url).await
}

pub async fn download_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_krakenfiles(url).await
}

pub async fn download_danbooru(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_danbooru(url).await
}

pub async fn download_soundcloud(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_soundcloud(url).await
}

pub async fn download_dailymotion(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_dailymotion(url).await
}

pub async fn download_reddit(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_reddit(url).await
}

pub async fn download_streamable(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_streamable(url).await
}

pub async fn download_videy(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_videy(url).await
}

pub async fn download_bilibili(url: &str) -> Result<DownloadResult, ScrapingError> {
    DownloaderRepository::download_bilibili(url).await
}

/// Use case: detect platform from URL pattern.
pub fn detect_platform(url: &str) -> String {
    Platform::detect(url).as_str().to_string()
}
