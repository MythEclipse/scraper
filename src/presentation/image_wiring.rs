//! Composition root for the image handlers.
//!
//! The handlers call these helpers by module path, so this is where the
//! concrete [`ImageRepository`] implementation gets chosen — the application layer
//! only ever sees the port.

use crate::application::image::new_use_cases;
use crate::domain::error::ScrapingError;
use crate::infrastructure::repository::image::ImageRepositoryImpl;

pub async fn brat(text: &str) -> Result<Vec<u8>, ScrapingError> {
    new_use_cases(ImageRepositoryImpl).brat(text).await
}

pub async fn brat_animated(text: &str) -> Result<Vec<u8>, ScrapingError> {
    new_use_cases(ImageRepositoryImpl).brat_animated(text).await
}
