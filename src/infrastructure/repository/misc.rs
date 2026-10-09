//! Infrastructure — Misc utility scrapers (currency-converter, harga-emas, kurs-bca, server-info).
//!
//! Ported from Shirokami-API `scraper/misc/*.js`.
//! HTML scraping via reqwest + scraper crate.

use async_trait::async_trait;

use crate::domain::error::ScrapingError;
use crate::domain::repository::MiscRepository;

use crate::infrastructure::utils::http_client::http_client;
use regex::Regex;
use reqwest::header::USER_AGENT;
use scraper::{Html, Selector};
use serde_json::{json, Value};

/// Currency converter: scrape wise.com exchange rate page.
/// `amount`: amount to convert, `from`/`to`: currency codes (e.g. USD→IDR).
pub async fn fetch_currency_converter(amount: f64, from: &str, to: &str) -> Result<Value, String> {
    let from_lower = from.to_lowercase();
    let to_lower = to.to_lowercase();
    let url = format!(
        "https://wise.com/gb/currency-converter/{}-to-{}-rate?amount={}",
        from_lower, to_lower, amount
    );

    let resp = http_client()
        .client()
        .get(&url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from wise.com", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    // Try regex first: "1 USD = 15,885.00 IDR"
    let re = Regex::new(r"1 [A-Z]{3} = ([0-9,.]+) [A-Z]{3}")
        .map_err(|e| format!("Regex compile: {}", e))?;

    if let Some(caps) = re.captures(&body) {
        let rate_str = caps.get(1).map(|m| m.as_str()).unwrap_or("0");
        let rate = rate_str.replace(',', "").parse::<f64>().unwrap_or(0.0);
        if rate > 0.0 {
            let result = amount * rate;
            return Ok(json!({
                "from": from.to_uppercase(),
                "to": to.to_uppercase(),
                "amount": amount,
                "rate": rate,
                "result": result,
                "updateTime": chrono::Utc::now().to_rfc3339(),
            }));
        }
    }

    // Fallback: scrape the page for any rate info
    let document = Html::parse_document(&body);
    let selector = Selector::parse("body").unwrap();
    let text = document
        .select(&selector)
        .flat_map(|e| e.text())
        .collect::<String>();

    // Look for patterns like "15,885.00 IDR" in body text
    let re2 = Regex::new(&format!(r"([0-9,.]+)\s+{}", to.to_uppercase()))
        .map_err(|e| format!("Regex compile: {}", e))?;

    if let Some(caps) = re2.captures(&text) {
        let rate_str = caps.get(1).map(|m| m.as_str()).unwrap_or("0");
        let rate = rate_str.replace(',', "").parse::<f64>().unwrap_or(0.0);
        if rate > 0.0 {
            let result = amount * rate;
            return Ok(json!({
                "from": from.to_uppercase(),
                "to": to.to_uppercase(),
                "amount": amount,
                "rate": rate,
                "result": result,
                "updateTime": chrono::Utc::now().to_rfc3339(),
            }));
        }
    }

    Err("Gagal mendapatkan kurs dari wise.com".to_string())
}

/// Scrape harga emas Antam from harga-emas.org.
pub async fn fetch_harga_emas() -> Result<Value, String> {
    let url = "https://harga-emas.org/";

    let resp = http_client()
        .client()
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from harga-emas.org", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    let document = Html::parse_document(&body);

    let mut result = json!({
        "title": "Harga Emas Antam",
        "hargaUtama": "N/A",
        "perubahan": "0",
        "detail": [],
    });

    // Look for tables containing "Antam" + "Pegadaian"
    let table_sel = Selector::parse("table").unwrap();
    let tr_sel = Selector::parse("tbody tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let mut detail: Vec<Value> = Vec::new();

    for table in document.select(&table_sel) {
        let table_text = table.text().collect::<String>();
        if table_text.contains("Antam") && table_text.contains("Pegadaian") {
            for tr in table.select(&tr_sel) {
                let cols: Vec<String> = tr
                    .select(&td_sel)
                    .map(|td| td.text().collect::<String>())
                    .map(|s| s.trim().to_string())
                    .collect();

                if cols.len() >= 2 {
                    let label = cols[0].trim();
                    let price = cols[1].trim();

                    if !label.is_empty() && !price.is_empty() {
                        // Check if label is a numeric gram value
                        let numeric_label: String =
                            label.chars().filter(|c| c.is_ascii_digit()).collect();
                        if !numeric_label.is_empty() {
                            detail.push(json!({
                                "ukuran": format!("{} gr", label.replace(" gr", "").replace("gr", "").trim()),
                                "harga": format!("Rp {}", price),
                            }));
                        }
                    }
                }
            }
            break;
        }
    }

    if !detail.is_empty() {
        // Find the 1gr price as main price (fallback: smallest gram size).
        let one_gr = detail.iter().find(|d| {
            d["ukuran"]
                .as_str()
                .map(|s| {
                    let s = s.trim();
                    s == "1 gr" || s.starts_with("1 gr") || s.starts_with("1gr")
                })
                .unwrap_or(false)
        });
        let main_price = one_gr
            .map(|d| d["harga"].as_str().unwrap_or("N/A"))
            .unwrap_or_else(|| detail[0]["harga"].as_str().unwrap_or("N/A"));

        result["hargaUtama"] = json!(main_price);
        result["detail"] = json!(detail);
    }

    Ok(result)
}

/// Scrape kurs BCA (jual/beli) from bca.co.id.
pub async fn fetch_kurs_bca() -> Result<Value, String> {
    let url = "https://www.bca.co.id/id/informasi/kurs";

    let resp = http_client()
        .client()
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header("Referer", "https://www.google.com/")
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} from bca.co.id", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    let document = Html::parse_document(&body);

    let table_sel = Selector::parse("table").unwrap();
    let tr_sel = Selector::parse("tbody tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let mut kurs_data: Vec<Value> = Vec::new();

    for table in document.select(&table_sel) {
        let table_text = table.text().collect::<String>();
        if table_text.contains("USD") || table_text.contains("SGD") || table_text.contains("kurs") {
            for tr in table.select(&tr_sel) {
                let cols: Vec<String> = tr
                    .select(&td_sel)
                    .map(|td| td.text().collect::<String>().trim().to_string())
                    .collect();

                if cols.len() >= 3
                    && !cols[0].is_empty()
                    && !cols[1].is_empty()
                    && !cols[2].is_empty()
                {
                    kurs_data.push(json!({
                        "currency": cols[0],
                        "jual": cols[1],
                        "beli": cols[2],
                    }));
                }
            }
            if !kurs_data.is_empty() {
                break;
            }
        }
    }

    if kurs_data.is_empty() {
        return Err("Gagal mendapatkan data kurs BCA".to_string());
    }

    Ok(json!({
        "source": "BCA",
        "url": url,
        "data": kurs_data,
    }))
}

/// Local server info: OS, CPU, RAM, storage.
pub async fn fetch_server_info() -> Result<Value, String> {
    let os_info = json!({
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY,
    });

    // CPU info from /proc/cpuinfo (Linux)
    let cpu_model = tokio::fs::read_to_string("/proc/cpuinfo")
        .await
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // RAM from /proc/meminfo
    let mem_info = tokio::fs::read_to_string("/proc/meminfo")
        .await
        .ok()
        .unwrap_or_default();
    let total_mem_kb: u64 = mem_info
        .lines()
        .find(|l| l.starts_with("MemTotal"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let free_mem_kb: u64 = mem_info
        .lines()
        .find(|l| l.starts_with("MemAvailable"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let total_mem_gb = total_mem_kb as f64 / 1048576.0;
    let free_mem_gb = free_mem_kb as f64 / 1048576.0;

    // Disk usage from `df -B1 /`
    let disk_usage = tokio::process::Command::new("df")
        .args(["-B1", "/"])
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|out| {
            let lines: Vec<&str> = out.lines().collect();
            if lines.len() >= 2 {
                let parts: Vec<&str> = lines[1].split_whitespace().collect();
                if parts.len() >= 4 {
                    let total: u64 = parts[1].parse().unwrap_or(0);
                    let used: u64 = parts[2].parse().unwrap_or(0);
                    let free: u64 = parts[3].parse().unwrap_or(0);
                    return Some(json!({
                        "drive": parts[5],
                        "total": format_gb(total),
                        "used": format_gb(used),
                        "free": format_gb(free),
                    }));
                }
            }
            None
        })
        .unwrap_or(json!({
            "drive": "/",
            "total": "N/A",
            "used": "N/A",
            "free": "N/A",
        }));

    Ok(json!({
        "os": os_info,
        "cpu": {
            "model": cpu_model,
            "cores": cpu_cores,
        },
        "ram": {
            "total": format!("{:.2} GB", total_mem_gb),
            "free": format!("{:.2} GB", free_mem_gb),
            "used": format!("{:.2} GB", total_mem_gb - free_mem_gb),
        },
        "storage": disk_usage,
    }))
}

fn format_gb(bytes: u64) -> String {
    let gb = bytes as f64 / 1_073_741_824.0;
    format!("{:.2} GB", gb)
}

// ============================================================================
// Port implementation — [`MiscRepository`]
// ============================================================================

/// Adapter for the [`MiscRepository`] port. Stateless: the module functions above are
/// already the whole implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct MiscRepositoryImpl;

#[async_trait]
impl MiscRepository for MiscRepositoryImpl {
    async fn currency_converter(
        &self,
        amount: f64,
        from: &str,
        to: &str,
    ) -> Result<Value, ScrapingError> {
        fetch_currency_converter(amount, from, to)
            .await
            .map_err(ScrapingError::Http)
    }
    async fn harga_emas(&self) -> Result<Value, ScrapingError> {
        fetch_harga_emas().await.map_err(ScrapingError::Http)
    }
    async fn kurs_bca(&self) -> Result<Value, ScrapingError> {
        fetch_kurs_bca().await.map_err(ScrapingError::Http)
    }
    async fn server_info(&self) -> Result<Value, ScrapingError> {
        fetch_server_info().await.map_err(ScrapingError::Http)
    }
}
