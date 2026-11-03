//! Application use cases backed by the [`ImageRepository`] port.

use crate::domain::error::ScrapingError;
use crate::domain::repository::ImageRepository;

pub struct ImageUseCases<R: ImageRepository> {
    repository: R,
}

/// Wires the port implementation chosen by the composition root.
pub fn new_use_cases<R: ImageRepository>(repository: R) -> ImageUseCases<R> {
    ImageUseCases { repository }
}

impl<R: ImageRepository> ImageUseCases<R> {
    pub async fn brat(&self, text: &str) -> Result<Vec<u8>, ScrapingError> {
        self.repository.brat(text).await
    }

    pub async fn brat_animated(&self, text: &str) -> Result<Vec<u8>, ScrapingError> {
        self.repository.brat_animated(text).await
    }
}
