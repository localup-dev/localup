use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Make disconnected_at, duration_ms, and disconnect_reason nullable for active connections
        // This is needed because we now track active connections (inserted with NULL for these fields)
        // and update them when they disconnect

        let db_backend = manager.get_database_backend();

        match db_backend {
            sea_orm::DatabaseBackend::Sqlite => {
                // SQLite doesn't support ALTER COLUMN directly, so we need to:
                // 1. Create a new table with the correct schema
                // 2. Copy data from old table
                // 3. Drop old table
                // 4. Rename new table

                manager
                    .get_connection()
                    .execute_unprepared(
                        r#"
                        -- Create new table with nullable fields
                        CREATE TABLE captured_tcp_connections_new (
                            id TEXT NOT NULL PRIMARY KEY,
                            localup_id TEXT NOT NULL,
                            client_addr TEXT NOT NULL,
                            target_port INTEGER NOT NULL,
                            bytes_received BIGINT NOT NULL DEFAULT 0,
                            bytes_sent BIGINT NOT NULL DEFAULT 0,
                            connected_at TEXT NOT NULL,
                            disconnected_at TEXT NULL,
                            duration_ms INTEGER NULL,
                            disconnect_reason TEXT NULL
                        );

                        -- Copy existing data
                        INSERT INTO captured_tcp_connections_new
                        SELECT * FROM captured_tcp_connections;

                        -- Drop old table
                        DROP TABLE captured_tcp_connections;

                        -- Rename new table
                        ALTER TABLE captured_tcp_connections_new
                        RENAME TO captured_tcp_connections;

                        -- Recreate indexes
                        CREATE INDEX IF NOT EXISTS idx_captured_tcp_connections_localup_id
                        ON captured_tcp_connections(localup_id);

                        CREATE INDEX IF NOT EXISTS idx_captured_tcp_connections_connected_at
                        ON captured_tcp_connections(connected_at);
                        "#,
                    )
                    .await?;
            }
            sea_orm::DatabaseBackend::Postgres => {
                // PostgreSQL supports ALTER COLUMN
                manager
                    .get_connection()
                    .execute_unprepared(
                        r#"
                        ALTER TABLE captured_tcp_connections
                        ALTER COLUMN disconnected_at DROP NOT NULL;

                        ALTER TABLE captured_tcp_connections
                        ALTER COLUMN duration_ms DROP NOT NULL;

                        ALTER TABLE captured_tcp_connections
                        ALTER COLUMN disconnect_reason DROP NOT NULL;
                        "#,
                    )
                    .await?;
            }
            sea_orm::DatabaseBackend::MySql => {
                // MySQL supports MODIFY COLUMN
                manager
                    .get_connection()
                    .execute_unprepared(
                        r#"
                        ALTER TABLE captured_tcp_connections
                        MODIFY COLUMN disconnected_at TIMESTAMP NULL;

                        ALTER TABLE captured_tcp_connections
                        MODIFY COLUMN duration_ms INT NULL;

                        ALTER TABLE captured_tcp_connections
                        MODIFY COLUMN disconnect_reason TEXT NULL;
                        "#,
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Cannot safely revert this migration as it would break active connections
        // that have NULL values in these fields
        println!("Warning: Reverting this migration is not supported as it would require dropping active connection data");
        Ok(())
    }
}
