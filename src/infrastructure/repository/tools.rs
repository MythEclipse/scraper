//! Infrastructure — Tool utilities.
//!
//! Ported from Shirokami-API `scraper/tool/*.js` and `scraper/tool/*.js`:
//! whois, ip-location, tinyurl, check-hosting, cek-resi, hargapangan.

use async_trait::async_trait;

use crate::domain::error::ScrapingError;
use crate::domain::repository::ToolsRepository;

use crate::infrastructure::utils::http_client::http_client;
use regex::Regex;
use reqwest::header::USER_AGENT;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use urlencoding::encode as urlencode;

const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// WHOIS — RDAP lookup (whois.com HTML `.df-raw` is dead/JS-rendered since 2026;
// RDAP is the structured, reliable server-side source for the same data).
// ---------------------------------------------------------------------------

pub async fn fetch_whois(domain: &str) -> Result<Value, String> {
    let domain = domain.to_lowercase();
    let tld = domain.rsplit('.').next().unwrap_or("com");
    let base = match tld {
        "com" | "net" => "https://rdap.verisign.com",
        "org" => "https://rdap.org",
        "io" => "https://rdap.identitydigital.services",
        "co" => "https://rdap.nic.co",
        "id" => "https://rdap.idnnic.id",
        _ => "https://rdap.org", // generic bootstrap
    };
    let url = if base == "https://rdap.verisign.com" {
        format!("{}/{}/v1/domain/{}", base, tld, domain)
    } else {
        format!("{}/domain/{}", base, domain)
    };
    let resp = http_client()
        .client()
        .get(&url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(
            json!({"domain": domain, "available": true, "valid": true, "data": "No data", "rawData": "No data"}),
        );
    }
    if !resp.status().is_success() {
        return Ok(
            json!({"domain": domain, "valid": false, "data": "RDAP unavailable", "rawData": "No data"}),
        );
    }
    let data: Value = resp.json().await.map_err(|e| format!("JSON: {}", e))?;

    let event_date = |action: &str| -> String {
        data.get("events")
            .and_then(|e| e.as_array())
            .map(|events| {
                events
                    .iter()
                    .find(|ev| ev.get("eventAction").and_then(|a| a.as_str()) == Some(action))
                    .and_then(|ev| ev.get("eventDate").and_then(|d| d.as_str()))
                    .unwrap_or("No data")
                    .to_string()
            })
            .unwrap_or_else(|| "No data".to_string())
    };

    let domain_name = data
        .get("ldhName")
        .and_then(|v| v.as_str())
        .unwrap_or(&domain)
        .to_string();
    let domain_id = data
        .get("handle")
        .and_then(|v| v.as_str())
        .unwrap_or("No data")
        .to_string();
    let creation = event_date("registration");
    let expiration = event_date("expiration");
    let updated = event_date("last changed");

    // Registrar name from the registrar entity's vcardArray
    let registrar = data
        .get("entities")
        .and_then(|e| e.as_array())
        .map(|entities| {
            entities
                .iter()
                .find(|en| {
                    en.get("roles")
                        .and_then(|r| r.as_array())
                        .map(|roles| roles.iter().any(|ro| ro.as_str() == Some("registrar")))
                        .unwrap_or(false)
                })
                .and_then(|reg| {
                    reg.get("vcardArray")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.get(1))
                        .and_then(|props| props.as_array())
                        .and_then(|props| {
                            props.iter().find_map(|p| {
                                let pa = p.as_array()?;
                                if pa.first()?.as_str() == Some("fn") {
                                    pa.get(3).and_then(|v| v.as_str()).map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                        })
                })
                .unwrap_or_else(|| "No data".to_string())
        })
        .unwrap_or_else(|| "No data".to_string());

    Ok(json!({
        "domain": domain,
        "available": false,
        "valid": true,
        "data": {
            "domainName": domain_name,
            "registryDomainId": domain_id,
            "creationDate": creation,
            "expirationDate": expiration,
            "updatedDate": updated,
            "country": "No data",
            "registrar": registrar,
        },
        "rawData": serde_json::to_string(&data).unwrap_or_default(),
    }))
}

// ---------------------------------------------------------------------------
// IP Location — ipapi.co JSON
// ---------------------------------------------------------------------------

pub async fn fetch_ip_location(ip: &str) -> Result<Value, String> {
    let url = format!("https://ipapi.co/{}/json/", ip);
    let resp = http_client()
        .client()
        .get(&url)
        .header(USER_AGENT, "nodejs-ipapi-v1.02")
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    resp.json().await.map_err(|e| format!("JSON: {}", e))
}

// ---------------------------------------------------------------------------
// TinyURL — tinyurl.com API
// ---------------------------------------------------------------------------

pub async fn fetch_tinyurl(url: &str) -> Result<Value, String> {
    let api_url = format!("https://tinyurl.com/api-create.php?url={}", urlencode(url));
    let resp = http_client()
        .client()
        .get(&api_url)
        .header(USER_AGENT, CHROME_UA)
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    let short_url = resp.text().await.map_err(|e| format!("Body: {}", e))?;
    Ok(json!({"shortUrl": short_url.trim()}))
}

// ---------------------------------------------------------------------------
// Check Hosting — hosting-checker.net API
// ---------------------------------------------------------------------------

pub async fn fetch_check_hosting(domain: &str) -> Result<Value, String> {
    let cd = domain.to_lowercase().trim().to_string();
    // Strip scheme and www
    let cd = cd
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .to_string();
    let domain_re =
        Regex::new(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap();
    if !domain_re.is_match(&cd) {
        return Err("Invalid domain format".into());
    }
    let url = format!("https://hosting-checker.net/api/hosting/{}", cd);
    let resp = http_client()
        .client()
        .get(&url)
        .header("authority", "hosting-checker.net")
        .header(USER_AGENT, "Postify/1.0.0")
        .header(
            "referer",
            format!("https://hosting-checker.net/websites/{}", cd),
        )
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let data: Value = resp.json().await.map_err(|e| format!("JSON: {}", e))?;
    // Pass through the relevant fields (same structure as JS version)
    Ok(json!({
        "domain": {
            "name": data["web"]["domain"],
            "original": data["web"]["originalDomain"],
            "ipv6_support": data["web"]["ipV6Support"],
        },
        "web": {
            "ips": data["web"]["lookups"],
            "providers": data["web"]["providers"],
        },
        "nameserver": data["nameserver"],
        "mail": {"incoming": data["incomingMail"], "outgoing": data["outgoingMail"]},
    }))
}

// ---------------------------------------------------------------------------
// Harga Pangan (BAPANAS) — Indonesian government food prices
// ---------------------------------------------------------------------------

const BAPANAS_KEY: &str = "zHWbt7U2qvPoUDkiUgvnOqYrtj3zClR7unnH2G4apE7HcMV4QyNC6BSD0yV3uvSHqS91TxwE8aMDTiCznmGceEX3zQmO1Xwq7TJblotIt2CpwvK6YjRKDJwcgMJwav9p4RshM3nfuFyurSQQv9BhueMJ0HJ778oD";

pub async fn fetch_hargapangan() -> Result<Value, String> {
    let url = "https://api-panelhargav2.badanpangan.go.id/api/front/harga-pangan-informasi";
    let resp = http_client()
        .client()
        .get(url)
        .header(USER_AGENT, CHROME_UA)
        .header("accept", "application/json")
        .header("x-api-key", BAPANAS_KEY)
        .header("origin", "https://panelharga.badanpangan.go.id")
        .header("referer", "https://panelharga.badanpangan.go.id/")
        .query(&[("level_harga_id", "1")])
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let data: Value = resp.json().await.map_err(|e| format!("JSON: {}", e))?;
    if data.get("status").and_then(|s| s.as_str()) == Some("success") {
        Ok(data.get("data").cloned().unwrap_or(Value::Null))
    } else {
        Ok(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// Cek Resi (cekresi.com) — Indonesian package tracking
// ---------------------------------------------------------------------------

fn aes_encrypt_resi(resi: &str) -> String {
    use aes::cipher::generic_array::GenericArray;
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
    let key_hex = "79540e250fdb16afac03e19c46dbdeb3";
    let iv_hex = "eb2bb9425e81ffa942522e4414e95bd0";
    let key_bytes = hex::decode(key_hex).expect("bad key hex");
    let iv_bytes = hex::decode(iv_hex).expect("bad iv hex");
    let key = GenericArray::from_slice(&key_bytes);
    let iv = GenericArray::from_slice(&iv_bytes);

    let data = resi.as_bytes();
    let mut buf = vec![0u8; data.len() + 16];
    buf[..data.len()].copy_from_slice(data);
    let ct = Aes128CbcEnc::new(key, iv)
        .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
        .expect("encrypt");
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, ct)
}

fn detect_ekspedisi(resi: &str) -> Option<&'static str> {
    let upper = resi.to_uppercase();
    let map = [
        ("jne", "JNE"),
        ("pos", "POS"),
        ("tiki", "TIKI"),
        ("jnt", "J&T"),
        ("sicepat", "SICEPAT"),
        ("anteraja", "ANTERAJA"),
        ("ninja", "NINJA"),
        ("lionparcel", "LIONPARCEL"),
    ];
    for (prefix, code) in map {
        if upper.starts_with(code) {
            return Some(prefix);
        }
    }
    None
}

pub async fn fetch_cek_resi(noresi: &str, ekspedisi: Option<&str>) -> Result<Value, String> {
    let noresi = noresi.trim().to_uppercase();
    let ekspedisi = match ekspedisi {
        Some(e) => e.to_string(),
        None => detect_ekspedisi(&noresi)
            .ok_or_else(|| "Ekspedisi tidak ditemukan dari nomor resi".to_string())?
            .to_string(),
    };
    let timers = aes_encrypt_resi(&noresi);

    // 1. GET the form to extract viewstate + secret_key
    let client = http_client().client();
    let html = client
        .get("https://cekresi.com/")
        .header(USER_AGENT, CHROME_UA)
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Body: {}", e))?;
    let (viewstate, secret_key) = {
        let doc = Html::parse_document(&html);
        let input_sel = Selector::parse(r#"input[name="viewstate"]"#).unwrap();
        let secret_sel = Selector::parse(r#"input[name="secret_key"]"#).unwrap();
        let vs = doc
            .select(&input_sel)
            .next()
            .and_then(|e| e.value().attr("value"))
            .unwrap_or("")
            .to_string();
        let sk = doc
            .select(&secret_sel)
            .next()
            .and_then(|e| e.value().attr("value"))
            .unwrap_or("")
            .to_string();
        (vs, sk)
    };

    // 2. POST the tracking request
    let w = &format!("{:x}", fastrand::u32(..));
    let post_url = format!(
        "https://apa2.cekresi.com/cekresi/resi/initialize.php?ui=e0ad7e971ce77822056ba7a155f85c11&p=1&w={}",
        w
    );
    let form = [
        ("viewstate", viewstate.as_str()),
        ("secret_key", secret_key.as_str()),
        ("e", &ekspedisi),
        ("noresi", &noresi),
        ("timers", &timers),
    ];
    let resp = client.post(&post_url)
        .header(USER_AGENT, "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Mobile Safari/537.36")
        .header("referer", "https://cekresi.com/")
        .header("origin", "https://cekresi.com")
        .form(&form)
        .send().await.map_err(|e| format!("HTTP: {}", e))?;
    let body = resp.text().await.map_err(|e| format!("Body: {}", e))?;
    let doc2 = Html::parse_document(&body);

    // 3. Parse results
    let alert_sel = Selector::parse(".alert.alert-success").unwrap();
    let success = doc2.select(&alert_sel).next().is_some();

    if success {
        let message = doc2
            .select(&alert_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let table_sel = Selector::parse("table.table-striped tbody tr").unwrap();
        let mut status = String::new();
        let mut tanggal = String::new();
        for row in doc2.select(&table_sel) {
            let tds: Vec<_> = row.select(&Selector::parse("td").unwrap()).collect();
            if tds.len() >= 3 {
                let label = tds[0].text().collect::<String>().trim().to_string();
                let value = tds[2].text().collect::<String>().trim().to_string();
                if label == "Tanggal Pengiriman" {
                    tanggal = value;
                } else if label == "Status" {
                    status = value;
                }
            }
        }
        let last_pos = Selector::parse("#last_position").unwrap();
        let last_position = doc2
            .select(&last_pos)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        // History: scan all tables for the one whose rows have exactly 2 cells
        // (tanggal, keterangan). The status table above uses 3 cells, so the
        // 2-cell table after the "History" heading is unambiguous.
        let mut history: Vec<Value> = Vec::new();
        let table_sel_all = Selector::parse("table").unwrap();
        let td_sel = Selector::parse("td").unwrap();
        for table in doc2.select(&table_sel_all) {
            let mut rows: Vec<Value> = Vec::new();
            for tr in table.select(&Selector::parse("tbody tr").unwrap()) {
                let tds: Vec<_> = tr.select(&td_sel).collect();
                if tds.len() == 2 {
                    rows.push(json!({
                        "tanggal": tds[0].text().collect::<String>().trim(),
                        "keterangan": tds[1].text().collect::<String>().trim(),
                    }));
                }
            }
            // A table with 2-cell rows is the history table; skip its header row.
            if rows.len() > 1 {
                history = rows.into_iter().skip(1).collect();
                break;
            }
        }

        Ok(json!({
            "success": true,
            "message": message,
            "data": {
                "resi": noresi,
                "ekspedisi": doc2.select(&Selector::parse("#nama_expedisi").unwrap()).next()
                    .map(|e| e.text().collect::<String>().trim().to_string()).unwrap_or_default(),
                "status": status,
                "tanggalKirim": tanggal,
                "lastPosition": last_position,
                "history": history,
            }
        }))
    } else {
        let alert_err = Selector::parse(".alert.alert-danger, .alert.alert-warning").unwrap();
        let message = doc2
            .select(&alert_err)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Tidak dapat mengambil informasi resi".to_string());
        Ok(json!({"success": false, "message": message}))
    }
}

// ============================================================================
// Port implementation — [`ToolsRepository`]
// ============================================================================

/// Adapter for the [`ToolsRepository`] port. Stateless: the module functions above are
/// already the whole implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolsRepositoryImpl;

#[async_trait]
impl ToolsRepository for ToolsRepositoryImpl {
    async fn whois(&self, domain: &str) -> Result<Value, ScrapingError> {
        fetch_whois(domain).await.map_err(ScrapingError::Http)
    }
    async fn ip_location(&self, ip: &str) -> Result<Value, ScrapingError> {
        fetch_ip_location(ip).await.map_err(ScrapingError::Http)
    }
    async fn tinyurl(&self, url: &str) -> Result<Value, ScrapingError> {
        fetch_tinyurl(url).await.map_err(ScrapingError::Http)
    }
    async fn check_hosting(&self, domain: &str) -> Result<Value, ScrapingError> {
        fetch_check_hosting(domain)
            .await
            .map_err(ScrapingError::Http)
    }
    async fn hargapangan(&self) -> Result<Value, ScrapingError> {
        fetch_hargapangan().await.map_err(ScrapingError::Http)
    }
    async fn cek_resi(
        &self,
        resi: String,
        ekspedisi: Option<String>,
    ) -> Result<Value, ScrapingError> {
        fetch_cek_resi(&resi, ekspedisi.as_deref())
            .await
            .map_err(ScrapingError::Http)
    }
}
