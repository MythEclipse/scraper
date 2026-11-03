//! Composition root for the search handlers.
//!
//! The handlers call these helpers by module path, so this is where the
//! concrete [`SearchRepository`] implementation gets chosen — the application layer
//! only ever sees the port.

use serde_json::Value;

use crate::application::search::new_use_cases;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::search::SearchRepositoryImpl;

pub async fn bmkg() -> Result<Value, ScrapingError> {
    new_use_cases(SearchRepositoryImpl).bmkg().await
}

pub async fn jadwal_sholat(kota: &str) -> Result<Value, ScrapingError> {
    new_use_cases(SearchRepositoryImpl)
        .jadwal_sholat(kota)
        .await
}

pub async fn weather(city: &str) -> Result<Value, ScrapingError> {
    new_use_cases(SearchRepositoryImpl).weather(city).await
}

pub async fn google(query: &str) -> Result<Value, ScrapingError> {
    new_use_cases(SearchRepositoryImpl).google(query).await
}

pub async fn yt_search(query: &str) -> Result<Value, ScrapingError> {
    new_use_cases(SearchRepositoryImpl).yt_search(query).await
}
