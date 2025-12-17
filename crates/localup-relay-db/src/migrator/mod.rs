//! Database migrations

use sea_orm_migration::prelude::*;

mod m20250117_999999_init_schema;
mod m20250118_000001_make_tcp_fields_nullable;
mod m20251216_000001_create_domain_challenges;
mod m20251216_000002_make_cert_paths_nullable;
mod m20251216_000003_add_domain_id;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250117_999999_init_schema::Migration),
            Box::new(m20250118_000001_make_tcp_fields_nullable::Migration),
            Box::new(m20251216_000001_create_domain_challenges::Migration),
            Box::new(m20251216_000002_make_cert_paths_nullable::Migration),
            Box::new(m20251216_000003_add_domain_id::Migration),
        ]
    }
}
