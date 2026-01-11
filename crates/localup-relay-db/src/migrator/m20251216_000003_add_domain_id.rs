//! Migration to add UUID id column to custom_domains table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add id column (UUID as text for SQLite compatibility)
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .add_column(
                        ColumnDef::new(CustomDomains::Id).string_len(36).null(), // Initially nullable for existing rows
                    )
                    .to_owned(),
            )
            .await?;

        // Generate UUIDs for existing rows
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            UPDATE custom_domains
            SET id = lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
                    substr(hex(randomblob(2)),2) || '-' ||
                    substr('89ab', abs(random()) % 4 + 1, 1) ||
                    substr(hex(randomblob(2)),2) || '-' || hex(randomblob(6)))
            WHERE id IS NULL
            "#,
        )
        .await?;

        // Create index on id
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_domains_id")
                    .table(CustomDomains::Table)
                    .col(CustomDomains::Id)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .drop_column(CustomDomains::Id)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum CustomDomains {
    Table,
    Id,
}
