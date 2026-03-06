//! Database migrations

use sea_orm_migration::prelude::*;

mod m20250117_999999_init_schema;
mod m20250118_000001_make_tcp_fields_nullable;
mod m20251216_000001_create_domain_challenges;
mod m20251216_000002_make_cert_paths_nullable;
mod m20251216_000003_add_domain_id;
mod m20260102_000001_add_cert_pem_columns;
mod m20260108_000001_add_is_wildcard;
mod m20260222_000001_add_oauth_fields;
mod m20260222_000002_create_magic_link_tokens;
mod m20260222_000003_create_device_authorizations;

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
            Box::new(m20260102_000001_add_cert_pem_columns::Migration),
            Box::new(m20260108_000001_add_is_wildcard::Migration),
            Box::new(m20260222_000001_add_oauth_fields::Migration),
            Box::new(m20260222_000002_create_magic_link_tokens::Migration),
            Box::new(m20260222_000003_create_device_authorizations::Migration),
        ]
    }
}
