//! Composition root for the downloader handlers.
//!
//! The handlers call these helpers by module path, so this is where the concrete
//! [`DownloaderRepository`] implementation gets chosen — the application layer
//! only ever sees the port. `detect_platform` is re-exported here too; it is
//! pure domain logic and needs no wiring at all.

use crate::application::downloader::new_use_cases;
use crate::domain::entity::downloader::DownloadResult;
use crate::domain::entity::platform::Platform;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::DownloaderRepository;

pub async fn download_all_in_one(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_all_in_one(url, cookies)
        .await
}
pub async fn download_instagram(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_instagram(url, cookies)
        .await
}
pub async fn download_facebook(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_facebook(url, cookies)
        .await
}
pub async fn download_tiktok(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_tiktok(url, cookies)
        .await
}
pub async fn download_youtube(url: &str, quality: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_youtube(url, quality)
        .await
}
pub async fn download_youtube_merge(
    url: &str,
    quality: &str,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_youtube_merge(url, quality)
        .await
}
pub async fn download_youtube_mp3(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_youtube_mp3(url)
        .await
}
pub async fn download_spotify(
    url: &str,
    api_key: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_spotify(url, api_key)
        .await
}
pub async fn download_twitter(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_twitter(url, cookies)
        .await
}
pub async fn download_pinterest(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_pinterest(url)
        .await
}
pub async fn download_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository).download_mega(url).await
}
pub async fn download_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_terabox(url)
        .await
}
pub async fn download_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_gdrive(url)
        .await
}
pub async fn download_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_mediafire(url)
        .await
}
pub async fn download_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_pixeldrain(url)
        .await
}
pub async fn download_threads(
    url: &str,
    cookies: Option<&str>,
) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_threads(url, cookies)
        .await
}
pub async fn download_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_doodstream(url)
        .await
}
pub async fn download_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_krakenfiles(url)
        .await
}
pub async fn download_danbooru(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_danbooru(url)
        .await
}
pub async fn download_soundcloud(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_soundcloud(url)
        .await
}
pub async fn download_dailymotion(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_dailymotion(url)
        .await
}
pub async fn download_reddit(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_reddit(url)
        .await
}
pub async fn download_streamable(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_streamable(url)
        .await
}
pub async fn download_videy(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_videy(url)
        .await
}
pub async fn download_bilibili(url: &str) -> Result<DownloadResult, ScrapingError> {
    new_use_cases(DownloaderRepository)
        .download_bilibili(url)
        .await
}

/// Detect the platform a URL belongs to. Pure domain logic, so it needs no port
/// and no wiring — it is re-exported from here only because the handlers call it
/// through this module.
pub fn detect_platform(url: &str) -> String {
    Platform::detect(url).as_str().to_string()
}
