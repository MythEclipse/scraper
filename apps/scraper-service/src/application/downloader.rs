//! Application-layer use cases for media downloading.
//!
//! Delegates to the [`DownloaderRepository`] port, which each infrastructure
//! platform module implements. Every use case returns a [`DownloadResult`]
//! domain entity; `detect_platform` stays here because it is pure domain logic.

use crate::domain::entity::downloader::DownloadResult;
use crate::domain::error::ScrapingError;
use crate::domain::repository::DownloaderRepository as DownloaderPort;

pub struct DownloaderUseCases<R: DownloaderPort> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: DownloaderPort>(repository: R) -> DownloaderUseCases<R> {
    DownloaderUseCases { repository }
}

impl<R: DownloaderPort> DownloaderUseCases<R> {
    pub async fn download_all_in_one(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_all_in_one(url, cookies).await
    }
    pub async fn download_instagram(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_instagram(url, cookies).await
    }
    pub async fn download_facebook(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_facebook(url, cookies).await
    }
    pub async fn download_tiktok(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_tiktok(url, cookies).await
    }
    pub async fn download_youtube(
        &self,
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_youtube(url, quality).await
    }
    pub async fn download_youtube_merge(
        &self,
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_youtube_merge(url, quality).await
    }
    pub async fn download_youtube_mp3(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_youtube_mp3(url).await
    }
    pub async fn download_spotify(
        &self,
        url: &str,
        api_key: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_spotify(url, api_key).await
    }
    pub async fn download_twitter(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_twitter(url, cookies).await
    }
    pub async fn download_pinterest(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_pinterest(url).await
    }
    pub async fn download_mega(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_mega(url).await
    }
    pub async fn download_terabox(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_terabox(url).await
    }
    pub async fn download_gdrive(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_gdrive(url).await
    }
    pub async fn download_mediafire(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_mediafire(url).await
    }
    pub async fn download_pixeldrain(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_pixeldrain(url).await
    }
    pub async fn download_threads(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_threads(url, cookies).await
    }
    pub async fn download_doodstream(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_doodstream(url).await
    }
    pub async fn download_krakenfiles(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_krakenfiles(url).await
    }
    pub async fn download_danbooru(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_danbooru(url).await
    }
    pub async fn download_soundcloud(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_soundcloud(url).await
    }
    pub async fn download_dailymotion(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_dailymotion(url).await
    }
    pub async fn download_reddit(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_reddit(url).await
    }
    pub async fn download_streamable(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_streamable(url).await
    }
    pub async fn download_videy(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_videy(url).await
    }
    pub async fn download_bilibili(&self, url: &str) -> Result<DownloadResult, ScrapingError> {
        self.repository.download_bilibili(url).await
    }
}
