//! Composition root for the tools handlers.
//!
//! The handlers call these helpers by module path, so this is where the
//! concrete [`ToolsRepository`] implementation gets chosen — the application layer
//! only ever sees the port.

use serde_json::Value;

use crate::application::tools::new_use_cases;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::tools::ToolsRepositoryImpl;

pub async fn whois(domain: &str) -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl).whois(domain).await
}

pub async fn ip_location(ip: &str) -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl).ip_location(ip).await
}

pub async fn tinyurl(url: &str) -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl).tinyurl(url).await
}

pub async fn check_hosting(domain: &str) -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl)
        .check_hosting(domain)
        .await
}

pub async fn hargapangan() -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl).hargapangan().await
}

pub async fn cek_resi(resi: String, ekspedisi: Option<String>) -> Result<Value, ScrapingError> {
    new_use_cases(ToolsRepositoryImpl)
        .cek_resi(resi, ekspedisi)
        .await
}
