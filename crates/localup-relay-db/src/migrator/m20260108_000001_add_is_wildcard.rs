//! Migration to add is_wildcard column to custom_domains table
//! This enables wildcard domain support (e.g., *.example.com)

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add is_wildcard column with default false
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .add_column(
                        ColumnDef::new(CustomDomains::IsWildcard)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // Add index on is_wildcard for efficient wildcard domain queries
        manager
            .create_index(
                Index::create()
                    .name("idx_custom_domains_is_wildcard")
                    .table(CustomDomains::Table)
                    .col(CustomDomains::IsWildcard)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop index first
        manager
            .drop_index(
                Index::drop()
                    .name("idx_custom_domains_is_wildcard")
                    .table(CustomDomains::Table)
                    .to_owned(),
            )
            .await?;

        // Remove is_wildcard column
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .drop_column(CustomDomains::IsWildcard)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum CustomDomains {
    Table,
    IsWildcard,
}
