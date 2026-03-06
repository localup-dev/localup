//! Create magic_link_tokens table for passwordless email login

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MagicLinkToken::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MagicLinkToken::Id)
                            .string_len(255)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(MagicLinkToken::Email)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MagicLinkToken::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MagicLinkToken::UsedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(MagicLinkToken::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on email for lookups when sending
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_magic_link_tokens_email")
                    .table(MagicLinkToken::Table)
                    .col(MagicLinkToken::Email)
                    .to_owned(),
            )
            .await?;

        // Index on expires_at for cleanup queries
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_magic_link_tokens_expires_at")
                    .table(MagicLinkToken::Table)
                    .col(MagicLinkToken::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MagicLinkToken::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum MagicLinkToken {
    #[sea_orm(iden = "magic_link_tokens")]
    Table,
    Id,
    Email,
    ExpiresAt,
    UsedAt,
    CreatedAt,
}
