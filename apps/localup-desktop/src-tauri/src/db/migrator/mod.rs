//! Database migrations for LocalUp Desktop

use sea_orm_migration::prelude::*;

mod m20260103_000001_init_schema;
mod m20260104_000001_add_supported_protocols;
mod m20260108_000001_add_ip_allowlist;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260103_000001_init_schema::Migration),
            Box::new(m20260104_000001_add_supported_protocols::Migration),
            Box::new(m20260108_000001_add_ip_allowlist::Migration),
        ]
    }
}
