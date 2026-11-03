//! Event bus — delegated to `mytheclipse_event`.
//!
//! The scraper uses `TypedEventBus<InMemoryEventBus>` from the mytheclipse
//! event crate (JSON-typed pub/sub over an in-process broadcast bus).
//! Domain events are plain serde structs; `mytheclipse_event::Event` is a
//! blanket trait, so every `Serialize + DeserializeOwned + Send + Sync + 'static`
//! type is automatically an event — no manual impl needed.

use mytheclipse_event::{InMemoryEventBus, TypedEventBus};

/// Event payload marker — re-exported so domain types can reference it.
pub use mytheclipse_event::Event;

/// The scraper's application event bus.
pub type EventBus = TypedEventBus<InMemoryEventBus>;

/// Build a new in-memory event bus.
pub fn new_event_bus() -> EventBus {
    TypedEventBus::new(InMemoryEventBus::default())
}

// Common events
/// User registered event.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserRegistered {
    pub user_id: String,
    pub email: String,
    pub name: String,
}

/// User logged in event.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserLoggedIn {
    pub user_id: String,
    pub ip_address: Option<String>,
}

/// Order created event.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OrderCreated {
    pub order_id: String,
    pub user_id: String,
    pub total: f64,
}
