//! Port traits for the three upstream anime/comic sites.
//!
//! Each trait is a *port*: it names what a capability must do, in domain terms,
//! without naming a host, a URL path, or an HTML parser. Infrastructure
//! implements the port; the application layer holds it behind `impl Trait`, so
//! the dependency rule `application → domain ← infrastructure` holds.
//!
//! Ports cover a whole fetch-and-parse operation rather than a bare HTTP call,
//! which is what lets the use cases stop importing parsers.

use std::future::Future;

use serde_json::Value;

use crate::domain::entity::downloader::DownloadResult;

use crate::domain::entity::alqanime::AlqDetailData;
use crate::domain::entity::anime::{
    AnimeData, AnimeDetailData, AnimeFullData, CompleteAnimeItem, CompleteAnimeListItem,
    FilterAnimeItem, Genre, GenreAnimeItem, LatestAnimeItem, OngoingAnimeItem,
    OngoingAnimeItemWithScore, OngoingAnimeListItem, Pagination, PaginationWithStringPages,
    SearchAnimeItem,
};
use crate::domain::entity::komik::{ChapterData, DetailData, KomikGenre, KomikItem};
use crate::domain::error::ScrapingError;

/// Anime listings and detail pages from the Otakudesu site.
pub trait OtakudesuAnimeRepository: Send + Sync {
    fn fetch_anime_index(&self) -> impl Future<Output = Result<AnimeData, ScrapingError>> + Send;
    fn fetch_genres(&self) -> impl Future<Output = Result<Vec<Genre>, ScrapingError>> + Send;
    fn fetch_anime_detail(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<AnimeDetailData, ScrapingError>> + Send;
    fn fetch_complete_anime_page(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<(Vec<CompleteAnimeListItem>, Pagination), ScrapingError>> + Send;
    fn fetch_ongoing_anime_page(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<(Vec<OngoingAnimeListItem>, Pagination), ScrapingError>> + Send;
    fn fetch_latest_anime_page(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_search_anime_page(
        &self,
        slug: &str,
        page: &str,
    ) -> impl Future<Output = Result<(Vec<SearchAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_genre_anime_page(
        &self,
        genre_slug: &str,
        page: &str,
    ) -> impl Future<Output = Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_anime_full(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<AnimeFullData, ScrapingError>> + Send;
}

/// Comic listings, details and chapters from the Komik site.
pub trait KomikComicRepository: Send + Sync {
    fn fetch_genres(&self) -> impl Future<Output = Result<Vec<KomikGenre>, ScrapingError>> + Send;
    fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
    fn fetch_detail(
        &self,
        komik_id: &str,
    ) -> impl Future<Output = Result<DetailData, ScrapingError>> + Send;
    fn fetch_chapter(
        &self,
        chapter_url: &str,
    ) -> impl Future<Output = Result<ChapterData, ScrapingError>> + Send;
    fn fetch_manga_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
    fn fetch_manhua_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
    fn fetch_manhwa_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
    fn fetch_popular_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
    fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<KomikItem>, Pagination), ScrapingError>> + Send;
}

/// Anime listings and detail pages from the Alqanime site.
pub trait AlqanimeAnimeRepository: Send + Sync {
    fn fetch_index_ongoing(
        &self,
    ) -> impl Future<Output = Result<Vec<OngoingAnimeItem>, ScrapingError>> + Send;
    fn fetch_index_complete(
        &self,
    ) -> impl Future<Output = Result<Vec<CompleteAnimeItem>, ScrapingError>> + Send;
    fn fetch_genres(&self) -> impl Future<Output = Result<Vec<Genre>, ScrapingError>> + Send;
    fn fetch_filter(
        &self,
        page: u32,
        genre: &str,
        status: &str,
        anime_type: &str,
        order: &str,
    ) -> impl Future<Output = Result<(Vec<FilterAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_detail(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<AlqDetailData, ScrapingError>> + Send;
    fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> impl Future<
        Output = Result<(Vec<SearchAnimeItem>, PaginationWithStringPages), ScrapingError>,
    > + Send;
    fn fetch_latest_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_ongoing_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<OngoingAnimeItemWithScore>, Pagination), ScrapingError>> + Send;
    fn fetch_complete_page(
        &self,
        page: u32,
    ) -> impl Future<Output = Result<(Vec<CompleteAnimeItem>, Pagination), ScrapingError>> + Send;
    fn fetch_episode_download(
        &self,
        episode_url: &str,
    ) -> impl Future<Output = Result<String, ScrapingError>> + Send;
}

/// Cache primitives the use cases depend on.
///
/// The port speaks raw strings: what to cache, for how long, and how to encode
/// it is use-case policy (see [`crate::application::cache`]), not adapter
/// detail. Both methods are best-effort — `get_cached` reports `None` when the
/// cache is unavailable and `set_cached` may fail — so a cache outage degrades
/// to cache-less rather than to a failed request.
pub trait CachePort: Send + Sync {
    fn get_cached(&self, key: &str) -> impl Future<Output = Option<String>> + Send;
    fn set_cached(
        &self,
        key: &str,
        value: String,
        ttl_secs: u64,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

// ============================================================================
// Single-endpoint ports
//
// These mirror the application's public surface one-for-one: each method is an
// operation a handler asks for, so the port needs no renaming relative to the
// use case it backs.
// ============================================================================

/// Profile lookups for social platforms.
pub trait StalkRepository: Send + Sync {
    fn github(&self, username: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn youtube(&self, username: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn twitter(&self, username: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
}

/// Public data endpoints that do not belong to a larger domain.
pub trait MiscRepository: Send + Sync {
    fn currency_converter(
        &self,
        amount: f64,
        from: &str,
        to: &str,
    ) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn harga_emas(&self) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn kurs_bca(&self) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn server_info(&self) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
}

/// Image rendering endpoints.
pub trait ImageRepository: Send + Sync {
    fn brat(&self, text: &str) -> impl Future<Output = Result<Vec<u8>, ScrapingError>> + Send;
    fn brat_animated(
        &self,
        text: &str,
    ) -> impl Future<Output = Result<Vec<u8>, ScrapingError>> + Send;
}

/// Network diagnostics and convenience link tools.
pub trait ToolsRepository: Send + Sync {
    fn whois(&self, domain: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn ip_location(&self, ip: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn tinyurl(&self, url: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn check_hosting(
        &self,
        domain: &str,
    ) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn hargapangan(&self) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn cek_resi(
        &self,
        resi: String,
        ekspedisi: Option<String>,
    ) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
}

/// General search and public-information queries.
pub trait SearchRepository: Send + Sync {
    fn bmkg(&self) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn jadwal_sholat(
        &self,
        kota: &str,
    ) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn weather(&self, city: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn google(&self, query: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
    fn yt_search(&self, query: &str) -> impl Future<Output = Result<Value, ScrapingError>> + Send;
}

/// Media download across every supported platform.
///
/// `detect_platform` is deliberately absent: it is pure domain logic
/// (`Platform::detect`), not an I/O port.
pub trait DownloaderRepository: Send + Sync {
    fn download_all_in_one(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_instagram(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_facebook(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_tiktok(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_youtube(
        &self,
        url: &str,
        quality: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_youtube_merge(
        &self,
        url: &str,
        quality: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_youtube_mp3(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_spotify(
        &self,
        url: &str,
        api_key: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_twitter(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_pinterest(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_mega(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_terabox(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_gdrive(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_mediafire(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_pixeldrain(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_threads(
        &self,
        url: &str,
        cookies: Option<&str>,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_doodstream(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_krakenfiles(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_danbooru(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_soundcloud(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_dailymotion(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_reddit(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_streamable(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_videy(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
    fn download_bilibili(
        &self,
        url: &str,
    ) -> impl Future<Output = Result<DownloadResult, ScrapingError>> + Send;
}
