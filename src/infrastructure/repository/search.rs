//! Infrastructure — Search utilities.
//!
//! Ported from Shirokami-API `scraper/search/*.js`:
//! bmkg (BMKG earthquake), jadwal-sholat (myquran.com), weather (OpenWeather),
//! google (HTML scrape), yt (via yt-dlp flat-playlist).

use crate::infrastructure::utils::http_client::http_client;
use reqwest::header::USER_AGENT;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::process::Stdio;

/// BMKG Indonesia earthquake info (auto-gempa, terkini, dirasakan).
pub async fn fetch_bmkg() -> Result<Value, String> {
    let urls = [
        "https://data.bmkg.go.id/DataMKG/TEWS/autogempa.json",
        "https://data.bmkg.go.id/DataMKG/TEWS/gempaterkini.json",
        "https://data.bmkg.go.id/DataMKG/TEWS/gempadirasakan.json",
    ];
    let mut auto = Value::Null;
    let mut terkini: Vec<Value> = Vec::new();
    let mut dirasakan: Vec<Value> = Vec::new();
    for url in urls {
        let resp = http_client()
            .client()
            .get(url)
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("BMKG returned HTTP {}", resp.status()));
        }
        let data: Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse: {}", e))?;
        if url.contains("autogempa") {
            auto = data
                .pointer("/Infogempa/gempa")
                .cloned()
                .unwrap_or(Value::Null);
        } else if url.contains("gempaterkini") {
            terkini = data
                .pointer("/Infogempa/gempa")
                .and_then(|g| g.as_array())
                .cloned()
                .unwrap_or_default();
        } else {
            dirasakan = data
                .pointer("/Infogempa/gempa")
                .and_then(|g| g.as_array())
                .cloned()
                .unwrap_or_default();
        }
    }
    Ok(json!({
        "autogempa": auto,
        "gempaterkini": terkini,
        "gempadirasakan": dirasakan,
    }))
}

/// Jadwal sholat from myquran.com.
pub async fn fetch_jadwal_sholat(kota: &str) -> Result<Value, String> {
    // 1. Find city id
    let search_url = format!(
        "https://api.myquran.com/v2/sholat/kota/cari/{}",
        urlencode(kota)
    );
    let city_data = get_json(&search_url).await?;
    if !city_data
        .get("status")
        .and_then(|s| s.as_bool())
        .unwrap_or(false)
    {
        return Ok(json!({ "error": "Kota tidak ditemukan" }));
    }
    let id_list = city_data
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    if id_list.is_empty() {
        return Ok(json!({ "error": "Kota tidak ditemukan" }));
    }

    // 2. Today's date (Asia/Jakarta). Use chrono with a fixed offset approximation.
    let now = chrono::Local::now();
    let (year, month, day) = (
        now.format("%Y").to_string(),
        now.format("%m").to_string(),
        now.format("%d").to_string(),
    );

    let mut results: Vec<Value> = Vec::new();
    for city in id_list.iter().take(5) {
        let cid = city.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let jadwal_url = format!(
            "https://api.myquran.com/v2/sholat/jadwal/{}/{}/{}/{}",
            cid, year, month, day
        );
        if let Ok(sched) = get_json(&jadwal_url).await {
            if let Some(d) = sched.get("data") {
                results.push(json!({
                    "lokasi": d.get("lokasi").cloned().unwrap_or(Value::Null),
                    "daerah": d.get("daerah").cloned().unwrap_or(Value::Null),
                    "jadwal": d.get("jadwal").cloned().unwrap_or(Value::Null),
                }));
            }
        }
    }

    if results.is_empty() {
        return Ok(json!({ "error": "Tidak dapat mengambil jadwal" }));
    }
    Ok(json!({ "total": results.len(), "schedules": results }))
}

/// OpenWeather current weather (uses env OPENWEATHER_API_KEY, falls back to source's key).
pub async fn fetch_weather(city: &str) -> Result<Value, String> {
    let key = std::env::var("OPENWEATHER_API_KEY")
        .unwrap_or_else(|_| "060a6bcfa19809c2cd4d97a212b19273".to_string());
    let url = format!(
        "https://api.openweathermap.org/data/2.5/weather?q={}&units=metric&appid={}",
        urlencode(city),
        key
    );
    get_json(&url).await
}

/// Google web search via HTML scraping.
pub async fn fetch_google(query: &str) -> Result<Value, String> {
    let url = format!(
        "https://www.google.com/search?q={}&safe=off&hl=en&gl=us",
        urlencode(query)
    );
    let resp = http_client()
        .client()
        .get(&url)
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .header("Accept-Language", "en-US")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    let document = Html::parse_document(&body);
    let result_sel =
        Selector::parse("div.g, div[data-hveid]").unwrap_or_else(|_| Selector::parse("a").unwrap());
    let link_sel = Selector::parse("a").unwrap();
    let h3_sel = Selector::parse("h3").unwrap();

    let mut results: Vec<Value> = Vec::new();
    for node in document.select(&result_sel) {
        let link = node
            .select(&link_sel)
            .find_map(|a| a.value().attr("href"))
            .and_then(|h| h.strip_prefix("/url?q="))
            .map(|h| h.split('&').next().unwrap_or(h).to_string());
        let title = node
            .select(&h3_sel)
            .next()
            .map(|h| h.text().collect::<String>());
        if title.is_some() && link.is_some() {
            results.push(json!({
                "title": title.unwrap_or_default(),
                "description": "",
                "link": link.unwrap_or_default(),
            }));
        }
        if results.len() >= 10 {
            break;
        }
    }

    Ok(json!(results))
}

/// YouTube search via yt-dlp flat playlist (real results, scraping).
pub async fn fetch_yt_search(query: &str) -> Result<Value, String> {
    let q = format!("ytsearch15:{}", query);
    let output = run_ytdlp(&q).await?;

    let mut videos: Vec<Value> = Vec::new();
    let arr: Vec<Value> = serde_json::from_str(&output).unwrap_or_default();
    for v in arr.iter().take(15) {
        videos.push(json!({
            "title": v.get("title").cloned().unwrap_or(Value::Null),
            "id": v.get("id").cloned().unwrap_or(Value::Null),
            "url": v.get("webpage_url").cloned().unwrap_or(Value::Null),
            "thumbnail": v.get("thumbnail").cloned().unwrap_or(Value::Null),
            "duration": v.get("duration").cloned().unwrap_or(Value::Null),
            "views": v.get("view_count").cloned().unwrap_or(Value::Null),
            "author": {
                "name": v.pointer("/channel").cloned().unwrap_or(Value::Null),
                "url": v.get("channel_url").cloned().unwrap_or(Value::Null),
            },
        }));
    }

    Ok(json!({ "total": videos.len(), "videos": videos }))
}

// ============================================================================
// Helpers
// ============================================================================

async fn get_json(url: &str) -> Result<Value, String> {
    let resp = http_client()
        .client()
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} from upstream", resp.status()));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| format!("JSON parse: {}", e))
}

/// Run yt-dlp with `--dump-json --flat-playlist` for the given search/query arg.
async fn run_ytdlp(query_arg: &str) -> Result<String, String> {
    let bins = [
        "/usr/local/bin/yt-dlp-native",
        "/home/code/hermes-agent/.venv/bin/yt-dlp",
        "yt-dlp",
    ];
    let bin = bins
        .iter()
        .find(|b| std::path::Path::new(b).exists() || **b == "yt-dlp")
        .copied()
        .unwrap_or("yt-dlp");

    let child = tokio::process::Command::new(bin)
        .args(["--dump-json", "--flat-playlist", "--no-warnings", query_arg])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("yt-dlp failed: {}", e))?;

    let stdout = String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;
    // `--flat-playlist` outputs multiple JSON lines (one per video).
    let mut joined = String::new();
    joined.push('[');
    let mut first = true;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !first {
            joined.push(',');
        }
        joined.push_str(line);
        first = false;
    }
    joined.push(']');
    Ok(joined)
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
