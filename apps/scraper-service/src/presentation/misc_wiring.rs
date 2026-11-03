//! Composition root for the misc handlers.
//!
//! The handlers call these helpers by module path, so this is where the
//! concrete [`MiscRepository`] implementation gets chosen — the application layer
//! only ever sees the port.

use serde_json::Value;

use crate::application::misc::new_use_cases;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::misc::MiscRepositoryImpl;

pub async fn currency_converter(amount: f64, from: &str, to: &str) -> Result<Value, ScrapingError> {
    new_use_cases(MiscRepositoryImpl)
        .currency_converter(amount, from, to)
        .await
}

pub async fn harga_emas() -> Result<Value, ScrapingError> {
    new_use_cases(MiscRepositoryImpl).harga_emas().await
}

pub async fn kurs_bca() -> Result<Value, ScrapingError> {
    new_use_cases(MiscRepositoryImpl).kurs_bca().await
}

pub async fn server_info() -> Result<Value, ScrapingError> {
    new_use_cases(MiscRepositoryImpl).server_info().await
}
