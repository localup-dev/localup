//! Create device_authorizations table for OAuth 2.0 Device Authorization Grant (RFC 8628)

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DeviceAuthorization::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DeviceAuthorization::Id)
                            .string_len(255)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::DeviceCode)
                            .string_len(255)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::UserCode)
                            .string_len(16)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::ClientId)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::Scope)
                            .string_len(1024)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::Status)
                            .string_len(32)
                            .not_null()
                            .default("pending"),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::UserId)
                            .string_len(255)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::PollingInterval)
                            .integer()
                            .not_null()
                            .default(5),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::LastPolledAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(DeviceAuthorization::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on device_code for token polling lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_device_authorizations_device_code")
                    .table(DeviceAuthorization::Table)
                    .col(DeviceAuthorization::DeviceCode)
                    .to_owned(),
            )
            .await?;

        // Index on user_code for verification page lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_device_authorizations_user_code")
                    .table(DeviceAuthorization::Table)
                    .col(DeviceAuthorization::UserCode)
                    .to_owned(),
            )
            .await?;

        // Index on expires_at for cleanup queries
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_device_authorizations_expires_at")
                    .table(DeviceAuthorization::Table)
                    .col(DeviceAuthorization::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DeviceAuthorization::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum DeviceAuthorization {
    #[sea_orm(iden = "device_authorizations")]
    Table,
    Id,
    DeviceCode,
    UserCode,
    ClientId,
    Scope,
    Status,
    UserId,
    PollingInterval,
    LastPolledAt,
    ExpiresAt,
    CreatedAt,
}
