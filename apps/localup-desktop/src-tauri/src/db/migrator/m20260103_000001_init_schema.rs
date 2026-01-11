//! Initial schema migration for LocalUp Desktop

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ============================================================
        // 1. Create relay_servers table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(RelayServer::Table)
                    .if_not_exists()
                    .col(string(RelayServer::Id).not_null().primary_key())
                    .col(string(RelayServer::Name).not_null())
                    .col(string(RelayServer::Address).not_null())
                    .col(text_null(RelayServer::JwtToken))
                    .col(string(RelayServer::Protocol).not_null().default("quic"))
                    .col(boolean(RelayServer::Insecure).not_null().default(false))
                    .col(boolean(RelayServer::IsDefault).not_null().default(false))
                    .col(
                        timestamp_with_time_zone(RelayServer::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(RelayServer::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 2. Create tunnel_configs table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(TunnelConfig::Table)
                    .if_not_exists()
                    .col(string(TunnelConfig::Id).not_null().primary_key())
                    .col(string(TunnelConfig::Name).not_null())
                    .col(string(TunnelConfig::RelayServerId).not_null())
                    .col(
                        string(TunnelConfig::LocalHost)
                            .not_null()
                            .default("localhost"),
                    )
                    .col(integer(TunnelConfig::LocalPort).not_null())
                    .col(string(TunnelConfig::Protocol).not_null())
                    .col(text_null(TunnelConfig::Subdomain))
                    .col(text_null(TunnelConfig::CustomDomain))
                    .col(boolean(TunnelConfig::AutoStart).not_null().default(false))
                    .col(boolean(TunnelConfig::Enabled).not_null().default(true))
                    .col(
                        timestamp_with_time_zone(TunnelConfig::CreatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(TunnelConfig::UpdatedAt)
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tunnel_configs_relay_server_id")
                            .from(TunnelConfig::Table, TunnelConfig::RelayServerId)
                            .to(RelayServer::Table, RelayServer::Id)
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
                    .name("idx_tunnel_configs_relay_server_id")
                    .table(TunnelConfig::Table)
                    .col(TunnelConfig::RelayServerId)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 3. Create tunnel_sessions table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(TunnelSession::Table)
                    .if_not_exists()
                    .col(string(TunnelSession::Id).not_null().primary_key())
                    .col(string(TunnelSession::ConfigId).not_null())
                    .col(string(TunnelSession::Status).not_null())
                    .col(text_null(TunnelSession::PublicUrl))
                    .col(text_null(TunnelSession::LocalupId))
                    .col(timestamp_with_time_zone_null(TunnelSession::ConnectedAt))
                    .col(text_null(TunnelSession::ErrorMessage))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tunnel_sessions_config_id")
                            .from(TunnelSession::Table, TunnelSession::ConfigId)
                            .to(TunnelConfig::Table, TunnelConfig::Id)
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
                    .name("idx_tunnel_sessions_config_id")
                    .table(TunnelSession::Table)
                    .col(TunnelSession::ConfigId)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 4. Create captured_requests table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(CapturedRequest::Table)
                    .if_not_exists()
                    .col(string(CapturedRequest::Id).not_null().primary_key())
                    .col(string(CapturedRequest::TunnelSessionId).not_null())
                    .col(string(CapturedRequest::LocalupId).not_null())
                    .col(string(CapturedRequest::Method).not_null())
                    .col(string(CapturedRequest::Path).not_null())
                    .col(text_null(CapturedRequest::Host))
                    .col(text(CapturedRequest::Headers))
                    .col(text_null(CapturedRequest::Body))
                    .col(integer_null(CapturedRequest::Status))
                    .col(text_null(CapturedRequest::ResponseHeaders))
                    .col(text_null(CapturedRequest::ResponseBody))
                    .col(timestamp_with_time_zone(CapturedRequest::CreatedAt).not_null())
                    .col(integer_null(CapturedRequest::LatencyMs))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_captured_requests_tunnel_session_id")
                            .from(CapturedRequest::Table, CapturedRequest::TunnelSessionId)
                            .to(TunnelSession::Table, TunnelSession::Id)
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
                    .name("idx_captured_requests_tunnel_session_id")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::TunnelSessionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_captured_requests_localup_id")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::LocalupId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_captured_requests_created_at")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // ============================================================
        // 5. Create settings table
        // ============================================================
        manager
            .create_table(
                Table::create()
                    .table(Setting::Table)
                    .if_not_exists()
                    .col(string(Setting::Key).not_null().primary_key())
                    .col(text(Setting::Value).not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order (respecting foreign keys)
        manager
            .drop_table(Table::drop().table(Setting::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(CapturedRequest::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(TunnelSession::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(TunnelConfig::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(RelayServer::Table).to_owned())
            .await?;

        Ok(())
    }
}

// ============================================================
// Table identifiers
// ============================================================

#[derive(DeriveIden)]
enum RelayServer {
    #[sea_orm(iden = "relay_servers")]
    Table,
    Id,
    Name,
    Address,
    JwtToken,
    Protocol,
    Insecure,
    IsDefault,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum TunnelConfig {
    #[sea_orm(iden = "tunnel_configs")]
    Table,
    Id,
    Name,
    RelayServerId,
    LocalHost,
    LocalPort,
    Protocol,
    Subdomain,
    CustomDomain,
    AutoStart,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum TunnelSession {
    #[sea_orm(iden = "tunnel_sessions")]
    Table,
    Id,
    ConfigId,
    Status,
    PublicUrl,
    LocalupId,
    ConnectedAt,
    ErrorMessage,
}

#[derive(DeriveIden)]
enum CapturedRequest {
    #[sea_orm(iden = "captured_requests")]
    Table,
    Id,
    TunnelSessionId,
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
    LatencyMs,
}

#[derive(DeriveIden)]
enum Setting {
    #[sea_orm(iden = "settings")]
    Table,
    Key,
    Value,
}
