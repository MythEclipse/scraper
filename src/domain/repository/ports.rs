//! Port traits for the three upstream anime/comic sites.
//!
//! Each trait is a *port*: it names what a capability must do, in domain terms,
//! without naming a host, a URL path, or an HTML parser. Infrastructure
//! implements the port; the application layer holds it behind `impl Trait`, so
//! the dependency rule `application → domain ← infrastructure` holds.
//!
//! Ports cover a whole fetch-and-parse operation rather than a bare HTTP call,
//! which is what lets the use cases stop importing parsers.

use async_trait::async_trait;

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
#[async_trait]
pub trait OtakudesuAnimeRepository: Send + Sync {
    async fn fetch_anime_index(&self) -> Result<AnimeData, ScrapingError>;
    async fn fetch_genres(&self) -> Result<Vec<Genre>, ScrapingError>;
    async fn fetch_anime_detail(&self, slug: &str) -> Result<AnimeDetailData, ScrapingError>;
    async fn fetch_complete_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<CompleteAnimeListItem>, Pagination), ScrapingError>;
    async fn fetch_ongoing_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<OngoingAnimeListItem>, Pagination), ScrapingError>;
    async fn fetch_latest_anime_page(
        &self,
        slug: &str,
    ) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_search_anime_page(
        &self,
        slug: &str,
        page: &str,
    ) -> Result<(Vec<SearchAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_genre_anime_page(
        &self,
        genre_slug: &str,
        page: &str,
    ) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_anime_full(&self, slug: &str) -> Result<AnimeFullData, ScrapingError>;
}

/// Comic listings, details and chapters from the Komik site.
#[async_trait]
pub trait KomikComicRepository: Send + Sync {
    async fn fetch_genres(&self) -> Result<Vec<KomikGenre>, ScrapingError>;
    async fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
    async fn fetch_detail(&self, komik_id: &str) -> Result<DetailData, ScrapingError>;
    async fn fetch_chapter(&self, chapter_url: &str) -> Result<ChapterData, ScrapingError>;
    async fn fetch_manga_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
    async fn fetch_manhua_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
    async fn fetch_manhwa_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
    async fn fetch_popular_page(
        &self,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
    async fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> Result<(Vec<KomikItem>, Pagination), ScrapingError>;
}

/// Anime listings and detail pages from the Alqanime site.
#[async_trait]
pub trait AlqanimeAnimeRepository: Send + Sync {
    async fn fetch_index_ongoing(&self) -> Result<Vec<OngoingAnimeItem>, ScrapingError>;
    async fn fetch_index_complete(&self) -> Result<Vec<CompleteAnimeItem>, ScrapingError>;
    async fn fetch_genres(&self) -> Result<Vec<Genre>, ScrapingError>;
    async fn fetch_filter(
        &self,
        page: u32,
        genre: &str,
        status: &str,
        anime_type: &str,
        order: &str,
    ) -> Result<(Vec<FilterAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_detail(&self, slug: &str) -> Result<AlqDetailData, ScrapingError>;
    async fn fetch_genre_page(
        &self,
        genre_slug: &str,
        page: u32,
    ) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_search_page(
        &self,
        query: &str,
        page: u32,
    ) -> Result<(Vec<SearchAnimeItem>, PaginationWithStringPages), ScrapingError>;
    async fn fetch_latest_page(
        &self,
        page: u32,
    ) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_ongoing_page(
        &self,
        page: u32,
    ) -> Result<(Vec<OngoingAnimeItemWithScore>, Pagination), ScrapingError>;
    async fn fetch_complete_page(
        &self,
        page: u32,
    ) -> Result<(Vec<CompleteAnimeItem>, Pagination), ScrapingError>;
    async fn fetch_episode_download(&self, episode_url: &str) -> Result<String, ScrapingError>;
}

/// Read-through cache contract used by the use cases.
///
/// Declared here so the application layer can depend on the *capability* rather
/// than on the Redis adapter. Best-effort by contract: a failed cache write
/// must still return the freshly computed value, so a cache outage degrades to
/// cache-less rather than to a failed request.
#[async_trait]
pub trait CachePort: Send + Sync {
    async fn get_or_set<T, F, Fut>(
        &self,
        key: &str,
        ttl_secs: u64,
        compute: F,
    ) -> Result<T, String>
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, String>> + Send,
        Self: Sized;
}
