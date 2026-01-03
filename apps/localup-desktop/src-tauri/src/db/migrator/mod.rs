//! Database migrations for LocalUp Desktop

use sea_orm_migration::prelude::*;

mod m20260103_000001_init_schema;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260103_000001_init_schema::Migration)]
    }
}
