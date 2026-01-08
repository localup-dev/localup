//! Migration to add ip_allowlist column to tunnel_configs table

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add ip_allowlist column to tunnel_configs table
        // Stores JSON array of IP addresses and CIDR ranges, e.g. ["192.168.1.0/24", "10.0.0.1"]
        manager
            .alter_table(
                Table::alter()
                    .table(TunnelConfig::Table)
                    .add_column(text_null(TunnelConfig::IpAllowlist))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(TunnelConfig::Table)
                    .drop_column(TunnelConfig::IpAllowlist)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum TunnelConfig {
    #[sea_orm(iden = "tunnel_configs")]
    Table,
    IpAllowlist,
}
