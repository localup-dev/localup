//! Add OAuth provider fields to users table for social login support

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add oauth_provider column (e.g., "google", "github", null for password auth)
        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .add_column(ColumnDef::new(User::OauthProvider).string_len(32).null())
                    .to_owned(),
            )
            .await?;

        // Add oauth_provider_id column (provider's user ID)
        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .add_column(ColumnDef::new(User::OauthProviderId).string_len(255).null())
                    .to_owned(),
            )
            .await?;

        // Make password_hash nullable (social login users won't have passwords)
        // Note: SQLite doesn't support ALTER COLUMN, so we skip this for SQLite
        // The column is already TEXT which accepts null in practice
        // For PostgreSQL, we would do:
        // ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;

        // Create unique index on (oauth_provider, oauth_provider_id) for lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_oauth_provider_id")
                    .table(User::Table)
                    .col(User::OauthProvider)
                    .col(User::OauthProviderId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_users_oauth_provider_id")
                    .table(User::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .drop_column(User::OauthProviderId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .drop_column(User::OauthProvider)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum User {
    #[sea_orm(iden = "users")]
    Table,
    OauthProvider,
    OauthProviderId,
}
