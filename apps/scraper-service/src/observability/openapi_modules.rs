use utoipa::OpenApi;

/// Manual aggregation of OpenAPI docs from module controllers.
#[derive(OpenApi)]
#[openapi(
    paths(
        // ── Anime module ──────────────────────────────────────────
        crate::presentation::handler::anime::anime_index,
        crate::presentation::handler::anime::genres,
        crate::presentation::handler::anime::detail_slug,
        crate::presentation::handler::anime::complete_anime_slug,
        crate::presentation::handler::anime::full_slug,
        crate::presentation::handler::anime::ongoing_anime_slug,
        crate::presentation::handler::anime::latest_slug,
        crate::presentation::handler::anime::search_slug_index,
        crate::presentation::handler::anime::search_slug_page,
        crate::presentation::handler::anime::genre_slug_index,
        crate::presentation::handler::anime::genre_slug_page,
        // ── Anime2 module ─────────────────────────────────────────
        crate::presentation::handler::anime2::index,
        crate::presentation::handler::anime2::genre_list,
        crate::presentation::handler::anime2::filter,
        crate::presentation::handler::anime2::detail_slug,
        crate::presentation::handler::anime2::genre_slug_index,
        crate::presentation::handler::anime2::genre_slug_page,
        crate::presentation::handler::anime2::search_slug_index,
        crate::presentation::handler::anime2::search_slug_page,
        crate::presentation::handler::anime2::latest_slug,
        crate::presentation::handler::anime2::ongoing_anime_slug,
        crate::presentation::handler::anime2::complete_anime_slug,
        // ── Komik module ──────────────────────────────────────────
        crate::presentation::handler::komik::genre_list,
        crate::presentation::handler::komik::chapter_slug,
        crate::presentation::handler::komik::detail_slug,
        crate::presentation::handler::komik::genre_slug,
        crate::presentation::handler::komik::genre_slug_page,
        crate::presentation::handler::komik::manga_slug,
        crate::presentation::handler::komik::manhua_slug,
        crate::presentation::handler::komik::manhwa_slug,
        crate::presentation::handler::komik::popular_slug,
        crate::presentation::handler::komik::search_slug,
        crate::presentation::handler::komik::search_slug_page,
        // ── Download module ────────────────────────────────────────
        crate::presentation::handler::downloader::download,
        crate::presentation::handler::downloader::detect_platform_handler,
        crate::presentation::handler::downloader::download_instagram,
        crate::presentation::handler::downloader::download_facebook,
        crate::presentation::handler::downloader::download_tiktok,
        crate::presentation::handler::downloader::download_youtube,
        crate::presentation::handler::downloader::download_youtube_mp3,
        crate::presentation::handler::downloader::download_spotify,
        crate::presentation::handler::downloader::download_twitter,
        crate::presentation::handler::downloader::download_pinterest,
        crate::presentation::handler::downloader::download_mega,
        crate::presentation::handler::downloader::download_terabox,
        crate::presentation::handler::downloader::download_gdrive,
        crate::presentation::handler::downloader::download_mediafire,
        crate::presentation::handler::downloader::download_pixeldrain,
        crate::presentation::handler::downloader::download_threads,
        crate::presentation::handler::downloader::download_doodstream,
        crate::presentation::handler::downloader::download_krakenfiles,
        crate::presentation::handler::downloader::download_danbooru,
        crate::presentation::handler::downloader::download_soundcloud,
        crate::presentation::handler::downloader::download_dailymotion,
        crate::presentation::handler::downloader::download_reddit,
        crate::presentation::handler::downloader::download_streamable,
        crate::presentation::handler::downloader::download_videy,
        crate::presentation::handler::downloader::download_bilibili,
        crate::presentation::handler::downloader::serve_merged_file,
        // ── Misc module ────────────────────────────────────────────
        crate::presentation::handler::misc::currency_converter_handler,
        crate::presentation::handler::misc::harga_emas_handler,
        crate::presentation::handler::misc::kurs_bca_handler,
        crate::presentation::handler::misc::server_info_handler,
        // ── Stalk module ───────────────────────────────────────────
        crate::presentation::handler::stalk::github_handler,
        crate::presentation::handler::stalk::youtube_handler,
        crate::presentation::handler::stalk::twitter_handler,
        // ── Search module ──────────────────────────────────────────
        crate::presentation::handler::search::bmkg_handler,
        crate::presentation::handler::search::jadwal_sholat_handler,
        crate::presentation::handler::search::weather_handler,
        crate::presentation::handler::search::google_handler,
        crate::presentation::handler::search::yt_handler,
        // ── Tool module ────────────────────────────────────────────
        crate::presentation::handler::tools::whois_handler,
        crate::presentation::handler::tools::ip_location_handler,
        crate::presentation::handler::tools::tinyurl_handler,
        crate::presentation::handler::tools::check_hosting_handler,
        crate::presentation::handler::tools::hargapangan_handler,
        crate::presentation::handler::tools::cek_resi_handler,
        // ── Image module ───────────────────────────────────────────
        crate::presentation::handler::image::brat_handler,
        crate::presentation::handler::image::brat_animated_handler,
    ),
    components(
        schemas(
            crate::presentation::dto::common::ApiResponse<String>,
            crate::presentation::dto::downloader::DownloadResponse,
        )
    ),
    tags(
        (name = "anime", description = "Anime endpoints"),
        (name = "anime2", description = "Anime2 endpoints"),
        (name = "komik", description = "Komik endpoints"),
        (name = "download", description = "Media download endpoints (23 platforms)"),
        (name = "misc", description = "Miscellaneous utilities (currency, gold, server)"),
        (name = "stalk", description = "User profile stalking (GitHub, YT, Twitter)"),
        (name = "search", description = "Search endpoints (BMKG, prayer times, weather, YouTube, Google)"),
        (name = "tool", description = "Utility tools (WHOIS, IP lookup, URL shortener, tracking, hosting)"),
        (name = "image", description = "Image generation (brat static & animated)"),
    )
)]
pub struct ModuleApiDoc;
