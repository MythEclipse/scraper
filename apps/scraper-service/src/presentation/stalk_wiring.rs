//! Composition root for the stalk handlers.
//!
//! The handlers call these helpers by module path, so this is where the
//! concrete [`StalkRepository`] implementation gets chosen — the application layer
//! only ever sees the port.

use serde_json::Value;

use crate::application::stalk::new_use_cases;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::stalk::StalkRepositoryImpl;

pub async fn github(username: &str) -> Result<Value, ScrapingError> {
    new_use_cases(StalkRepositoryImpl).github(username).await
}

pub async fn youtube(username: &str) -> Result<Value, ScrapingError> {
    new_use_cases(StalkRepositoryImpl).youtube(username).await
}

pub async fn twitter(username: &str) -> Result<Value, ScrapingError> {
    new_use_cases(StalkRepositoryImpl).twitter(username).await
}
