//! Application use cases backed by the [`ToolsRepository`] port.

use serde_json::Value;

use crate::domain::error::ScrapingError;
use crate::domain::repository::ToolsRepository;

pub struct ToolsUseCases<R: ToolsRepository> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: ToolsRepository>(repository: R) -> ToolsUseCases<R> {
    ToolsUseCases { repository }
}

impl<R: ToolsRepository> ToolsUseCases<R> {
    pub async fn whois(&self, domain: &str) -> Result<Value, ScrapingError> {
        self.repository.whois(domain).await
    }

    pub async fn ip_location(&self, ip: &str) -> Result<Value, ScrapingError> {
        self.repository.ip_location(ip).await
    }

    pub async fn tinyurl(&self, url: &str) -> Result<Value, ScrapingError> {
        self.repository.tinyurl(url).await
    }

    pub async fn check_hosting(&self, domain: &str) -> Result<Value, ScrapingError> {
        self.repository.check_hosting(domain).await
    }

    pub async fn hargapangan(&self) -> Result<Value, ScrapingError> {
        self.repository.hargapangan().await
    }

    pub async fn cek_resi(
        &self,
        resi: String,
        ekspedisi: Option<String>,
    ) -> Result<Value, ScrapingError> {
        self.repository.cek_resi(resi, ekspedisi).await
    }
}
