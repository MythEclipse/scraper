//! Application state shared across all handlers.

use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::events::bus::EventBus;

/// Shared application state injected into every handler via Axum State.
///
/// Contains the infrastructure dependencies that handlers and use cases
/// need to serve requests. Redis is intentionally absent: the cache layer
/// checks a connection out of the process-wide pool per operation
/// ([`crate::infrastructure::cache::mytheclipse`]), so no per-request state
/// needs to carry it.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DatabaseConnection>,
    pub event_bus: Arc<EventBus>,
}

impl AppState {
    pub fn sea_orm(&self) -> &DatabaseConnection {
        &self.db
    }
}
