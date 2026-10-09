//! Media downloader repository — ports HTTP API results to domain types.
//!
//! Each method corresponds to a downloader for one platform. The provider
//! implementations live in sibling modules grouped by the extraction strategy
//! they share; this file is the public façade and the all-in-one dispatcher.

pub mod filehosts;
pub mod misc;
pub mod patterns;
pub mod pinterest;
pub mod shared;
pub mod social;
pub mod tiktok;
pub mod twitter;
pub mod youtube;
pub mod ytdl;

use crate::domain::entity::downloader::DownloadResult;
use crate::domain::entity::platform::Platform;
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

use social::fetch_snapsave;

pub struct DownloaderRepository;

impl DownloaderRepository {
    pub fn new() -> Self {
        Self
    }

    /// Shared client getter — uses the global 30s-timeout client.
    #[allow(dead_code)]
    fn client(&self) -> &'static crate::infrastructure::utils::http_client::HttpClient {
        http_client()
    }

    /// All-in-one: auto-detect platform from URL and delegate to specialized
    /// downloader. Falls back to `downr.org` universal scraper.
    pub async fn download_all_in_one(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        let platform = Platform::detect(url);
        match platform {
            // Instagram and Facebook share the SnapSave backend: try the
            // Instagram entrypoint first, then the Facebook one.
            Platform::Instagram | Platform::Facebook => {
                match Self::download_snapsave(url, cookies).await {
                    ok @ Ok(_) => ok,
                    Err(_) => Self::download_snapsave(url, cookies).await,
                }
            }
            Platform::TikTok => Self::download_tiktok(url, cookies).await,
            Platform::Videy => Self::download_videy(url).await,
            // YouTube: try the video downloader, fall back to MP3 extraction
            // so a blocked yt-dlp path still yields audio.
            Platform::YouTube => match Self::download_youtube(url, "720").await {
                ok @ Ok(_) => ok,
                Err(_) => Self::download_youtube_mp3(url).await,
            },
            Platform::Spotify => Self::download_spotify(url, cookies).await,
            Platform::Twitter => Self::download_twitter(url, cookies).await,
            Platform::Pinterest => Self::download_pinterest(url).await,
            Platform::Reddit => Self::download_reddit(url).await,
            Platform::Mega => Self::download_mega(url).await,
            Platform::TeraBox => Self::download_terabox(url).await,
            Platform::GoogleDrive => Self::download_gdrive(url).await,
            Platform::MediaFire => Self::download_mediafire(url).await,
            Platform::PixelDrain => Self::download_pixeldrain(url).await,
            Platform::Threads => Self::download_threads(url, cookies).await,
            Platform::DoodStream => Self::download_doodstream(url).await,
            Platform::KrakenFiles => Self::download_krakenfiles(url).await,
            Platform::Danbooru => Self::download_danbooru(url).await,
            Platform::SoundCloud => Self::download_soundcloud(url).await,
            Platform::Dailymotion => Self::download_dailymotion(url).await,
            Platform::Streamable => Self::download_streamable(url).await,
            Platform::Bilibili => Self::download_bilibili(url).await,
            Platform::Unknown => misc::fetch_all_in_one(url).await,
        }
    }

    /// Shared SnapSave backend for Instagram / Facebook / Threads.
    ///
    /// Both platforms are served by the same snapsave.app extraction, so the
    /// two public entrypoints are thin aliases over this one implementation.
    async fn download_snapsave(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        fetch_snapsave(url).await
    }

    /// Instagram / Facebook via SnapSave.
    /// Cookies param accepted for API consistency but not required (SnapSave
    /// fetches its own).
    pub async fn download_instagram(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        Self::download_snapsave(url, cookies).await
    }

    /// Facebook via SnapSave.
    pub async fn download_facebook(
        url: &str,
        cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        Self::download_snapsave(url, cookies).await
    }

    /// TikTok via tikwm.com.
    pub async fn download_tiktok(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        tiktok::fetch_tiktok(url).await
    }

    /// YouTube video via savetube.media.
    pub async fn download_youtube(
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        youtube::fetch_youtube_mp4(url, quality).await
    }

    /// YouTube video + audio merged server-side into a single MP4.
    pub async fn fetch_youtube_merge(
        url: &str,
        quality: &str,
    ) -> Result<DownloadResult, ScrapingError> {
        youtube::fetch_youtube_merge_free(url, quality).await
    }

    /// YouTube to MP3 via ydlp.yard.id.
    pub async fn download_youtube_mp3(url: &str) -> Result<DownloadResult, ScrapingError> {
        youtube::fetch_youtube_mp3(url).await
    }

    /// Spotify via Spotify API (requires api_key).
    pub async fn download_spotify(
        url: &str,
        api_key: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        let _key = match api_key {
            Some(k) => k.to_string(),
            None => std::env::var("SPOTIFY_API_KEY").unwrap_or_default(),
        };
        misc::fetch_spotify(url).await
    }

    /// Twitter/X media via api.lrm.tube.
    pub async fn download_twitter(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        twitter::fetch_twitter(url).await
    }

    /// Pinterest media via PinterestDownloader.
    pub async fn download_pinterest(url: &str) -> Result<DownloadResult, ScrapingError> {
        pinterest::fetch_pinterest(url).await
    }

    /// MEGA.nz file link resolution.
    pub async fn download_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_mega(url).await
    }

    /// TeraBox / TeraFile direct link extraction.
    pub async fn download_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_terabox(url).await
    }

    /// Google Drive direct download link.
    pub async fn download_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_gdrive(url).await
    }

    /// MediaFire direct download link.
    pub async fn download_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_mediafire(url).await
    }

    /// PixelDrain file direct link.
    pub async fn download_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_pixeldrain(url).await
    }

    /// Meta Threads media extraction.
    pub async fn download_threads(
        url: &str,
        _cookies: Option<&str>,
    ) -> Result<DownloadResult, ScrapingError> {
        misc::fetch_threads(url, _cookies).await
    }

    /// DoodStream direct link extraction.
    pub async fn download_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_doodstream(url).await
    }

    /// KrakenFiles direct download link.
    pub async fn download_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
        filehosts::fetch_krakenfiles(url).await
    }

    /// Danbooru image post extraction.
    pub async fn download_danbooru(url: &str) -> Result<DownloadResult, ScrapingError> {
        misc::fetch_danbooru(url).await
    }

    /// SoundCloud track via ydlp converter.
    pub async fn download_soundcloud(url: &str) -> Result<DownloadResult, ScrapingError> {
        ytdl::fetch_soundcloud(url).await
    }

    /// Dailymotion video via yt-dlp.
    pub async fn download_dailymotion(url: &str) -> Result<DownloadResult, ScrapingError> {
        ytdl::fetch_dailymotion(url).await
    }

    /// Reddit video via yt-dlp.
    pub async fn download_reddit(url: &str) -> Result<DownloadResult, ScrapingError> {
        ytdl::fetch_reddit(url).await
    }

    /// Streamable video via yt-dlp.
    pub async fn download_streamable(url: &str) -> Result<DownloadResult, ScrapingError> {
        ytdl::fetch_streamable(url).await
    }

    /// Videy video (direct CDN link build; ported from Shirokami-API videy.js).
    pub async fn download_videy(url: &str) -> Result<DownloadResult, ScrapingError> {
        misc::fetch_videy(url).await
    }

    /// Bilibili video via b23.tv short link expansion.
    pub async fn download_bilibili(url: &str) -> Result<DownloadResult, ScrapingError> {
        ytdl::fetch_bilibili(url).await
    }
}
