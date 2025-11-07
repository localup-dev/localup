//! Database migrations

use sea_orm_migration::prelude::*;

mod m20250115_000001_create_captured_requests_table;
mod m20250115_000002_create_captured_tcp_connections_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250115_000001_create_captured_requests_table::Migration),
            Box::new(m20250115_000002_create_captured_tcp_connections_table::Migration),
        ]
    }
}
