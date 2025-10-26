//! Migration to create captured_requests table

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CapturedRequest::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CapturedRequest::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CapturedRequest::TunnelId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CapturedRequest::Method).string().not_null())
                    .col(ColumnDef::new(CapturedRequest::Path).string().not_null())
                    .col(ColumnDef::new(CapturedRequest::Host).string())
                    .col(ColumnDef::new(CapturedRequest::Headers).text().not_null())
                    .col(ColumnDef::new(CapturedRequest::Body).text())
                    .col(ColumnDef::new(CapturedRequest::Status).integer())
                    .col(ColumnDef::new(CapturedRequest::ResponseHeaders).text())
                    .col(ColumnDef::new(CapturedRequest::ResponseBody).text())
                    .col(
                        ColumnDef::new(CapturedRequest::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CapturedRequest::RespondedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(CapturedRequest::LatencyMs).integer())
                    .to_owned(),
            )
            .await?;

        // Create index on tunnel_id for efficient queries
        manager
            .create_index(
                Index::create()
                    .name("idx_captured_requests_tunnel_id")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::TunnelId)
                    .to_owned(),
            )
            .await?;

        // Create index on created_at for time-based queries
        manager
            .create_index(
                Index::create()
                    .name("idx_captured_requests_created_at")
                    .table(CapturedRequest::Table)
                    .col(CapturedRequest::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // For PostgreSQL, enable TimescaleDB hypertable (if TimescaleDB extension is available)
        // This is optional and will be skipped if TimescaleDB is not installed
        // For SQLite, this will be a no-op
        let db_backend = manager.get_database_backend();
        if matches!(db_backend, sea_orm::DbBackend::Postgres) {
            let sql = r#"
                DO $$
                BEGIN
                    -- Check if timescaledb extension exists
                    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb') THEN
                        -- Convert table to hypertable for time-series optimization
                        PERFORM create_hypertable('captured_requests', 'created_at', if_not_exists => TRUE);
                    END IF;
                END
                $$;
            "#;

            manager.get_connection().execute_unprepared(sql).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CapturedRequest::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum CapturedRequest {
    #[sea_orm(iden = "captured_requests")]
    Table,
    Id,
    TunnelId,
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
