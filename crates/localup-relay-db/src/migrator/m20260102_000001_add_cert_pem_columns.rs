//! Migration to add cert_pem and key_pem columns to custom_domains table
//! This allows storing certificate content directly in the database instead of filesystem paths

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add cert_pem column
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .add_column(ColumnDef::new(CustomDomains::CertPem).text().null())
                    .to_owned(),
            )
            .await?;

        // Add key_pem column
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .add_column(ColumnDef::new(CustomDomains::KeyPem).text().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Remove key_pem column
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .drop_column(CustomDomains::KeyPem)
                    .to_owned(),
            )
            .await?;

        // Remove cert_pem column
        manager
            .alter_table(
                Table::alter()
                    .table(CustomDomains::Table)
                    .drop_column(CustomDomains::CertPem)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum CustomDomains {
    Table,
    CertPem,
    KeyPem,
}
