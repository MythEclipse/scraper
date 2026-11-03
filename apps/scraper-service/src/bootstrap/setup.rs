//! Database schema initialization.

use sea_orm::DatabaseConnection;
use tracing::info;

pub async fn init(_db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    info!("✅ Database schema initialization complete (no tables to create).");
    Ok(())
}
