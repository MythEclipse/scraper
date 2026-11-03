//! Otakudesu HTML parser — native implementation using infrastructure utilities.
//!
//! Parses Otakudesu HTML pages into domain types. All parsing runs in
//! `spawn_blocking` (called from the repository layer).

use crate::domain::entity::anime::*;
use crate::domain::error::ScrapingError;
use crate::infrastructure::scraping::parsing_utils::{
    attr, attr_from, attr_from_or, extract_slug, parse_html, selector, text, text_from_or,
};

// ============================================================================
// INDEX (Ongoing + Complete Anime)
// ============================================================================

pub fn parse_ongoing_anime(html: &str) -> Result<Vec<OngoingAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    let venz_sel = selector(".venz ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .venz ul li selector".into()))?;
    let title_sel = selector(".thumbz h2.jdlflm")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;
    let ep_sel = selector(".epz")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse epz selector".into()))?;

    for element in document.select(&venz_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let href = attr_from(&element, &link_sel, "href").unwrap_or_default();
        let slug = extract_slug(&href);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let current_episode = text_from_or(&element, &ep_sel, "N/A");
        let anime_url = attr_from_or(&element, &link_sel, "href", "");

        if !title.is_empty() {
            items.push(OngoingAnimeItem {
                title,
                slug,
                poster,
                current_episode,
                anime_url,
            });
        }
    }
    Ok(items)
}

pub fn parse_complete_anime(html: &str) -> Result<Vec<CompleteAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    let venz_sel = selector(".venz ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .venz ul li selector".into()))?;
    let title_sel = selector(".thumbz h2.jdlflm")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;
    let ep_sel = selector(".epz")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse epz selector".into()))?;

    for element in document.select(&venz_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let href = attr_from(&element, &link_sel, "href").unwrap_or_default();
        let slug = extract_slug(&href);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let episode_count = text_from_or(&element, &ep_sel, "N/A");
        let anime_url = attr_from_or(&element, &link_sel, "href", "");

        if !title.is_empty() {
            items.push(CompleteAnimeItem {
                title,
                slug,
                poster,
                episode_count,
                anime_url,
            });
        }
    }
    Ok(items)
}

// ============================================================================
// GENRES
// ============================================================================

pub fn parse_genres(html: &str) -> Result<Vec<Genre>, ScrapingError> {
    let document = parse_html(html);
    let mut genres = Vec::new();
    let genre_sel = selector(".genres li a, .genre-list a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse genre selector".into()))?;

    for element in document.select(&genre_sel) {
        let name = text(&element);
        let url = attr(&element, "href").unwrap_or_default();
        let slug = extract_slug(&url);

        if !name.is_empty() && !slug.is_empty() {
            genres.push(Genre { name, slug, url });
        }
    }
    Ok(genres)
}

// ============================================================================
// DETAIL
// ============================================================================

pub fn parse_anime_detail_document(html: &str) -> Result<AnimeDetailData, ScrapingError> {
    let document = parse_html(html);

    let info_sel = selector(".infozingle p")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse info selector".into()))?;
    let poster_sel = selector(".fotoanime img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse poster selector".into()))?;
    let synopsis_sel = selector(".sinopc")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse synopsis selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let ep_list_sel = selector(".episodelist ul li a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse episode list selector".into()))?;
    let rec_sel = selector("#recommend-anime-series .isi-anime")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse recommendation selector".into()))?;
    let rec_title_sel = selector(".judul-anime a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse rec title selector".into()))?;
    let rec_img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse rec img selector".into()))?;

    let mut title = String::new();
    let mut alternative_title = String::new();
    let mut r#type: Option<String> = None;
    let mut status: Option<String> = None;
    let mut release_date = String::new();
    let mut studio = String::new();

    for element in document.select(&info_sel) {
        let text = text(&element);
        if text.contains("Judul:") {
            title = text.replace("Judul:", "").trim().to_string();
        } else if text.contains("Japanese:") {
            alternative_title = text.replace("Japanese:", "").trim().to_string();
        } else if text.contains("Type:") {
            let type_str = text.replace("Type:", "").trim().to_string();
            if !type_str.is_empty() {
                r#type = Some(type_str);
            }
        } else if text.contains("Status:") {
            let status_str = text.replace("Status:", "").trim().to_string();
            if !status_str.is_empty() {
                status = Some(status_str);
            }
        } else if text.contains("Tanggal Rilis:") {
            release_date = text.replace("Tanggal Rilis:", "").trim().to_string();
        } else if text.contains("Studio:") {
            studio = text.replace("Studio:", "").trim().to_string();
        }
    }

    let poster = document
        .select(&poster_sel)
        .next()
        .and_then(|e| e.value().attr("src"))
        .unwrap_or("")
        .to_string();

    let synopsis = text_from_or(&document.root_element(), &synopsis_sel, "");

    let mut genres = Vec::new();
    if let Some(genres_element) = document
        .select(&info_sel)
        .find(|e| text(e).contains("Genres:"))
    {
        for genre_link in genres_element.select(&link_sel) {
            let gname = text(&genre_link);
            let anine_url = attr(&genre_link, "href").unwrap_or_default();
            let genre_slug = extract_slug(&anine_url);
            genres.push(DetailGenre {
                name: gname,
                slug: genre_slug,
                anime_url: anine_url,
            });
        }
    }

    let mut episode_lists = Vec::new();
    for element in document.select(&ep_list_sel) {
        let episode = text(&element);
        let href = attr(&element, "href").unwrap_or_default();
        let slug = extract_slug(&href);
        episode_lists.push(EpisodeList { episode, slug });
    }

    let mut recommendations = Vec::new();
    for element in document.select(&rec_sel) {
        let rtitle = text_from_or(&element, &rec_title_sel, "");
        let rposter = attr_from_or(&element, &rec_img_sel, "src", "");
        let rhref = element
            .select(&link_sel)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("");

        let rslug = extract_slug(rhref);

        recommendations.push(Recommendation {
            title: rtitle,
            slug: rslug,
            poster: rposter,
            status: None,
            r#type: None,
        });
    }

    Ok(AnimeDetailData {
        title,
        alternative_title,
        poster,
        r#type,
        status,
        release_date,
        studio,
        genres,
        synopsis,
        episode_lists,
        batch: vec![],
        producers: vec![],
        recommendations,
    })
}

// ============================================================================
// PAGINATION HELPER
// ============================================================================

fn parse_pagination(slug: &str, document: &scraper::Html) -> Result<Pagination, ScrapingError> {
    let pagination_sel = selector(".pagenavix .page-numbers:not(.next)")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse pagination selector".into()))?;
    let next_sel = selector(".pagenavix .next.page-numbers")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse next selector".into()))?;

    let current_page = slug.parse::<u32>().unwrap_or(1);
    let last_visible_page = document
        .select(&pagination_sel)
        .next_back()
        .and_then(|e| e.text().collect::<String>().trim().parse::<u32>().ok())
        .unwrap_or(1);

    let has_next_page = document.select(&next_sel).next().is_some();
    let next_page = if has_next_page {
        Some(current_page + 1)
    } else {
        None
    };
    let has_previous_page = current_page > 1;
    let previous_page = if has_previous_page {
        Some(current_page - 1)
    } else {
        None
    };

    Ok(Pagination {
        current_page,
        last_visible_page,
        has_next_page,
        next_page,
        has_previous_page,
        previous_page,
    })
}

// ============================================================================
// COMPLETE ANIME PAGE
// ============================================================================

pub fn parse_anime_page(
    html: &str,
    slug: &str,
) -> Result<(Vec<CompleteAnimeListItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    let venz_sel = selector(".venz ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .venz ul li selector".into()))?;
    let title_sel = selector(".thumbz h2.jdlflm")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;
    let ep_sel = selector(".epz")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse epz selector".into()))?;

    for element in document.select(&venz_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let anime_url = attr_from_or(&element, &link_sel, "href", "");
        let slug = extract_slug(&anime_url);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let episode_count = text_from_or(&element, &ep_sel, "N/A");

        if !title.is_empty() {
            items.push(CompleteAnimeListItem {
                title,
                slug,
                poster,
                episode_count,
                anime_url,
            });
        }
    }

    let pagination = parse_pagination(slug, &document)?;
    Ok((items, pagination))
}

// ============================================================================
// ONGOING ANIME PAGE
// ============================================================================

pub fn parse_ongoing_anime_document(
    html: &str,
    slug: &str,
) -> Result<(Vec<OngoingAnimeListItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    let venz_sel = selector(".venz ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .venz ul li selector".into()))?;
    let title_sel = selector(".thumbz h2.jdlflm")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;
    let score_sel = selector(".epz")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse epz selector".into()))?;

    for element in document.select(&venz_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let anime_url = attr_from_or(&element, &link_sel, "href", "");
        let slug = extract_slug(&anime_url);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let score = text_from_or(&element, &score_sel, "N/A");

        if !title.is_empty() {
            items.push(OngoingAnimeListItem {
                title,
                slug,
                poster,
                score,
                anime_url,
            });
        }
    }

    let pagination = parse_pagination(slug, &document)?;
    Ok((items, pagination))
}

// ============================================================================
// LATEST ANIME PAGE
// ============================================================================

pub fn parse_latest_anime_document(
    html: &str,
    slug: &str,
) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    let venz_sel = selector(".venz ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .venz ul li selector".into()))?;
    let title_sel = selector(".thumbz h2.jdlflm")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let img_sel = selector("img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;
    let ep_sel = selector(".epz")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse epz selector".into()))?;

    for element in document.select(&venz_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let anime_url = attr_from_or(&element, &link_sel, "href", "");
        let slug = extract_slug(&anime_url);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let episode = text_from_or(&element, &ep_sel, "N/A");

        if !title.is_empty() {
            items.push(LatestAnimeItem {
                title,
                slug,
                poster,
                episode,
                score: String::new(),
                anime_url,
            });
        }
    }

    let pagination = parse_pagination(slug, &document)?;
    Ok((items, pagination))
}

// ============================================================================
// SEARCH ANIME PAGE
// ============================================================================

/// Normalise scraped text: collapse newlines/tabs into single spaces and trim.
fn clean_text(text: String) -> String {
    text.replace(['\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn parse_search_anime_document(
    html: &str,
    page: &str,
) -> Result<(Vec<SearchAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    // Otakudesu search results live under `<ul class="chivsrc"><li>...`, one
    // result per `<li>` with `<h2><a href="...">Title</a></h2>` plus a series
    // of `<div class="set"><b>Label</b> : value</div>` rows (Status, Rating,
    // Genre...). The old parser targeted the index layout (`.venz ul li`,
    // `.thumbz h2.jdlflm`, `img`, `.epz`) which does NOT appear on the search
    // page, so search silently returned 0 items.
    let item_sel = selector(".chivsrc li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .chivsrc li selector".into()))?;
    let title_link_sel = selector("h2 a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse h2 a selector".into()))?;
    let set_row_sel = selector(".set")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .set selector".into()))?;

    let status_sel = selector(".set b:first-child, .set b")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse status label selector".into()))?;

    for element in document.select(&item_sel) {
        let title = text_from_or(&element, &title_link_sel, "");
        let anime_url = attr_from_or(&element, &title_link_sel, "href", "");
        let slug = extract_slug(&anime_url);

        if title.is_empty() {
            continue;
        }

        // Extract label → value pairs from each `.set` row (e.g. "Status : Ongoing",
        // "Rating : 8.0", "Genre : Action, Drama"). Falls back gracefully when the
        // value is blank or the row label is absent.
        let mut status = String::new();
        let mut rating = String::new();
        let mut genres = Vec::new();
        for row in element.select(&set_row_sel) {
            let row_text = text(&row);
            let lower = row_text.to_lowercase();
            if lower.contains("status") {
                if let Some(label) = row.select(&status_sel).next() {
                    let label_len = text(&label).len();
                    status = clean_text(row_text[label_len..].to_string());
                } else {
                    status = clean_text(row_text.trim_start_matches("Status").to_string());
                }
            } else if lower.contains("rating") {
                let label_len = row
                    .select(&status_sel)
                    .next()
                    .map(|label| text(&label).len())
                    .unwrap_or("Rating".len());
                rating = clean_text(row_text[label_len..].to_string());
            } else if lower.contains("genre") {
                let label_len = row
                    .select(&status_sel)
                    .next()
                    .map(|label| text(&label).len())
                    .unwrap_or("Genre".len());
                let raw = clean_text(row_text[label_len..].to_string());
                genres = raw
                    .split(',')
                    .map(|g| g.trim().to_string())
                    .filter(|g| !g.is_empty())
                    .collect();
            }
        }

        items.push(SearchAnimeItem {
            title,
            slug,
            poster: String::new(),
            episode: String::new(),
            anime_url,
            genres,
            status,
            rating,
            description: String::new(),
            r#type: String::new(),
            season: String::new(),
        });
    }

    let pagination = parse_pagination(page, &document)?;
    Ok((items, pagination))
}

// ============================================================================
// GENRE ANIME PAGE
// ============================================================================

pub fn parse_genre_anime_document(
    html: &str,
    page: &str,
) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut items = Vec::new();

    // Current otakudesu genre archive pages use the `.col-anime` card layout
    // (`.col-md-4 .col-anime`), each with `.col-anime-title a`, `.col-anime-eps`,
    // `.col-anime-rating`, `.col-anime-cover img` and a synopsis. The old
    // `.venz ul li` / `.thumbz h2.jdlflm` / `.epz` layout no longer appears on
    // these pages, which made genre filtering silently return 0 items.
    let card_sel = selector(".col-anime")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .col-anime selector".into()))?;
    let title_sel = selector(".col-anime-title a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse title selector".into()))?;
    let eps_sel = selector(".col-anime-eps")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse eps selector".into()))?;
    let rating_sel = selector(".col-anime-rating")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse rating selector".into()))?;
    let img_sel = selector(".col-anime-cover img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse img selector".into()))?;

    for element in document.select(&card_sel) {
        let title = text_from_or(&element, &title_sel, "");
        let anime_url = attr_from_or(&element, &title_sel, "href", "");
        let slug = extract_slug(&anime_url);
        let poster = attr_from_or(&element, &img_sel, "src", "");
        let episode = text_from_or(&element, &eps_sel, "N/A");
        let score = text_from_or(&element, &rating_sel, "");

        if !title.is_empty() {
            items.push(GenreAnimeItem {
                title,
                slug,
                poster,
                episode,
                score,
                status: String::new(),
                anime_url,
            });
        }
    }

    let pagination = parse_pagination(page, &document)?;
    Ok((items, pagination))
}

// ============================================================================
// FULL EPISODE PAGE
// ============================================================================

pub fn parse_anime_full_document(html: &str, slug: &str) -> Result<AnimeFullData, ScrapingError> {
    let document = parse_html(html);

    let ep_title_sel = selector("h1.posttl")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse h1.posttl selector".into()))?;
    let img_sel = selector(".cukder img")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse .cukder img selector".into()))?;
    let stream_sel = selector("#embed_holder iframe")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse embed_holder selector".into()))?;
    let dl_item_sel = selector(".download ul li")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse download selector".into()))?;
    let res_sel = selector("strong")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse strong selector".into()))?;
    let link_sel = selector("a")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse link selector".into()))?;
    let next_ep_sel = selector(".flir a[title*='Episode Selanjutnya']")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse next episode selector".into()))?;
    let prev_ep_sel = selector(".flir a[title*='Episode Sebelumnya']")
        .ok_or_else(|| ScrapingError::Parse("Failed to parse prev episode selector".into()))?;

    let episode = document
        .select(&ep_title_sel)
        .next()
        .map(|e| text(&e))
        .unwrap_or_default();

    let episode_number = episode
        .split("Episode")
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let image_url = document
        .select(&img_sel)
        .next()
        .and_then(|e| attr(&e, "src"))
        .unwrap_or_default();

    let stream_url = document
        .select(&stream_sel)
        .next()
        .and_then(|e| attr(&e, "src"))
        .unwrap_or_default();

    let mut download_urls = std::collections::HashMap::new();

    for element in document.select(&dl_item_sel) {
        let resolution = element
            .select(&res_sel)
            .next()
            .map(|e| text(&e))
            .unwrap_or_default();

        let mut links = Vec::new();
        for link_element in element.select(&link_sel) {
            let server = text(&link_element);
            let url = attr(&link_element, "href").unwrap_or_default();
            links.push(DownloadLink { server, url });
        }

        if !resolution.is_empty() && !links.is_empty() {
            download_urls.insert(resolution, links);
        }
    }

    let next_episode_element = document.select(&next_ep_sel).next();
    let previous_episode_element = document.select(&prev_ep_sel).next();

    let next_episode_slug = next_episode_element
        .and_then(|e| attr(&e, "href"))
        .and_then(|href| {
            href.split('/')
                .nth(href.split('/').count().saturating_sub(2))
                .map(|s| s.to_string() + "/")
        });

    let previous_episode_slug = previous_episode_element
        .and_then(|e| attr(&e, "href"))
        .and_then(|href| {
            href.split('/')
                .nth(href.split('/').count().saturating_sub(2))
                .map(|s| s.to_string() + "/")
        });

    Ok(AnimeFullData {
        episode,
        episode_number,
        anime: AnimeInfo {
            slug: slug.to_string(),
        },
        has_next_episode: next_episode_slug.is_some(),
        next_episode: next_episode_slug.map(|s| EpisodeInfo { slug: s }),
        has_previous_episode: previous_episode_slug.is_some(),
        previous_episode: previous_episode_slug.map(|s| EpisodeInfo { slug: s }),
        stream_url,
        download_urls,
        image_url,
    })
}
