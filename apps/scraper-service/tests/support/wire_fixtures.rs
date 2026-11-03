//! Entity fixtures for the wire-contract test.
//!
//! Each constructor fills every field with a distinguishable value so a dropped
//! or mis-converted field shows up in the JSON comparison.

use std::collections::HashMap;

use scraper_service::domain::entity::alqanime::{
    AlqDetailData, AlqDownloadItem, AlqEpisode, AlqLink, AlqRecommendation,
};
use scraper_service::domain::entity::anime::{
    Anime2ItemDetail, AnimeDetailData, AnimeFullData, AnimeInfo, DetailGenre, DownloadLink,
    EpisodeInfo, EpisodeList, Pagination, Recommendation,
};
use scraper_service::domain::entity::downloader::{
    DownloadResult, DownloadStatus, MediaItem, MediaType,
};
use scraper_service::domain::entity::komik::{Chapter, DetailData};

pub fn pagination() -> Pagination {
    Pagination {
        current_page: 2,
        last_visible_page: 9,
        has_next_page: true,
        next_page: Some(3),
        has_previous_page: true,
        previous_page: Some(1),
    }
}

pub fn anime_detail() -> AnimeDetailData {
    AnimeDetailData {
        title: "Sousou no Frieren".to_string(),
        alternative_title: "Frieren: Beyond Journey's End".to_string(),
        poster: "https://cdn.example/frieren.jpg".to_string(),
        r#type: Some("TV".to_string()),
        status: Some("Completed".to_string()),
        release_date: "2023".to_string(),
        studio: "Madhouse".to_string(),
        genres: vec![DetailGenre {
            name: "Adventure".to_string(),
            slug: "adventure".to_string(),
            anime_url: "https://otakudesu.example/genre/adventure".to_string(),
        }],
        synopsis: "An elf mage retraces a journey.".to_string(),
        episode_lists: vec![EpisodeList {
            episode: "Episode 1".to_string(),
            slug: "episode-1".to_string(),
        }],
        batch: Vec::new(),
        producers: Vec::new(),
        recommendations: vec![Recommendation {
            title: "Bocchi the Rock!".to_string(),
            slug: "bocchi".to_string(),
            poster: "https://cdn.example/bocchi.jpg".to_string(),
            status: None,
            r#type: Some("TV".to_string()),
        }],
    }
}

pub fn anime_full() -> AnimeFullData {
    let mut download_urls: HashMap<String, Vec<DownloadLink>> = HashMap::new();
    download_urls.insert(
        "720p".to_string(),
        vec![DownloadLink {
            server: "Mega".to_string(),
            url: "https://mega.example/frieren-720".to_string(),
        }],
    );
    AnimeFullData {
        episode: "Episode 1".to_string(),
        episode_number: "1".to_string(),
        anime: AnimeInfo {
            slug: "sousou-no-frieren".to_string(),
        },
        has_next_episode: true,
        next_episode: Some(EpisodeInfo {
            slug: "episode-2".to_string(),
        }),
        has_previous_episode: false,
        previous_episode: None,
        stream_url: "https://stream.example/embed/1".to_string(),
        download_urls,
        image_url: "https://cdn.example/frieren-1.jpg".to_string(),
    }
}

pub fn anime2_detail() -> Anime2ItemDetail {
    Anime2ItemDetail {
        title: "Frieren".to_string(),
        slug: "frieren".to_string(),
        poster: "https://cdn.example/frieren.jpg".to_string(),
        poster2: "https://cdn.example/frieren-2.jpg".to_string(),
        synopsis: "An elf mage retraces a journey.".to_string(),
        alternative_title: "Sousou no Frieren".to_string(),
        r#type: "TV".to_string(),
        status: "Completed".to_string(),
        score: "9.3".to_string(),
        genres: vec![DetailGenre {
            name: "Adventure".to_string(),
            slug: "adventure".to_string(),
            anime_url: "https://alqanime.example/genre/adventure".to_string(),
        }],
        episodes: vec![EpisodeList {
            episode: "Episode 1".to_string(),
            slug: "episode-1".to_string(),
        }],
        recommendations: Vec::new(),
    }
}

pub fn komik_detail() -> DetailData {
    DetailData {
        title: "One Piece".to_string(),
        poster: "https://cdn.example/op.jpg".to_string(),
        description: "Pirates.".to_string(),
        status: "Ongoing".to_string(),
        r#type: "Manga".to_string(),
        release_date: "1997".to_string(),
        author: "Eiichiro Oda".to_string(),
        total_chapter: "1100".to_string(),
        updated_on: "2024-01-01".to_string(),
        genres: vec!["Action".to_string()],
        chapters: vec![Chapter {
            chapter: "Chapter 1100".to_string(),
            date: "2024-01-01".to_string(),
            chapter_id: "ch-1100".to_string(),
        }],
    }
}

pub fn alq_detail() -> AlqDetailData {
    AlqDetailData {
        title: "Frieren".to_string(),
        alternative_title: "Sousou no Frieren".to_string(),
        poster: "https://cdn.example/frieren.jpg".to_string(),
        poster2: "https://cdn.example/frieren-2.jpg".to_string(),
        r#type: "TV".to_string(),
        release_date: "2023".to_string(),
        status: "Completed".to_string(),
        synopsis: "An elf mage retraces a journey.".to_string(),
        studio: "Madhouse".to_string(),
        genres: vec![DetailGenre {
            name: "Adventure".to_string(),
            slug: "adventure".to_string(),
            anime_url: "https://alqanime.example/genre/adventure".to_string(),
        }],
        producers: vec!["Madhouse".to_string()],
        recommendations: vec![AlqRecommendation {
            title: "Bocchi the Rock!".to_string(),
            slug: "bocchi".to_string(),
            poster: "https://cdn.example/bocchi.jpg".to_string(),
            status: "Completed".to_string(),
            r#type: "TV".to_string(),
        }],
        batch: vec![AlqDownloadItem {
            resolution: "720p".to_string(),
            links: vec![AlqLink {
                name: "Mega".to_string(),
                url: "https://mega.example/frieren-batch".to_string(),
            }],
        }],
        ova: Vec::new(),
        downloads: Vec::new(),
        episodes: vec![AlqEpisode {
            episode: "Episode 1".to_string(),
            title: "The Journey's End".to_string(),
            url: "https://alqanime.example/episode-1".to_string(),
            date: "2023-09-29".to_string(),
            download_url: None,
        }],
    }
}

pub fn download_result() -> DownloadResult {
    DownloadResult {
        status: DownloadStatus::Success,
        message: None,
        title: Some("Frieren EP1".to_string()),
        author: Some("Madhouse".to_string()),
        thumbnail: Some("https://cdn.example/frieren.jpg".to_string()),
        duration: None,
        description: Some("Episode 1".to_string()),
        media: vec![
            MediaItem {
                url: "https://cdn.example/frieren-720.mp4".to_string(),
                quality: Some("720p".to_string()),
                file_type: Some(MediaType::Video),
                extension: Some("mp4".to_string()),
                thumbnail: None,
                file_size: Some("120MB".to_string()),
                size_bytes: Some(125_829_120),
                frame_width: Some("1280".to_string()),
                frame_height: Some("720".to_string()),
                note: None,
            },
            MediaItem {
                url: "https://cdn.example/frieren.mp3".to_string(),
                quality: None,
                file_type: Some(MediaType::Audio),
                extension: Some("mp3".to_string()),
                thumbnail: None,
                file_size: None,
                size_bytes: None,
                frame_width: None,
                frame_height: None,
                note: Some("audio only".to_string()),
            },
        ],
        provider: Some("savetube".to_string()),
    }
}
