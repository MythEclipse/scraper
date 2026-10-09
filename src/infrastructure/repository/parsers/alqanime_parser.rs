use crate::domain::entity::alqanime::{
    AlqDetailData, AlqDownloadItem, AlqEpisode, AlqLink, AlqRecommendation,
};
use crate::domain::entity::anime::{
    CompleteAnimeItem, DetailGenre, FilterAnimeItem, Genre, GenreAnimeItem, LatestAnimeItem,
    OngoingAnimeItem, OngoingAnimeItemWithScore, Pagination, PaginationWithStringPages,
    SearchAnimeItem,
};
use crate::domain::error::ScrapingError;
use crate::infrastructure::scraping::parsing_utils::parse_html;
use crate::infrastructure::scraping::parsing_utils::{
    attr, extract_slug, selector, text, text_from_or,
};

/// An episode entry from the detail page episode list.

/// Parser-specific types for Alqanime detail data
use regex::Regex;
use scraper::Selector;
use std::sync::LazyLock;

static ITEM_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article.bs").expect("literal CSS selector"));
static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".tt h2").expect("literal CSS selector"));
static IMG_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("literal CSS selector"));
static SCORE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".numscore").expect("literal CSS selector"));
static STATUS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".status").expect("literal CSS selector"));
static TYPE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".type").expect("literal CSS selector"));
static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a").expect("literal CSS selector"));
static PAGINATION_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".pagination .page-numbers:not(.next)").expect("literal CSS selector")
});
static NEXT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".pagination .next").expect("literal CSS selector"));
static SLUG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/([^/]+)/?$").expect("literal regex"));
static GENRE_SLUG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"genre-(.+)$").expect("literal regex"));
static EPISODE_LIST_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".eplister ul li").expect("literal CSS selector"));
static EP_NUM_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".epl-num").expect("literal CSS selector"));
static EP_TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".epl-title").expect("literal CSS selector"));
static EP_DATE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".epl-date").expect("literal CSS selector"));
static EP_DOWNLOAD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"a[rel="nofollow"][aria-label="Download"]"#).expect("literal CSS selector")
});
pub fn parse_ongoing_anime(html: &str) -> Result<Vec<OngoingAnimeItem>, ScrapingError> {
    let items = parse_ongoing_anime_with_score(html)?;
    Ok(items
        .into_iter()
        .map(|item| OngoingAnimeItem {
            title: item.title,
            slug: item.slug,
            poster: item.poster,
            current_episode: item.score,
            anime_url: item.anime_url,
        })
        .collect())
}

pub fn parse_complete_anime(html: &str) -> Result<Vec<CompleteAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut complete_anime = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();
        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();
        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let episode_count = text_from_or(&element, &STATUS_SELECTOR, "N/A");

        if !title.is_empty() {
            complete_anime.push(CompleteAnimeItem {
                title,
                slug,
                poster,
                episode_count,
                anime_url,
            });
        }
    }

    Ok(complete_anime)
}

pub fn parse_genres(html: &str) -> Result<Vec<Genre>, ScrapingError> {
    let document = parse_html(html);
    let mut genres = Vec::new();
    let genre_label_selector = selector("label[for^=\"genre-\"]").ok_or_else(|| {
        ScrapingError::Parse("Invalid selector: label[for^=\"genre-\"]".to_string())
    })?;

    for element in document.select(&genre_label_selector) {
        let name = text(&element).trim().to_string();
        let for_attr = attr(&element, "for").unwrap_or_default();

        let slug = GENRE_SLUG_REGEX
            .captures(&for_attr)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !name.is_empty() && !slug.is_empty() {
            genres.push(Genre {
                name,
                slug,
                url: String::new(),
            });
        }
    }

    Ok(genres)
}

pub fn parse_filter_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<FilterAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut anime_list = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();

        let score = element
            .select(&SCORE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("N/A".to_string());

        let status = element
            .select(&STATUS_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("Unknown".to_string());

        let anime_type = element
            .select(&TYPE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("Unknown".to_string());

        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            anime_list.push(FilterAnimeItem {
                title,
                slug,
                poster,
                score,
                status,
                r#type: anime_type,
                anime_url,
            });
        }
    }

    let last_visible_page = document
        .select(&PAGINATION_SELECTOR)
        .next_back()
        .map(|e| {
            e.text()
                .collect::<String>()
                .trim()
                .parse::<u32>()
                .unwrap_or(1)
        })
        .unwrap_or(1);

    let has_next_page = document.select(&NEXT_SELECTOR).next().is_some();
    let pagination = Pagination {
        current_page,
        last_visible_page,
        has_next_page,
        next_page: if has_next_page {
            Some(current_page + 1)
        } else {
            None
        },
        has_previous_page: current_page > 1,
        previous_page: if current_page > 1 {
            Some(current_page - 1)
        } else {
            None
        },
    };

    Ok((anime_list, pagination))
}

pub fn parse_genre_anime(html: &str) -> Result<Vec<GenreAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut anime_list = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();

        let score = element
            .select(&SCORE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("N/A".to_string());

        let status = element
            .select(&STATUS_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("Unknown".to_string());

        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            anime_list.push(GenreAnimeItem {
                title,
                slug,
                poster,
                episode: String::new(),
                score,
                status,
                anime_url,
            });
        }
    }

    Ok(anime_list)
}

pub fn parse_search_anime(html: &str) -> Result<Vec<SearchAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut anime_list = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();

        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            anime_list.push(SearchAnimeItem {
                title,
                slug,
                poster,
                episode: String::new(),
                anime_url,
                genres: Vec::new(),
                status: String::new(),
                rating: "N/A".to_string(),
                description: String::new(),
                r#type: "Unknown".to_string(),
                season: "Unknown".to_string(),
            });
        }
    }

    Ok(anime_list)
}

pub fn parse_latest_anime(html: &str) -> Result<Vec<LatestAnimeItem>, ScrapingError> {
    let document = parse_html(html);
    let mut anime_list = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();

        let score = element
            .select(&SCORE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("N/A".to_string());

        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            anime_list.push(LatestAnimeItem {
                title,
                slug,
                poster,
                episode: "N/A".to_string(),
                score,
                anime_url,
            });
        }
    }

    Ok(anime_list)
}

pub fn parse_ongoing_anime_with_score(
    html: &str,
) -> Result<Vec<OngoingAnimeItemWithScore>, ScrapingError> {
    let document = parse_html(html);
    let mut anime_list = Vec::new();

    for element in document.select(&ITEM_SELECTOR) {
        let title = element
            .select(&TITLE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let poster = element
            .select(&IMG_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("src").or(e.value().attr("data-src")))
            .unwrap_or("")
            .to_string();

        let score = element
            .select(&SCORE_SELECTOR)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or("N/A".to_string());

        let anime_url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        let slug = SLUG_REGEX
            .captures(&anime_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            anime_list.push(OngoingAnimeItemWithScore {
                title,
                slug,
                poster,
                score,
                anime_url,
            });
        }
    }

    Ok(anime_list)
}

pub fn parse_pagination(
    document: &scraper::Html,
    current_page: u32,
) -> Result<Pagination, ScrapingError> {
    let last_visible_page = document
        .select(&PAGINATION_SELECTOR)
        .next_back()
        .map(|e| {
            e.text()
                .collect::<String>()
                .trim()
                .parse::<u32>()
                .unwrap_or(1)
        })
        .unwrap_or(1);

    let has_next_page = document.select(&NEXT_SELECTOR).next().is_some();
    let pagination = Pagination {
        current_page,
        last_visible_page,
        has_next_page,
        next_page: if has_next_page {
            Some(current_page + 1)
        } else {
            None
        },
        has_previous_page: current_page > 1,
        previous_page: if current_page > 1 {
            Some(current_page - 1)
        } else {
            None
        },
    };

    Ok(pagination)
}

pub fn parse_pagination_with_string(
    document: &scraper::Html,
    current_page: u32,
) -> Result<PaginationWithStringPages, ScrapingError> {
    let last_visible_page = document
        .select(&PAGINATION_SELECTOR)
        .next_back()
        .map(|e| {
            e.text()
                .collect::<String>()
                .trim()
                .parse::<u32>()
                .unwrap_or(1)
        })
        .unwrap_or(1);

    let has_next_page = document.select(&NEXT_SELECTOR).next().is_some();
    let pagination = PaginationWithStringPages {
        current_page,
        last_visible_page,
        has_next_page,
        next_page: if has_next_page {
            Some((current_page + 1).to_string())
        } else {
            None
        },
        has_previous_page: current_page > 1,
        previous_page: if current_page > 1 {
            Some((current_page - 1).to_string())
        } else {
            None
        },
    };

    Ok(pagination)
}

/// Parse episode list from the detail page's `.eplister` section.
pub fn parse_episode_list(html: &str) -> Result<Vec<AlqEpisode>, ScrapingError> {
    let document = parse_html(html);
    let mut episodes = Vec::new();

    for element in document.select(&EPISODE_LIST_SELECTOR) {
        let episode = text_from_or(&element, &EP_NUM_SELECTOR, "");
        let title = text_from_or(&element, &EP_TITLE_SELECTOR, "");
        let date = text_from_or(&element, &EP_DATE_SELECTOR, "");
        let url = element
            .select(&LINK_SELECTOR)
            .next()
            .and_then(|e| e.value().attr("href"))
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            episodes.push(AlqEpisode {
                episode,
                title,
                url,
                date,
                download_url: None,
            });
        }
    }

    Ok(episodes)
}

/// Parse download URL from an individual episode page.
pub fn parse_episode_download(html: &str) -> Result<Option<String>, ScrapingError> {
    let document = parse_html(html);
    let download_url = document
        .select(&EP_DOWNLOAD_SELECTOR)
        .next()
        .and_then(|e| e.value().attr("href"))
        .map(|s| s.to_string());

    Ok(download_url)
}

pub fn parse_anime_detail(html: &str) -> Result<AlqDetailData, ScrapingError> {
    let document = parse_html(html);

    let title_selector = selector(".entry-title")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .entry-title".to_string()))?;
    let alt_title_selector = selector(".alter")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .alter".to_string()))?;
    let poster_selector = selector(".thumb img, .thumbook img, .wp-post-image, .ts-post-image")
        .ok_or_else(|| {
            ScrapingError::Parse(
                "Invalid selector: .thumb img, .thumbook img, .wp-post-image, .ts-post-image"
                    .to_string(),
            )
        })?;
    let poster2_selector = selector(".bigcover img, .bixbox.animefull .bigcover .ime img")
        .ok_or_else(|| {
            ScrapingError::Parse(
                "Invalid selector: .bigcover img, .bixbox.animefull .bigcover .ime img".to_string(),
            )
        })?;
    let spe_span_selector = selector(".info-content .spe span").ok_or_else(|| {
        ScrapingError::Parse("Invalid selector: .info-content .spe span".to_string())
    })?;
    let a_selector =
        selector("a").ok_or_else(|| ScrapingError::Parse("Invalid selector: a".to_string()))?;
    let synopsis_selector = selector(".entry-content p")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .entry-content p".to_string()))?;
    let genre_selector = selector(".genxed a")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .genxed a".to_string()))?;
    let download_container_selector = selector(".soraddl.dlone")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .soraddl.dlone".to_string()))?;
    let resolution_selector = selector(".res")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .res".to_string()))?;
    let link_selector = selector(".slink a")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .slink a".to_string()))?;
    let h3_selector =
        selector("h3").ok_or_else(|| ScrapingError::Parse("Invalid selector: h3".to_string()))?;
    let recommendation_selector = selector(".listupd .bs")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .listupd .bs".to_string()))?;
    let rec_title_selector = selector(".ntitle")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .ntitle".to_string()))?;
    let rec_img_selector =
        selector("img").ok_or_else(|| ScrapingError::Parse("Invalid selector: img".to_string()))?;
    let status_selector = selector(".status")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .status".to_string()))?;
    let type_selector = selector(".typez")
        .ok_or_else(|| ScrapingError::Parse("Invalid selector: .typez".to_string()))?;

    let title = text_from_or(&document.root_element(), &title_selector, "");
    let alternative_title = text_from_or(&document.root_element(), &alt_title_selector, "");

    let poster = document
        .select(&poster_selector)
        .next()
        .and_then(|e| {
            attr(&e, "src")
                .or_else(|| attr(&e, "data-src"))
                .or_else(|| attr(&e, "data-lazy-src"))
        })
        .unwrap_or_default();

    let poster2 = document
        .select(&poster2_selector)
        .next()
        .and_then(|e| {
            attr(&e, "src")
                .or_else(|| attr(&e, "data-src"))
                .or_else(|| attr(&e, "data-lazy-src"))
        })
        .unwrap_or_default();

    let r#type = document
        .select(&spe_span_selector)
        .find(|e| text(e).contains("Tipe:"))
        .and_then(|span| span.select(&a_selector).next())
        .map(|e| text(&e))
        .unwrap_or_default();

    let release_date = document
        .select(&spe_span_selector)
        .find(|e| text(e).contains("Dirilis:"))
        .map(|e| text(&e))
        .unwrap_or_default();

    let status = document
        .select(&spe_span_selector)
        .find(|e| text(e).contains("Status:"))
        .map(|e| text(&e))
        .unwrap_or_default();

    let synopsis = text_from_or(&document.root_element(), &synopsis_selector, "");

    let studio = document
        .select(&spe_span_selector)
        .find(|e| text(e).contains("Studio:"))
        .and_then(|span| span.select(&a_selector).next())
        .map(|e| text(&e))
        .unwrap_or_default();

    let mut genres = Vec::new();
    for element in document.select(&genre_selector) {
        let name = text(&element);
        let anime_url = attr(&element, "href").unwrap_or_default();
        let genre_slug = extract_slug(&anime_url);
        genres.push(DetailGenre {
            name,
            slug: genre_slug,
            anime_url,
        });
    }

    let mut batch = Vec::new();
    let mut ova = Vec::new();
    let mut downloads = Vec::new();

    for element in document.select(&download_container_selector) {
        let title = element
            .select(&h3_selector)
            .next()
            .map(|e| text(&e))
            .unwrap_or_else(|| "Unknown".to_string());

        let category = title.to_lowercase();
        let is_batch = category.contains("batch");
        let is_ova = category.contains("ova");

        let mut all_links = Vec::new();

        let row_selector = selector("table tr")
            .ok_or_else(|| ScrapingError::Parse("Invalid selector: table tr".to_string()))?;
        for row in element.select(&row_selector) {
            let resolution = text_from_or(&row, &resolution_selector, "");

            for link_element in row.select(&link_selector) {
                let provider = text(&link_element);
                let url = attr(&link_element, "href").unwrap_or_default();

                let name = if !resolution.is_empty() {
                    format!("{} - {}", resolution, provider)
                } else {
                    provider
                };

                all_links.push(AlqLink { name, url });
            }
        }

        let download_item = AlqDownloadItem {
            resolution: title,
            links: all_links,
        };

        if is_batch {
            batch.push(download_item);
        } else if is_ova {
            ova.push(download_item);
        } else {
            downloads.push(download_item);
        }
    }

    let mut recommendations = Vec::new();
    for element in document.select(&recommendation_selector) {
        let title = text_from_or(&element, &rec_title_selector, "");

        let anime_url = element
            .select(&a_selector)
            .next()
            .and_then(|e| attr(&e, "href"))
            .unwrap_or_default();

        let rec_slug = extract_slug(&anime_url);

        let poster = element
            .select(&rec_img_selector)
            .next()
            .and_then(|e| attr(&e, "data-src").or_else(|| attr(&e, "src")))
            .unwrap_or_default();

        let status = text_from_or(&element, &status_selector, "");

        let r#type = text_from_or(&element, &type_selector, "");

        recommendations.push(AlqRecommendation {
            title,
            slug: rec_slug,
            poster,
            status,
            r#type,
        });
    }

    let episodes = parse_episode_list(html).unwrap_or_default();

    Ok(AlqDetailData {
        title,
        alternative_title,
        poster,
        poster2,
        r#type,
        release_date,
        status,
        synopsis,
        studio,
        genres,
        producers: vec![],
        recommendations,
        batch,
        ova,
        downloads,
        episodes,
    })
}

pub fn parse_genre_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<GenreAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let anime_list = parse_genre_anime(html)?;
    let pagination = parse_pagination(&document, current_page)?;
    Ok((anime_list, pagination))
}

pub fn parse_search_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<SearchAnimeItem>, PaginationWithStringPages), ScrapingError> {
    let document = parse_html(html);
    let data = parse_search_anime(html)?;
    let pagination = parse_pagination_with_string(&document, current_page)?;
    Ok((data, pagination))
}

pub fn parse_latest_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<LatestAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let anime_list = parse_latest_anime(html)?;
    let pagination = parse_pagination(&document, current_page)?;
    Ok((anime_list, pagination))
}

pub fn parse_ongoing_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<OngoingAnimeItemWithScore>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let anime_list = parse_ongoing_anime_with_score(html)?;
    let pagination = parse_pagination(&document, current_page)?;
    Ok((anime_list, pagination))
}

pub fn parse_complete_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<CompleteAnimeItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let anime_list = parse_complete_anime(html)?;
    let pagination = parse_pagination(&document, current_page)?;
    Ok((anime_list, pagination))
}
