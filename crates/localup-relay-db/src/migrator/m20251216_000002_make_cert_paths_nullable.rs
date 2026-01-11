//! Migration to make cert_path and key_path nullable in custom_domains table
//! This allows creating pending domains before certificate is provisioned

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite doesn't support ALTER COLUMN, so we need to recreate the table
        // For PostgreSQL/MySQL this would be simpler with ALTER TABLE

        let db = manager.get_connection();
        let backend = manager.get_database_backend();

        match backend {
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite: Recreate table with nullable columns
                db.execute_unprepared(
                    r#"
                    -- Create new table with nullable cert_path and key_path
                    CREATE TABLE custom_domains_new (
                        domain TEXT PRIMARY KEY NOT NULL,
                        cert_path TEXT,
                        key_path TEXT,
                        status TEXT NOT NULL DEFAULT 'pending',
                        provisioned_at TEXT NOT NULL,
                        expires_at TEXT,
                        auto_renew INTEGER NOT NULL DEFAULT 1,
                        error_message TEXT
                    );

                    -- Copy data from old table
                    INSERT INTO custom_domains_new
                    SELECT domain, cert_path, key_path, status, provisioned_at, expires_at, auto_renew, error_message
                    FROM custom_domains;

                    -- Drop old table
                    DROP TABLE custom_domains;

                    -- Rename new table
                    ALTER TABLE custom_domains_new RENAME TO custom_domains;
                    "#,
                )
                .await?;
            }
            _ => {
                // PostgreSQL/MySQL: Use ALTER COLUMN
                manager
                    .alter_table(
                        Table::alter()
                            .table(CustomDomains::Table)
                            .modify_column(ColumnDef::new(CustomDomains::CertPath).text().null())
                            .to_owned(),
                    )
                    .await?;

                manager
                    .alter_table(
                        Table::alter()
                            .table(CustomDomains::Table)
                            .modify_column(ColumnDef::new(CustomDomains::KeyPath).text().null())
                            .to_owned(),
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Note: This migration is not fully reversible for SQLite
        // as we'd need to handle existing NULL values
        let backend = manager.get_database_backend();

        if backend != sea_orm::DatabaseBackend::Sqlite {
            manager
                .alter_table(
                    Table::alter()
                        .table(CustomDomains::Table)
                        .modify_column(ColumnDef::new(CustomDomains::CertPath).text().not_null())
                        .to_owned(),
                )
                .await?;

            manager
                .alter_table(
                    Table::alter()
                        .table(CustomDomains::Table)
                        .modify_column(ColumnDef::new(CustomDomains::KeyPath).text().not_null())
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
enum CustomDomains {
    Table,
    CertPath,
    KeyPath,
}
