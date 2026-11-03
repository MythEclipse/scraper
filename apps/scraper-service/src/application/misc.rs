//! Application use cases backed by the [`MiscRepository`] port.

use serde_json::Value;

use crate::domain::error::ScrapingError;
use crate::domain::repository::MiscRepository;

pub struct MiscUseCases<R: MiscRepository> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: MiscRepository>(repository: R) -> MiscUseCases<R> {
    MiscUseCases { repository }
}

impl<R: MiscRepository> MiscUseCases<R> {
    pub async fn currency_converter(
        &self,
        amount: f64,
        from: &str,
        to: &str,
    ) -> Result<Value, ScrapingError> {
        self.repository.currency_converter(amount, from, to).await
    }

    pub async fn harga_emas(&self) -> Result<Value, ScrapingError> {
        self.repository.harga_emas().await
    }

    pub async fn kurs_bca(&self) -> Result<Value, ScrapingError> {
        self.repository.kurs_bca().await
    }

    pub async fn server_info(&self) -> Result<Value, ScrapingError> {
        self.repository.server_info().await
    }
}
