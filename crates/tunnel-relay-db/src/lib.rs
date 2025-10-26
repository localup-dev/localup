//! Database layer for tunnel request/response storage
//!
//! Supports multiple backends:
//! - **PostgreSQL with TimescaleDB** (recommended for production exit nodes)
//! - **PostgreSQL** (production exit nodes without TimescaleDB)
//! - **SQLite3** (development, testing, or lightweight deployments)
//! - **SQLite3 in-memory** (ephemeral storage for clients: "sqlite::memory:")

pub mod entities;
pub mod migrator;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbErr};
use tracing::info;

/// Initialize database connection
///
/// # Examples
/// - Exit node (PostgreSQL): `"postgres://user:pass@localhost/tunnel_db"`
/// - Exit node (SQLite): `"sqlite://./tunnel.db?mode=rwc"`
/// - Client (ephemeral): `"sqlite::memory:"`
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
    info!("✅ Database migrations completed");

    Ok(())
}
