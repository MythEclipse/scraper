//! Application use cases backed by the [`StalkRepository`] port.

use serde_json::Value;

use crate::domain::error::ScrapingError;
use crate::domain::repository::StalkRepository;

pub struct StalkUseCases<R: StalkRepository> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: StalkRepository>(repository: R) -> StalkUseCases<R> {
    StalkUseCases { repository }
}

impl<R: StalkRepository> StalkUseCases<R> {
    pub async fn github(&self, username: &str) -> Result<Value, ScrapingError> {
        self.repository.github(username).await
    }

    pub async fn youtube(&self, username: &str) -> Result<Value, ScrapingError> {
        self.repository.youtube(username).await
    }

    pub async fn twitter(&self, username: &str) -> Result<Value, ScrapingError> {
        self.repository.twitter(username).await
    }
}
