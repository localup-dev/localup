//! Database layer for LocalUp Desktop
//!
//! Uses SQLite for local persistence of:
//! - Relay server configurations
//! - Tunnel configurations
//! - Tunnel sessions (runtime state)
//! - Captured requests (traffic inspection)
//! - App settings

pub mod entities;
pub mod migrator;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbErr};
use tracing::info;

/// Initialize database connection
///
/// Uses SQLite stored in the app data directory
pub async fn connect(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let db = Database::connect(database_url).await?;

    let backend = db.get_database_backend();
    info!("Connected to database backend: {:?}", backend);

    Ok(db)
}

/// Run migrations
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm_migration::MigratorTrait;

    info!("Running database migrations...");
    migrator::Migrator::up(db, None).await?;
    info!("Database migrations completed");

    Ok(())
}

/// Get database URL for app data directory
pub fn get_database_url(app_data_dir: &std::path::Path) -> String {
    let db_path = app_data_dir.join("localup.db");
    format!("sqlite://{}?mode=rwc", db_path.display())
}
