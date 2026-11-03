use crate::domain::entity::anime::Pagination;
use crate::domain::entity::komik::{Chapter, ChapterData, DetailData, KomikGenre, KomikItem};
use crate::domain::error::ScrapingError;
use crate::infrastructure::scraping::parsing_utils::parse_html;
use crate::infrastructure::scraping::parsing_utils::{
    attr, attr_from, attr_from_or, selector, text, text_from_or,
};
use rayon::prelude::*;
use regex::Regex;
use std::sync::LazyLock;
use tracing::info;

static TD_LAST_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("td:last-child").unwrap());
static TITLE_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("div#Judul h1 span[itemprop=\"name\"]").unwrap());
static H1_SELECTOR: LazyLock<scraper::Selector> = LazyLock::new(|| selector("h1").unwrap());
static TITLE_TAG_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("title").unwrap());
static INFO_ROW_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("table.inftable tr").unwrap());
static POSTER_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("section#Informasi .ims img").unwrap());
static DESC_SELECTOR: LazyLock<scraper::Selector> = LazyLock::new(|| selector("p.desc").unwrap());
static CHAPTER_LIST_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("#Daftar_Chapter tr, tbody#daftarChapter tr").unwrap());
static DATE_LINK_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("td.tanggalseries, .tanggalseries").unwrap());
static JUDUL2_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("div.judul2").unwrap());
static GENRE_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("ul.genre li a").unwrap());
static CHAPTER_LINK_SELECTOR: LazyLock<scraper::Selector> =
    LazyLock::new(|| selector("td.judulseries a").unwrap());
static CHAPTER_TITLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:chapter|ch\.?)\s*([\d\.]+)").expect("literal regex"));
static CHAPTER_NUMBER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([\d\.]+)").expect("literal regex"));

pub fn parse_genres(html: &str) -> Result<Vec<KomikGenre>, ScrapingError> {
    let document = parse_html(html);
    let mut genres = Vec::new();

    let genre_selector = selector("#Genre .ls3, section#Genre .ls3, .ls3")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let genre_name_selector =
        selector(".ls3p h4, h4").ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let genre_link_selector =
        selector("a[href*='/genre/']").ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let slug_regex = Regex::new(r"/genre/([^/]+)").unwrap();

    for element in document.select(&genre_selector) {
        let name = text_from_or(&element, &genre_name_selector, "");
        let href = attr_from(&element, &genre_link_selector, "href").unwrap_or_default();

        let slug = slug_regex
            .captures(&href)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !name.is_empty() && !slug.is_empty() {
            genres.push(KomikGenre {
                name,
                slug,
                count: None,
            });
        }
    }

    info!("Parsed {} genres", genres.len());
    Ok(genres)
}

pub fn parse_komik_chapter_document(
    html: &str,
    chapter_url: &str,
) -> Result<ChapterData, ScrapingError> {
    let document = parse_html(html);
    let _start_time = std::time::Instant::now();
    info!("Starting to parse komik chapter document");

    let title_selector =
        selector("title").ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let prev_chapter_selector = selector(
        "a[aria-label='Prev'][href*='chapter'], .nxpr a:not(.rl):not([href*='#Chapter']), .chprev a, a.prev",
    )
    .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let next_chapter_selector = selector(
        "a[aria-label='Next'][href*='chapter'], .nxpr a.rl, .nxpr a.next, .chnext a, a.next",
    )
    .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let image_selector = selector("#Baca_Komik img, img.klazy.ww")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;

    let title = document
        .select(&title_selector)
        .next()
        .map(|e| {
            let full_title = text(&e);
            if let Some(start) = full_title.find("Komik ") {
                if let Some(end) = full_title.find(" - Komiku") {
                    full_title[start + 6..end].trim().to_string()
                } else {
                    full_title
                }
            } else {
                full_title
            }
        })
        .unwrap_or_default();

    let next_chapter_id = document
        .select(&next_chapter_selector)
        .next()
        .and_then(|e| attr(&e, "href"))
        .map(|href| {
            href.trim_end_matches('/')
                .split('/')
                .rfind(|s| !s.is_empty())
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();

    fn get_previous_chapter_id(chapter_url: &str) -> String {
        const CHAPTER_PATTERN: &str = "chapter-";

        if let Some(pattern_pos) = chapter_url.rfind(CHAPTER_PATTERN) {
            let prefix = &chapter_url[0..pattern_pos];
            let suffix = &chapter_url[pattern_pos + CHAPTER_PATTERN.len()..];

            let chapter_num = suffix
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>();

            if let Ok(num) = chapter_num.parse::<u32>() {
                let prev_num = num.saturating_sub(1);

                let formatted_num = if chapter_num.starts_with('0') {
                    format!("{:0width$}", prev_num, width = chapter_num.len())
                } else {
                    prev_num.to_string()
                };

                return format!("{}{}{}", prefix, CHAPTER_PATTERN, formatted_num);
            }
        }

        String::new()
    }

    let prev_chapter_id_from_url = get_previous_chapter_id(chapter_url);
    let prev_chapter_id = if !prev_chapter_id_from_url.is_empty() {
        prev_chapter_id_from_url
    } else {
        document
            .select(&prev_chapter_selector)
            .next()
            .and_then(|e| attr(&e, "href"))
            .map(|href| {
                href.trim_end_matches('/')
                    .split('/')
                    .rfind(|s| !s.is_empty())
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default()
    };

    fn get_list_chapter_from_url(chapter_url: &str) -> String {
        if let Some(pos) = chapter_url.rfind("-chapter-") {
            chapter_url[..pos].to_string()
        } else {
            chapter_url.to_string()
        }
    }

    let list_chapter = get_list_chapter_from_url(chapter_url);

    let mut images = Vec::new();
    let forbidden_images = [
        "https://flagcdn.com/32x24/jp.png",
        "https://flagcdn.com/32x24/kr.png",
        "https://flagcdn.com/32x24/cn.png",
        "https://www.gstatic.com/firebasejs/ui/2.0.0/images/auth/google.svg",
        "https://www.gravatar.com/avatar/?d=mp&s=80",
        "/asset/img/komikuplus2.jpg",
        "https://komiku.org/asset/img/Loading.gif",
    ];
    for el in document.select(&image_selector) {
        if let Some(src) = attr(&el, "src")
            .or_else(|| attr(&el, "data-src"))
            .or_else(|| attr(&el, "data-lazy-src"))
            .or_else(|| {
                attr(&el, "srcset").and_then(|s| s.split_whitespace().next().map(|s| s.to_string()))
            })
        {
            if !forbidden_images.contains(&src.as_str()) {
                images.push(src);
            }
        }
    }

    Ok(ChapterData {
        title,
        next_chapter_id,
        prev_chapter_id,
        list_chapter,
        images,
    })
}

fn clean_text(text: String) -> String {
    text.replace(['\n', '\t'], " ").trim().to_string()
}

fn extract_value_after_keyword(full_text: &str, keywords: &[&str], default_index: usize) -> String {
    let lower_text = full_text.to_lowercase();
    for keyword in keywords {
        if let Some(pos) = lower_text.find(&format!("{}:", keyword)) {
            return clean_text(full_text[pos + keyword.len() + 1..].to_string());
        } else if let Some(pos) = lower_text.find(&format!("{} ", keyword)) {
            return clean_text(full_text[pos + keyword.len() + 1..].to_string());
        }
    }
    if let Some(colon_pos) = full_text.find(':') {
        return clean_text(full_text[colon_pos + 1..].to_string());
    }
    clean_text(full_text[default_index..].to_string())
}

fn find_table_row_with_text<'a>(
    info_rows: &[scraper::ElementRef<'a>],
    text_fragments: &[&str],
) -> Option<String> {
    let lower_text_fragments: Vec<String> =
        text_fragments.iter().map(|&s| s.to_lowercase()).collect();
    let td_last_selector = &*TD_LAST_SELECTOR;

    info_rows
        .iter()
        .find(|row| {
            let row_text = text(row).to_lowercase();
            lower_text_fragments
                .iter()
                .any(|fragment| row_text.contains(fragment))
        })
        .and_then(|row| {
            text_from_or(row, td_last_selector, "")
                .trim()
                .to_string()
                .into()
        })
}

pub fn parse_komik_detail_document(html: &str) -> Result<DetailData, ScrapingError> {
    let start_time = std::time::Instant::now();
    info!("Starting to parse komik detail document");

    let document = parse_html(html);

    let title_selector = &*TITLE_SELECTOR;
    let h1_selector = &*H1_SELECTOR;
    let title_tag_selector = &*TITLE_TAG_SELECTOR;
    let info_row_selector = &*INFO_ROW_SELECTOR;
    let poster_selector = &*POSTER_SELECTOR;
    let desc_selector = &*DESC_SELECTOR;
    let chapter_list_selector = &*CHAPTER_LIST_SELECTOR;
    let date_link_selector = &*DATE_LINK_SELECTOR;
    let judul2_selector = &*JUDUL2_SELECTOR;
    let genre_selector = &*GENRE_SELECTOR;
    let chapter_link_selector = &*CHAPTER_LINK_SELECTOR;

    let title = document
        .select(title_selector)
        .next()
        .map(|e| {
            let text = clean_text(text(&e));
            text.replace("Komik ", "")
                .replace("Manga ", "")
                .replace("Manhua ", "")
                .replace("Manhwa ", "")
                .trim()
                .to_string()
        })
        .or_else(|| {
            document
                .select(h1_selector)
                .next()
                .map(|e| clean_text(text(&e)))
        })
        .or_else(|| {
            document.select(title_tag_selector).next().map(|e| {
                let text = clean_text(text(&e));
                if text.contains("Komik ") {
                    text.replace("Komik ", "").trim().to_string()
                } else {
                    text.trim().to_string()
                }
            })
        })
        .unwrap_or_default();

    let info_rows_vec: Vec<scraper::ElementRef> = document.select(info_row_selector).collect();
    let info_rows = &info_rows_vec[..];

    let status = info_rows
        .iter()
        .find_map(|&row| {
            let full_text = text(&row);
            if full_text.to_lowercase().contains("status") {
                Some(
                    extract_value_after_keyword(&full_text, &["status"], 0)
                        .replace("Status", "")
                        .replace("Jenis Komik", "")
                        .replace("Type", "")
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();

    let r#type = info_rows
        .iter()
        .find_map(|&row| {
            let full_text = text(&row);
            if full_text.to_lowercase().contains("jenis komik")
                || full_text.to_lowercase().contains("type")
            {
                Some(
                    extract_value_after_keyword(&full_text, &["jenis komik", "type"], 0)
                        .replace("Jenis Komik", "")
                        .replace("Type", "")
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();

    let author = info_rows
        .iter()
        .find_map(|&row| {
            let full_text = text(&row);
            if full_text.to_lowercase().contains("pengarang")
                || full_text.to_lowercase().contains("author")
                || full_text.to_lowercase().contains("artist")
            {
                Some(
                    extract_value_after_keyword(&full_text, &["pengarang", "author", "artist"], 0)
                        .replace("Pengarang", "")
                        .replace("Author", "")
                        .replace("pengarang", "")
                        .replace("author", "")
                        .replace("Artist", "")
                        .replace("artist", "")
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();

    let poster = document
        .select(poster_selector)
        .next()
        .and_then(|e| attr(&e, "src"))
        .map(|s| s.split('?').next().unwrap_or(&s).to_string())
        .unwrap_or_default();

    let description = document
        .select(desc_selector)
        .map(|e| clean_text(text(&e)))
        .filter(|t| t.len() > 50)
        .collect::<Vec<String>>()
        .join("\n")
        .trim()
        .to_string();

    let release_date = find_table_row_with_text(info_rows, &["tanggal rilis", "release date"])
        .map(clean_text)
        .unwrap_or_else(|| {
            document
                .select(chapter_list_selector)
                .next_back()
                .and_then(|last| last.select(date_link_selector).next())
                .map(|e| clean_text(text(&e)))
                .unwrap_or_default()
        });

    let total_chapter = find_table_row_with_text(info_rows, &["total chapter", "total chapters"])
        .unwrap_or_else(|| {
            let count = document.select(chapter_list_selector).count();
            if count > 0 {
                count.to_string()
            } else {
                String::new()
            }
        });

    let updated_on = find_table_row_with_text(info_rows, &["diperbarui", "updated"])
        .or_else(|| {
            document.select(judul2_selector).next().map(|e| {
                let text_str = clean_text(text(&e));
                text_str.split("• ").nth(1).unwrap_or("").trim().to_string()
            })
        })
        .unwrap_or_else(|| {
            document
                .select(chapter_list_selector)
                .next()
                .and_then(|first| first.select(date_link_selector).next())
                .map(|e| clean_text(text(&e)))
                .unwrap_or_default()
        });

    let mut genres = Vec::new();
    for element in document.select(genre_selector) {
        let genre = clean_text(text(&element));
        if !genre.is_empty() {
            genres.push(genre);
        }
    }

    let raw_chapter_data: Vec<(String, String, String)> = document
        .select(chapter_list_selector)
        .filter_map(|el| {
            let chapter_link_element = el.select(chapter_link_selector).next();
            let date_element = el.select(date_link_selector).next();

            let chapter_text = chapter_link_element
                .as_ref()
                .map(|e| clean_text(text(e)))
                .unwrap_or_default();

            let date_text = date_element
                .map(|e| clean_text(text(&e)))
                .unwrap_or_default();

            let href_text = chapter_link_element
                .and_then(|e| attr(&e, "href"))
                .unwrap_or_default();

            if !chapter_text.is_empty() || !date_text.is_empty() || !href_text.is_empty() {
                Some((chapter_text, date_text, href_text))
            } else {
                None
            }
        })
        .collect();

    let chapters: Vec<Chapter> = raw_chapter_data
        .par_iter()
        .filter_map(|(chapter_text, date_text, href_text)| {
            let chapter = {
                let trimmed_chapter_text = chapter_text.trim();
                if let Some(captures) = CHAPTER_TITLE_REGEX.captures(trimmed_chapter_text) {
                    captures
                        .get(1)
                        .map_or(trimmed_chapter_text.to_string(), |m| m.as_str().to_string())
                } else if let Some(captures) = CHAPTER_NUMBER_REGEX.captures(trimmed_chapter_text) {
                    captures
                        .get(1)
                        .map_or(trimmed_chapter_text.to_string(), |m| m.as_str().to_string())
                } else {
                    trimmed_chapter_text.to_string()
                }
            };

            let date = date_text.trim().to_string();

            let chapter_id = href_text
                .split('/')
                .rfind(|s| !s.is_empty())
                .unwrap_or("")
                .to_string();

            if !chapter_id.is_empty() {
                Some(Chapter {
                    chapter,
                    date,
                    chapter_id,
                })
            } else {
                None
            }
        })
        .collect();

    let duration = start_time.elapsed();
    info!("Parsed komik detail document in {:?}", duration);

    Ok(DetailData {
        title,
        poster,
        description,
        status,
        r#type,
        release_date,
        author,
        total_chapter,
        updated_on,
        genres,
        chapters,
    })
}

pub fn parse_genre_page(
    html: &str,
    current_page: u32,
) -> Result<(Vec<KomikItem>, Pagination), ScrapingError> {
    let document = parse_html(html);
    let mut komik_list = Vec::new();

    let item_selector = selector(".bge, article, .ls4, .ls2")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let title_selector = selector(".kan h3, h3 a, h4 a")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let img_selector = selector(".bgei img, img.lazy, img")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let chapter_selector = selector(".new1:last-of-type a span:last-child, .new1 a:last-child span:last-child, .ls4s a, .ls24, .ls2l a")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let score_selector = selector(".up, .numscore, .epx")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let type_selector = selector(".tpe1_inf, .ls3p, .type")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let link_selector = selector(".kan h3 a, .bgei a, h3 a, h4 a, a")
        .ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let next_selector =
        selector("span[hx-get]").ok_or(ScrapingError::Parse("Selector error".to_string()))?;
    let slug_regex = Regex::new(r"/([^/]+)/?$").unwrap();

    for element in document.select(&item_selector) {
        let title = text_from_or(&element, &title_selector, "");

        let poster = element
            .select(&img_selector)
            .next()
            .and_then(|e| attr(&e, "data-src").or(attr(&e, "src")))
            .unwrap_or_else(|| "".to_string())
            .to_string();

        let chapter = text_from_or(&element, &chapter_selector, "N/A");

        let score = text_from_or(&element, &score_selector, "N/A");

        let komik_type = text_from_or(&element, &type_selector, "Unknown")
            .split_whitespace()
            .next()
            .unwrap_or("Unknown")
            .to_string();

        let komik_url = attr_from_or(&element, &link_selector, "href", "");

        let slug = slug_regex
            .captures(&komik_url)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        if !title.is_empty() {
            komik_list.push(KomikItem {
                title,
                slug,
                poster,
                chapter,
                score,
                r#type: komik_type,
                komik_url,
            });
        }
    }

    let has_next_page = document.select(&next_selector).next().is_some();
    let last_visible_page = if has_next_page {
        current_page + 1
    } else {
        current_page
    };
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

    Ok((komik_list, pagination))
}
