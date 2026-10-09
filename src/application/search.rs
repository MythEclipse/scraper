//! Application use cases backed by the [`SearchRepository`] port.

use serde_json::Value;

use crate::domain::error::ScrapingError;
use crate::domain::repository::SearchRepository;

pub struct SearchUseCases<R: SearchRepository> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: SearchRepository>(repository: R) -> SearchUseCases<R> {
    SearchUseCases { repository }
}

impl<R: SearchRepository> SearchUseCases<R> {
    pub async fn bmkg(&self) -> Result<Value, ScrapingError> {
        self.repository.bmkg().await
    }

    pub async fn jadwal_sholat(&self, kota: &str) -> Result<Value, ScrapingError> {
        self.repository.jadwal_sholat(kota).await
    }

    pub async fn weather(&self, city: &str) -> Result<Value, ScrapingError> {
        self.repository.weather(city).await
    }

    pub async fn google(&self, query: &str) -> Result<Value, ScrapingError> {
        self.repository.google(query).await
    }

    pub async fn yt_search(&self, query: &str) -> Result<Value, ScrapingError> {
        self.repository.yt_search(query).await
    }
}
