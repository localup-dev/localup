//! Consolidated initial schema migration

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ============================================================
        // 1. Create users table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(uuid(User::Id).primary_key())
                    .col(string_len(User::Email, 255).not_null().unique_key())
                    .col(string_len(User::PasswordHash, 255).not_null())
                    .col(string_len(User::FullName, 255).null())
                    .col(string_len(User::Role, 32).not_null().default("user"))
                    .col(boolean(User::IsActive).not_null().default(true))
                    .col(
                        timestamp_with_time_zone(User::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(User::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_email")
                    .table(User::Table)
                    .col(User::Email)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 2. Create teams table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(Team::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Team::Id).uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new(Team::Name)
                            .string_len(255)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Team::Slug)
                            .string_len(255)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Team::Description).text())
                    .col(ColumnDef::new(Team::OwnerId).uuid().not_null())
                    .col(
                        ColumnDef::new(Team::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Team::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_teams_owner_id")
                            .from(Team::Table, Team::OwnerId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_teams_slug")
                    .table(Team::Table)
                    .col(Team::Slug)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_teams_owner_id")
                    .table(Team::Table)
                    .col(Team::OwnerId)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 3. Create team_members junction table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(TeamMember::Table)
                    .if_not_exists()
                    .col(uuid(TeamMember::TeamId).not_null())
                    .col(uuid(TeamMember::UserId).not_null())
                    .col(
                        string_len(TeamMember::Role, 32)
                            .not_null()
                            .default("member"),
                    )
                    .col(
                        timestamp_with_time_zone(TeamMember::JoinedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(TeamMember::TeamId)
                            .col(TeamMember::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_team_id")
                            .from(TeamMember::Table, TeamMember::TeamId)
                            .to(Team::Table, Team::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_user_id")
                            .from(TeamMember::Table, TeamMember::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_team_members_user_id")
                    .table(TeamMember::Table)
                    .col(TeamMember::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_team_members_team_id")
                    .table(TeamMember::Table)
                    .col(TeamMember::TeamId)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 4. Create auth_token table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(AuthToken::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuthToken::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuthToken::UserId).uuid().not_null())
                    .col(ColumnDef::new(AuthToken::TeamId).uuid())
                    .col(ColumnDef::new(AuthToken::Name).string_len(255).not_null())
                    .col(ColumnDef::new(AuthToken::Description).text())
                    .col(
                        ColumnDef::new(AuthToken::TokenHash)
                            .string_len(255)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(AuthToken::LastUsedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(AuthToken::ExpiresAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(AuthToken::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(AuthToken::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auth_tokens_user_id")
                            .from(AuthToken::Table, AuthToken::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auth_tokens_team_id")
                            .from(AuthToken::Table, AuthToken::TeamId)
                            .to(Team::Table, Team::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_auth_tokens_user_id")
                    .table(AuthToken::Table)
                    .col(AuthToken::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_auth_tokens_team_id")
                    .table(AuthToken::Table)
                    .col(AuthToken::TeamId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_auth_tokens_token_hash")
                    .table(AuthToken::Table)
                    .col(AuthToken::TokenHash)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 5. Create custom_domains table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(CustomDomain::Table)
                    .if_not_exists()
                    .col(string(CustomDomain::Domain).not_null().primary_key())
                    .col(string(CustomDomain::CertPath).null())
                    .col(string(CustomDomain::KeyPath).null())
                    .col(
                        string_len(CustomDomain::Status, 32)
                            .not_null()
                            .default("pending"),
                    )
                    .col(timestamp_with_time_zone(CustomDomain::ProvisionedAt).not_null())
                    .col(timestamp_with_time_zone(CustomDomain::ExpiresAt).null())
                    .col(boolean(CustomDomain::AutoRenew).not_null().default(true))
                    .col(text(CustomDomain::ErrorMessage).null())
                    .col(uuid(CustomDomain::UserId).null())
                    .col(uuid(CustomDomain::TeamId).null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_custom_domains_status")
                    .table(CustomDomain::Table)
                    .col(CustomDomain::Status)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_custom_domains_expires_at")
                    .table(CustomDomain::Table)
                    .col(CustomDomain::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_domains_user_id")
                    .table(CustomDomain::Table)
                    .col(CustomDomain::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_custom_domains_team_id")
                    .table(CustomDomain::Table)
                    .col(CustomDomain::TeamId)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 6. Create captured_requests table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(CapturedRequest::Table)
                    .if_not_exists()
                    .col(string(CapturedRequest::Id).not_null().primary_key())
                    .col(string(CapturedRequest::LocalupId).not_null())
                    .col(string(CapturedRequest::Method).not_null())
                    .col(string(CapturedRequest::Path).not_null())
                    .col(string(CapturedRequest::Host).null())
                    .col(text(CapturedRequest::Headers).not_null())
                    .col(text(CapturedRequest::Body).null())
                    .col(integer(CapturedRequest::Status).null())
                    .col(text(CapturedRequest::ResponseHeaders).null())
                    .col(text(CapturedRequest::ResponseBody).null())
                    .col(timestamp_with_time_zone(CapturedRequest::CreatedAt).not_null())
                    .col(timestamp_with_time_zone(CapturedRequest::RespondedAt).null())
                    .col(integer(CapturedRequest::LatencyMs).null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_captured_requests_localup_id")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::LocalupId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_captured_requests_created_at")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // For PostgreSQL, enable TimescaleDB hypertable (if extension available)
        let db_backend = manager.get_database_backend();
        if matches!(db_backend, sea_orm::DbBackend::Postgres) {
            let sql = r#"
                DO $$
                BEGIN
                    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb') THEN
                        PERFORM create_hypertable('captured_requests', 'created_at', if_not_exists => TRUE);
                    END IF;
                END
                $$;
            "#;
            manager.get_connection().execute_unprepared(sql).await?;
        }

        // ============================================================
        // 7. Create captured_tcp_connections table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(CapturedTcpConnection::Table)
                    .if_not_exists()
                    .col(string(CapturedTcpConnection::Id).not_null().primary_key())
                    .col(string(CapturedTcpConnection::LocalupId).not_null())
                    .col(string(CapturedTcpConnection::ClientAddr).not_null())
                    .col(integer(CapturedTcpConnection::TargetPort).not_null())
                    .col(
                        big_integer(CapturedTcpConnection::BytesReceived)
                            .not_null()
                            .default(0),
                    )
                    .col(
                        big_integer(CapturedTcpConnection::BytesSent)
                            .not_null()
                            .default(0),
                    )
                    .col(timestamp_with_time_zone(CapturedTcpConnection::ConnectedAt).not_null())
                    .col(timestamp_with_time_zone(CapturedTcpConnection::DisconnectedAt).null())
                    .col(integer(CapturedTcpConnection::DurationMs).null())
                    .col(string(CapturedTcpConnection::DisconnectReason).null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_captured_tcp_connections_localup_id")
                    .table(CapturedTcpConnection::Table)
                    .col(CapturedTcpConnection::LocalupId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_captured_tcp_connections_connected_at")
                    .table(CapturedTcpConnection::Table)
                    .col(CapturedTcpConnection::ConnectedAt)
                    .to_owned(),
            )
            .await?;

        // For PostgreSQL, enable TimescaleDB hypertable (if extension available)
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            let sql = r#"
                SELECT create_hypertable(
                    'captured_tcp_connections',
                    'connected_at',
                    if_not_exists => TRUE,
                    migrate_data => TRUE
                );
            "#;
            // Try to create hypertable, ignore error if TimescaleDB not installed
            let _ = manager.get_connection().execute_unprepared(sql).await;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order (respecting foreign keys)
        manager
            .drop_table(Table::drop().table(CapturedTcpConnection::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(CapturedRequest::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(CustomDomain::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(AuthToken::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(TeamMember::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Team::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await?;

        Ok(())
    }
}

// ============================================================
// Table identifiers
// ============================================================

#[derive(DeriveIden)]
enum User {
    #[sea_orm(iden = "users")]
    Table,
    Id,
    Email,
    PasswordHash,
    FullName,
    Role,
    IsActive,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Team {
    #[sea_orm(iden = "teams")]
    Table,
    Id,
    Name,
    Slug,
    Description,
    OwnerId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum TeamMember {
    #[sea_orm(iden = "team_members")]
    Table,
    TeamId,
    UserId,
    Role,
    JoinedAt,
}

#[derive(DeriveIden)]
enum AuthToken {
    Table,
    Id,
    UserId,
    TeamId,
    Name,
    Description,
    TokenHash,
    LastUsedAt,
    ExpiresAt,
    IsActive,
    CreatedAt,
}

#[derive(DeriveIden)]
enum CustomDomain {
    #[sea_orm(iden = "custom_domains")]
    Table,
    Domain,
    CertPath,
    KeyPath,
    Status,
    ProvisionedAt,
    ExpiresAt,
    AutoRenew,
    ErrorMessage,
    UserId,
    TeamId,
}

#[derive(DeriveIden)]
enum CapturedRequest {
    #[sea_orm(iden = "captured_requests")]
    Table,
    Id,
    LocalupId,
    Method,
    Path,
    Host,
    Headers,
    Body,
    Status,
    ResponseHeaders,
    ResponseBody,
    CreatedAt,
    RespondedAt,
    LatencyMs,
}

#[derive(DeriveIden)]
enum CapturedTcpConnection {
    #[sea_orm(iden = "captured_tcp_connections")]
    Table,
    Id,
    LocalupId,
    ClientAddr,
    TargetPort,
    BytesReceived,
    BytesSent,
    ConnectedAt,
    DisconnectedAt,
    DurationMs,
    DisconnectReason,
}
