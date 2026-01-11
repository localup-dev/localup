//! Migration to add supported_protocols to relay_servers

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add supported_protocols column to relay_servers table
        // This stores a JSON array of protocols like ["http", "https", "tcp", "tls"]
        manager
            .alter_table(
                Table::alter()
                    .table(RelayServer::Table)
                    .add_column(
                        text(RelayServer::SupportedProtocols)
                            .not_null()
                            .default("[\"http\",\"https\",\"tcp\",\"tls\"]"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(RelayServer::Table)
                    .drop_column(RelayServer::SupportedProtocols)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum RelayServer {
    #[sea_orm(iden = "relay_servers")]
    Table,
    SupportedProtocols,
}
