use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CapturedTcpConnection::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CapturedTcpConnection::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::LocalupId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::ClientAddr)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::TargetPort)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::BytesReceived)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::BytesSent)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::ConnectedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::DisconnectedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::DurationMs)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(CapturedTcpConnection::DisconnectReason)
                            .string()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on localup_id for filtering
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

        // Create index on connected_at for time-series queries
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

        // If PostgreSQL with TimescaleDB, create hypertable
        if manager.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            let sql = r#"
                SELECT create_hypertable(
                    'captured_tcp_connections',
                    'connected_at',
                    if_not_exists => TRUE,
                    migrate_data => TRUE
                );
            "#;

            // Try to create hypertable, ignore error if TimescaleDB is not installed
            let _ = manager.get_connection().execute_unprepared(sql).await;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CapturedTcpConnection::Table).to_owned())
            .await
    }
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
