//! Migration to create domain_challenges table for persisting ACME challenges

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DomainChallenges::Table)
                    .if_not_exists()
                    .col(string_len(DomainChallenges::Id, 255).primary_key())
                    .col(string_len(DomainChallenges::Domain, 255).not_null())
                    .col(string_len(DomainChallenges::ChallengeType, 16).not_null())
                    .col(
                        string_len(DomainChallenges::Status, 16)
                            .not_null()
                            .default("pending"),
                    )
                    .col(text_null(DomainChallenges::TokenOrRecordName))
                    .col(text_null(DomainChallenges::KeyAuthOrRecordValue))
                    .col(text_null(DomainChallenges::OrderUrl))
                    .col(
                        timestamp_with_time_zone(DomainChallenges::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(timestamp_with_time_zone(DomainChallenges::ExpiresAt).not_null())
                    .col(text_null(DomainChallenges::ErrorMessage))
                    .to_owned(),
            )
            .await?;

        // Index on domain for faster lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_domain_challenges_domain")
                    .table(DomainChallenges::Table)
                    .col(DomainChallenges::Domain)
                    .to_owned(),
            )
            .await?;

        // Index on status for finding pending challenges
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_domain_challenges_status")
                    .table(DomainChallenges::Table)
                    .col(DomainChallenges::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DomainChallenges::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DomainChallenges {
    #[sea_orm(iden = "domain_challenges")]
    Table,
    Id,
    Domain,
    ChallengeType,
    Status,
    TokenOrRecordName,
    KeyAuthOrRecordValue,
    OrderUrl,
    CreatedAt,
    ExpiresAt,
    ErrorMessage,
}
