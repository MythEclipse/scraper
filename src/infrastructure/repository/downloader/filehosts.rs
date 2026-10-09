//! //! Direct-link extractors for file hosts: MEGA, TeraBox, MediaFire, Google Drive,
//! PixelDrain, DoodStream, KrakenFiles.

use std::borrow::Cow;
use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;

use crate::domain::entity::downloader::{DownloadResult, MediaItem, MediaType};
use crate::domain::error::ScrapingError;
use crate::infrastructure::utils::http_client::http_client;

use super::patterns::{
    H1, KRAKENFILES_ID, MEDIAFIRE_CDN, MEDIAFIRE_DOWNLOAD_LINK, MEDIAFIRE_ID_PARAM,
    MEDIAFIRE_VIEWER_DATA, MEGA_FILE_ID, PIXELDRAIN_ID, TERABOX_HOST,
};
use super::shared::format_filesize;
use aes::cipher::BlockDecrypt;
use aes::cipher::KeyInit;
use base64::Engine;

/// MEGA.nz — requires AES-128-CBC decryption of the file attributes.
/// Ported from Shirokami's `mega.js` crypto logic.
pub async fn fetch_mega(url: &str) -> Result<DownloadResult, ScrapingError> {
    let url_fixed = url.replace('#', "%23");
    let _parts: Vec<&str> = url_fixed.splitn(2, "#").collect();
    // Actually need to handle the # separator differently
    let cleaned = url.replace("#", "%23");
    let decoded_url = urlencoding::decode(&cleaned)
        .unwrap_or(Cow::Borrowed(&cleaned))
        .into_owned();

    let file_id = MEGA_FILE_ID
        .captures(&decoded_url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let file_id = file_id.ok_or_else(|| ScrapingError::Http("Not found".to_string()))?;

    let parts: Vec<&str> = decoded_url.splitn(2, '#').collect();
    let file_key = parts.get(1).copied().unwrap_or("");

    if file_key.is_empty() || file_key.len() != 43 {
        return Ok(DownloadResult::error(
            if file_key.is_empty() {
                "File key tidak ditemukan"
            } else if file_key.len() < 43 {
                "Not enough character"
            } else {
                "Too many character"
            }
            .to_string(),
        ));
    }

    let client = http_client();
    let headers = {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
        h.insert("origin", HeaderValue::from_static("https://mega.nz"));
        h.insert("referer", HeaderValue::from_static("https://mega.nz"));
        h
    };

    let payload = serde_json::json!([{ "a": "g", "g": 1, "p": file_id }]);

    // Retry loop (3 attempts)
    let mut data = None;
    for attempt in 0..3 {
        match client
            .client()
            .post("https://g.api.mega.co.nz/cs")
            .headers(headers.clone())
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                data = Some(
                    resp.json::<Vec<serde_json::Value>>()
                        .await
                        .unwrap_or_default(),
                );
                break;
            }
            Err(_) if attempt < 2 => continue,
            Err(e) => {
                return Ok(DownloadResult::error(e.to_string()));
            }
        }
    }

    let data = data.unwrap_or_default();
    let first = data.first().unwrap_or(&serde_json::Value::Null);

    let mut attrs = None;
    if let Some(at) = first.get("at") {
        if let Some(dec) = decrypt_mega_attr(at.as_str().unwrap_or(""), file_key) {
            attrs = Some(dec);
        }
    }

    let mut result = DownloadResult::success(
        attrs
            .as_ref()
            .and_then(|a| a.get("n").and_then(|v| v.as_str()).map(|s| s.to_string())),
    );
    result.provider = Some("mega".to_string());
    result.title = attrs
        .as_ref()
        .and_then(|a| a.get("n").and_then(|v| v.as_str()).map(|s| s.to_string()));

    result.media.push(MediaItem {
        url: first
            .get("g")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        quality: None,
        file_type: Some(MediaType::File),
        extension: None,
        thumbnail: None,
        file_size: first.get("s").and_then(|v| v.as_u64()).map(format_filesize),
        size_bytes: first.get("s").and_then(|v| v.as_u64()),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

/// MEGA attribute decryption — AES-128-CBC with the derived key.
fn decrypt_mega_attr(enc: &str, file_key: &str) -> Option<serde_json::Value> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

    let fixed_key = fix_base64_key(file_key);
    let key_buffer = BASE64.decode(&fixed_key).ok()?;
    if key_buffer.len() < 32 {
        return None;
    }

    // Derive 16-byte key from 32-byte file key
    let key_ints: Vec<u32> = (0..8)
        .map(|i| {
            u32::from_le_bytes([
                key_buffer[i * 4],
                key_buffer[i * 4 + 1],
                key_buffer[i * 4 + 2],
                key_buffer[i * 4 + 3],
            ])
        })
        .collect();

    let key_out: [u8; 16] = {
        let ints = [
            key_ints[0] ^ key_ints[4],
            key_ints[1] ^ key_ints[5],
            key_ints[2] ^ key_ints[6],
            key_ints[3] ^ key_ints[7],
        ];
        let mut buf = [0u8; 16];
        for (i, &val) in ints.iter().enumerate() {
            buf[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
        }
        buf
    };

    let iv = [0u8; 16];
    decrypt_aes_128_cbc(&key_out, &iv, enc)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
}

fn fix_base64_key(s: &str) -> String {
    let rem = s.len() % 4;
    if rem > 0 {
        format!("{}{}", s, "=".repeat(4 - rem))
    } else {
        s.to_string()
    }
}

fn decrypt_aes_128_cbc(key: &[u8], _iv: &[u8], enc: &str) -> Option<String> {
    use aes::Aes128;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

    let data = BASE64.decode(enc).ok()?;
    let cipher = Aes128::new_from_slice(key).ok()?;

    let mut result = Vec::new();
    for chunk in data.chunks_exact(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        if let Ok(_) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cipher.decrypt_block((&mut block).into())
        })) {
            result.extend_from_slice(&block);
        } else {
            return None;
        }
    }

    let s = String::from_utf8_lossy(&result).into_owned();
    let s = s.trim_start_matches("MEGA");
    Some(s.to_string())
}

pub async fn fetch_terabox(url: &str) -> Result<DownloadResult, ScrapingError> {
    let pattern = regex::Regex::new(
        r"^https?://(?:www\.|1024)?terabox(?:app)?\.com/.*[?&]?surl=([a-zA-Z0-9_-]+)",
    )
    .unwrap();

    let mut surl = pattern
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    if surl.is_none() {
        // Try resolving redirect
        if !TERABOX_HOST.is_match(url) {
            return Ok(DownloadResult::error("Invalid TeraBox URL."));
        }

        let client = http_client();
        let resp = client
            .client()
            .get(url)
            .header(USER_AGENT, "Mozilla/5.0")
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| ScrapingError::Http(format!("TeraBox redirect fetch failed: {}", e)))?;

        let request_url = resp.url().to_string();
        surl = pattern
            .captures(&request_url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());
    }

    let surl = surl.ok_or_else(|| ScrapingError::Http("SURL not found.".to_string()))?;

    // Call local TeraBox resolver (Playwright + residential proxy + cookies)
    let resolver_url = std::env::var("TERABOX_RESOLVER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4092".to_string());
    let resolve_url = format!("{}/resolve?surl={}", resolver_url, surl);

    let client = http_client();
    let resp = client
        .client()
        .get(&resolve_url)
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("TeraBox resolver fetch failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Ok(DownloadResult::error(format!(
            "TeraBox resolver returned HTTP {}: {}",
            status, body
        )));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ScrapingError::Http(format!("TeraBox resolver JSON parse failed: {}", e)))?;

    if let Some(error) = data.get("error").and_then(|v| v.as_str()) {
        return Ok(DownloadResult::error(error));
    }

    let file_name = data
        .get("file_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut result = DownloadResult::success(file_name.clone());
    result.provider = Some("terabox".to_string());
    result.thumbnail = data
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Primary download link.
    //
    // IMPORTANT: the resolver's download_link is a raw terabox dlink that
    // 403s with {"error_code":31045,"user not exists"} when fetched WITHOUT
    // cookies — even from residential IPs. So we rewrite the public URL to
    // OUR OWN /proxy/terabox streaming endpoint (which forwards the resolver's
    // cookie-authenticated stream through the public API). This is the only
    // way users get a link that actually downloads.
    // The public API base that users can reach. `CONFIG.urls.site_url` is the
    // main site (hub → 4003) — NOT the scraper API domain. Use an explicit
    // env override (TERABOX_PUBLIC_BASE_URL), falling back to the scraper's
    // own public domain via site_url only when it's clearly the API host.
    let public_base = std::env::var("TERABOX_PUBLIC_BASE_URL").unwrap_or_else(|_| {
        let site = crate::config::CONFIG.urls.site_url.clone();
        if site.contains("scraper") || site.contains("api") {
            site
        } else {
            // default to the scraper API subdomain (this API's public host)
            "https://api.asepharyana.my.id".to_string()
        }
    });
    let proxy_dl = format!("{}/proxy/terabox?surl={}", public_base, surl);
    if let Some(download_url) = data.get("download_link").and_then(|v| v.as_str()) {
        let size_bytes = data.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);

        // PRIMARY = the cookie-authenticated public proxy. The raw terabox
        // dlink is only a secondary entry — it 403s without cookies, so it
        // must NOT be the first link users see.
        result.media.push(MediaItem {
            url: proxy_dl.clone(),
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: Some(format_filesize(size_bytes)),
            size_bytes: Some(size_bytes),
            frame_width: None,
            frame_height: None,
            note: Some("Direct download via cookie-authenticated proxy (works).".into()),
        });
        result.media.push(MediaItem {
            url: download_url.to_string(),
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: Some(format_filesize(size_bytes)),
            size_bytes: Some(size_bytes),
            frame_width: None,
            frame_height: None,
            note: Some("Raw CDN link (may require a TeraBox session cookie).".into()),
        });

        // Directory: add child file links
        if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
            for f in files {
                if let Some(furl) = f.get("download_link").and_then(|v| v.as_str()) {
                    let fsize = f.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
                    result.media.push(MediaItem {
                        url: furl.to_string(),
                        quality: None,
                        file_type: Some(MediaType::File),
                        extension: None,
                        thumbnail: None,
                        file_size: Some(format_filesize(fsize)),
                        size_bytes: Some(fsize),
                        frame_width: None,
                        frame_height: None,
                        note: f
                            .get("file_name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
    }

    Ok(result)
}

pub async fn fetch_doodstream(url: &str) -> Result<DownloadResult, ScrapingError> {
    let id = &*PIXELDRAIN_ID
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| ScrapingError::Http("Linknya tidak bisa diproses".to_string()))?;

    let proxy = "https://rv.lil-hacker.workers.dev/proxy?mirror=dood&url=";
    let client = http_client();

    // Get metadata from dood.li page
    let page_html = client
        .client()
        .get(format!("https://dood.li/d/{}", id))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream page fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream page read failed: {}", e)))?;

    // HTML parsing — extract all needed text BEFORE any .await to keep future Send
    // Scraper types (Html/HtmlElementRef/Selector) are NOT Send, so they must be
    // dropped before async points.
    let (page_title, page_length, page_uploadate) = {
        let document = scraper::Html::parse_document(&page_html);
        let text = |sel: &str| {
            document
                .select(&scraper::Selector::parse(sel).unwrap())
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
        };
        (text(".title-wrap h4"), text(".length"), text(".uploadate"))
    };

    // Get embedded player page
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36";
    let headers = {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(ua));
        h.insert("referer", HeaderValue::from_static("https://d000d.com/"));
        h
    };

    let embed_resp = client
        .client()
        .get(format!("{}{}/e/{}", proxy, "https://d000d.com", id))
        .headers(headers.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream embed fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream embed read failed: {}", e)))?;

    let cdn_match = MEDIAFIRE_CDN
        .captures(&embed_resp)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let cdn_path = cdn_match.ok_or_else(|| {
        ScrapingError::Http("Link Pass MD5 tidak ditemukan! Coba lagi nanti.".to_string())
    })?;

    // Generate random token (like JS crypto.randomBytes)
    let chars: String = (0..10)
        .map(|_| {
            let idx = fastrand::usize(..62);
            let c = if idx < 26 {
                (b'A' + idx as u8) as char
            } else if idx < 52 {
                (b'a' + (idx - 26) as u8) as char
            } else {
                (b'0' + (idx - 52) as u8) as char
            };
            c
        })
        .collect();

    let ds_resp = client
        .client()
        .get(format!("{}{}{}", proxy, "https://d000d.com", cdn_path))
        .headers(headers.clone())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream DS fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("DoodStream DS read failed: {}", e)))?;

    let _cm = cdn_path.split('/').last().unwrap_or("");

    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let direct_link = format!(
        "{}{}{}?token={}&expiry={}",
        proxy,
        "https://d000d.com",
        ds_resp.trim(),
        chars,
        expiry,
    );

    // Title/length/uploadate were extracted inside the scope block above
    // as owned strings to keep scraper::Html from crossing .await
    let (title, length_str, uploadate) = (page_title, page_length, page_uploadate);
    let mut result = DownloadResult::success(Some(title));
    result.provider = Some("doodstream".to_string());
    result.duration = Some(length_str);

    result.description = Some(uploadate);

    result.media.push(MediaItem {
        url: direct_link,
        quality: None,
        file_type: Some(MediaType::Video),
        extension: Some("mp4".to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: None,
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_krakenfiles(url: &str) -> Result<DownloadResult, ScrapingError> {
    // Parse file ID from krakenfiles.com URL
    let file_id = &*KRAKENFILES_ID
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or_else(|| ScrapingError::Http("Invalid KrakenFiles URL".to_string()))?;

    let client = http_client();
    let resp = client
        .client()
        .post(format!("https://krakenfiles.com/v/{}", file_id))
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("KrakenFiles fetch failed: {}", e)))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| ScrapingError::Http(format!("KrakenFiles JSON parse failed: {}", e)))?;

    let mut result = DownloadResult::success(
        resp.get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    result.provider = Some("krakenfiles".to_string());

    result.media.push(MediaItem {
        url: resp
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        quality: None,
        file_type: Some(MediaType::File),
        extension: resp
            .get("ext")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        thumbnail: None,
        file_size: None,
        size_bytes: resp.get("size_b").and_then(|v| v.as_u64()),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_pixeldrain(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let html = client
        .client()
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("PixelDrain fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("PixelDrain read failed: {}", e)))?;

    let re = &*MEDIAFIRE_VIEWER_DATA;
    let m = re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str());
    let json_str =
        m.ok_or_else(|| ScrapingError::Http("Failed to retrieve viewer data".to_string()))?;

    let viewer_data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ScrapingError::Http(format!("PixelDrain JSON parse failed: {}", e)))?;

    let file_data = &viewer_data["api_response"];
    let file_id = file_data["id"].as_str().unwrap_or("");

    let mut result = DownloadResult::success(file_data["name"].as_str().map(|s| s.to_string()));
    result.provider = Some("pixeldrain".to_string());

    result.media.push(MediaItem {
        url: format!("https://pixeldrain.com/api/file/{}?download", file_id),
        quality: None,
        file_type: Some(MediaType::File),
        extension: None,
        thumbnail: None,
        file_size: Some(format_filesize(file_data["size"].as_u64().unwrap_or(0))),
        size_bytes: file_data["size"].as_u64(),
        frame_width: None,
        frame_height: None,
        note: None,
    });

    Ok(result)
}

pub async fn fetch_gdrive(url: &str) -> Result<DownloadResult, ScrapingError> {
    let mut result = DownloadResult::error("Invalid Google Drive URL");
    let id_re = &*MEDIAFIRE_ID_PARAM;
    if let Some(caps) = id_re.captures(url) {
        let file_id = caps.get(1).unwrap().as_str();
        let download_url = format!("https://drive.google.com/uc?id={}&export=download", file_id);
        result = DownloadResult::success(None);
        result.provider = Some("gdrive".to_string());
        result.media.push(MediaItem {
            url: download_url,
            quality: None,
            file_type: Some(MediaType::File),
            extension: None,
            thumbnail: None,
            file_size: None,
            size_bytes: None,
            frame_width: None,
            frame_height: None,
            note: None,
        });
    }
    Ok(result)
}

pub async fn fetch_mediafire(url: &str) -> Result<DownloadResult, ScrapingError> {
    let client = http_client();
    let html = client
        .client()
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("accept", "text/html,application/xhtml+xml")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| ScrapingError::Http(format!("MediaFire fetch failed: {}", e)))?
        .text()
        .await
        .map_err(|e| ScrapingError::Http(format!("MediaFire response read failed: {}", e)))?;

    let document = scraper::Html::parse_document(&html);
    let dl_sel = &*MEDIAFIRE_DOWNLOAD_LINK;
    let download_url = document
        .select(&dl_sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.to_string());

    let title = {
        let title_sel = &*H1;
        document
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<String>())
    };

    Ok(DownloadResult {
        title,
        status: crate::domain::entity::downloader::DownloadStatus::Success,
        media: download_url
            .map(|u| {
                vec![MediaItem {
                    url: u,
                    quality: None,
                    file_type: Some(MediaType::File),
                    extension: None,
                    thumbnail: None,
                    file_size: None,
                    size_bytes: None,
                    frame_width: None,
                    frame_height: None,
                    note: None,
                }]
            })
            .unwrap_or_default(),
        provider: Some("mediafire".to_string()),
        author: None,
        thumbnail: None,
        description: None,
        duration: None,
        message: None,
    })
}
