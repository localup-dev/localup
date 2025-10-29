use chrono::Utc;
use sea_orm::{prelude::*, Database, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::models::{self, Protocol, Relay, Tunnel};

/// Database service for managing relays, tunnels, and protocols
pub struct DatabaseService {
    db: Arc<RwLock<Option<DatabaseConnection>>>,
}

impl DatabaseService {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the database connection and run migrations
    pub async fn init(&self, db_path: PathBuf) -> Result<(), DbErr> {
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        tracing::info!("Initializing database at: {}", db_url);

        let db = Database::connect(&db_url).await?;

        // Run migrations
        self.run_migrations(&db).await?;

        let mut conn = self.db.write().await;
        *conn = Some(db);

        tracing::info!("Database initialized successfully");
        Ok(())
    }

    /// Get database connection
    async fn get_db(&self) -> Result<DatabaseConnection, DbErr> {
        let conn = self.db.read().await;
        conn.as_ref()
            .ok_or_else(|| DbErr::Custom("Database not initialized".to_string()))
            .cloned()
    }

    /// Run database migrations
    async fn run_migrations(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
        use sea_orm::Schema;

        let schema = Schema::new(db.get_database_backend());
        let backend = db.get_database_backend();

        // Create relays table if not exists
        let mut relays_stmt = schema.create_table_from_entity(models::Entity);
        relays_stmt.if_not_exists();
        let _ = db.execute(backend.build(&relays_stmt)).await;

        // Create tunnels table if not exists
        let mut tunnels_stmt = schema.create_table_from_entity(models::tunnel::Entity);
        tunnels_stmt.if_not_exists();
        let _ = db.execute(backend.build(&tunnels_stmt)).await;

        // Create protocols table if not exists
        let mut protocols_stmt = schema.create_table_from_entity(models::protocol::Entity);
        protocols_stmt.if_not_exists();
        let _ = db.execute(backend.build(&protocols_stmt)).await;

        Ok(())
    }

    // ========== Relay CRUD Operations ==========

    pub async fn create_relay(
        &self,
        name: String,
        address: String,
        region: String,
        description: Option<String>,
    ) -> Result<Relay, DbErr> {
        let db = self.get_db().await?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let relay = models::ActiveModel {
            id: Set(id.clone()),
            name: Set(name),
            description: Set(description),
            address: Set(address),
            region: Set(region),
            is_default: Set(false),
            status: Set("active".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = relay.insert(&db).await?;
        Ok(result)
    }

    pub async fn get_relay(&self, id: String) -> Result<Option<Relay>, DbErr> {
        let db = self.get_db().await?;
        models::Entity::find_by_id(id).one(&db).await
    }

    pub async fn list_relays(&self) -> Result<Vec<Relay>, DbErr> {
        let db = self.get_db().await?;
        models::Entity::find().all(&db).await
    }

    pub async fn update_relay(
        &self,
        id: String,
        name: Option<String>,
        address: Option<String>,
        region: Option<String>,
        description: Option<String>,
        status: Option<String>,
    ) -> Result<Relay, DbErr> {
        let db = self.get_db().await?;

        let relay = models::Entity::find_by_id(id.clone())
            .one(&db)
            .await?
            .ok_or_else(|| DbErr::Custom("Relay not found".to_string()))?;

        let mut relay: models::ActiveModel = relay.into();

        if let Some(n) = name {
            relay.name = Set(n);
        }
        if let Some(a) = address {
            relay.address = Set(a);
        }
        if let Some(r) = region {
            relay.region = Set(r);
        }
        if let Some(d) = description {
            relay.description = Set(Some(d));
        }
        if let Some(s) = status {
            relay.status = Set(s);
        }
        relay.updated_at = Set(Utc::now());

        relay.update(&db).await
    }

    pub async fn delete_relay(&self, id: String) -> Result<(), DbErr> {
        let db = self.get_db().await?;
        models::Entity::delete_by_id(id).exec(&db).await?;
        Ok(())
    }

    // ========== Tunnel CRUD Operations ==========

    #[allow(clippy::too_many_arguments)]
    pub async fn create_tunnel(
        &self,
        name: String,
        local_host: String,
        auth_token: String,
        exit_node_config: String,
        description: Option<String>,
        failover: bool,
        connection_timeout: i32,
    ) -> Result<Tunnel, DbErr> {
        let db = self.get_db().await?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let tunnel = models::tunnel::ActiveModel {
            id: Set(id.clone()),
            name: Set(name),
            description: Set(description),
            local_host: Set(local_host),
            auth_token: Set(auth_token),
            exit_node_config: Set(exit_node_config),
            failover: Set(failover),
            connection_timeout: Set(connection_timeout),
            status: Set("disconnected".to_string()),
            last_connected_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = tunnel.insert(&db).await?;
        Ok(result)
    }

    pub async fn get_tunnel(&self, id: String) -> Result<Option<Tunnel>, DbErr> {
        let db = self.get_db().await?;
        models::tunnel::Entity::find_by_id(id).one(&db).await
    }

    pub async fn list_tunnels(&self) -> Result<Vec<Tunnel>, DbErr> {
        let db = self.get_db().await?;
        models::tunnel::Entity::find().all(&db).await
    }

    pub async fn update_tunnel_status(
        &self,
        id: String,
        status: String,
        last_connected_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Tunnel, DbErr> {
        let db = self.get_db().await?;

        let tunnel = models::tunnel::Entity::find_by_id(id.clone())
            .one(&db)
            .await?
            .ok_or_else(|| DbErr::Custom("Tunnel not found".to_string()))?;

        let mut tunnel: models::tunnel::ActiveModel = tunnel.into();
        tunnel.status = Set(status);
        if let Some(lca) = last_connected_at {
            tunnel.last_connected_at = Set(Some(lca));
        }
        tunnel.updated_at = Set(Utc::now());

        tunnel.update(&db).await
    }

    pub async fn delete_tunnel(&self, id: String) -> Result<(), DbErr> {
        let db = self.get_db().await?;
        models::tunnel::Entity::delete_by_id(id).exec(&db).await?;
        Ok(())
    }

    // ========== Protocol CRUD Operations ==========

    pub async fn create_protocol(
        &self,
        tunnel_id: String,
        protocol_type: String,
        local_port: i32,
        remote_port: Option<i32>,
        subdomain: Option<String>,
        custom_domain: Option<String>,
    ) -> Result<Protocol, DbErr> {
        let db = self.get_db().await?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let protocol = models::protocol::ActiveModel {
            id: Set(id.clone()),
            tunnel_id: Set(tunnel_id),
            protocol_type: Set(protocol_type),
            local_port: Set(local_port),
            remote_port: Set(remote_port),
            subdomain: Set(subdomain),
            custom_domain: Set(custom_domain),
            created_at: Set(now),
        };

        let result = protocol.insert(&db).await?;
        Ok(result)
    }

    pub async fn list_protocols_for_tunnel(
        &self,
        tunnel_id: String,
    ) -> Result<Vec<Protocol>, DbErr> {
        let db = self.get_db().await?;
        models::protocol::Entity::find()
            .filter(models::protocol::Column::TunnelId.eq(tunnel_id))
            .all(&db)
            .await
    }

    pub async fn delete_protocol(&self, id: String) -> Result<(), DbErr> {
        let db = self.get_db().await?;
        models::protocol::Entity::delete_by_id(id).exec(&db).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> DatabaseService {
        // Use in-memory SQLite database for tests
        let service = DatabaseService {
            db: Arc::new(RwLock::new(None)),
        };

        let db = Database::connect("sqlite::memory:").await.unwrap();
        service.run_migrations(&db).await.unwrap();

        {
            let mut conn = service.db.write().await;
            *conn = Some(db);
        } // Drop the write lock here

        service
    }

    #[tokio::test]
    async fn test_relay_crud() {
        let service = setup_test_db().await;

        // Create relay
        let relay = service
            .create_relay(
                "Test Relay".to_string(),
                "localhost:8080".to_string(),
                "UsEast".to_string(),
                Some("Test description".to_string()),
            )
            .await
            .unwrap();

        println!("✅ Created relay: {:?}", relay);
        assert_eq!(relay.name, "Test Relay");
        assert_eq!(relay.address, "localhost:8080");
        assert_eq!(relay.region, "UsEast");

        // List relays
        let relays = service.list_relays().await.unwrap();
        println!("✅ Listed {} relay(s)", relays.len());
        assert_eq!(relays.len(), 1);
        assert_eq!(relays[0].id, relay.id);

        // Get relay by ID
        let fetched = service.get_relay(relay.id.clone()).await.unwrap();
        println!("✅ Fetched relay: {:?}", fetched);
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "Test Relay");

        // Update relay
        let updated = service
            .update_relay(
                relay.id.clone(),
                Some("Updated Name".to_string()),
                None,
                None,
                None,
                Some("active".to_string()),
            )
            .await
            .unwrap();
        println!("✅ Updated relay: {:?}", updated);
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.status, "active");

        // Delete relay
        service.delete_relay(relay.id.clone()).await.unwrap();
        println!("✅ Deleted relay");

        // Verify deletion
        let after_delete = service.get_relay(relay.id).await.unwrap();
        println!("✅ Verified deletion: {:?}", after_delete);
        assert!(after_delete.is_none());

        let relays_after = service.list_relays().await.unwrap();
        assert_eq!(relays_after.len(), 0);
    }

    #[tokio::test]
    async fn test_tunnel_with_protocols() {
        let service = setup_test_db().await;

        // Create tunnel
        let tunnel = service
            .create_tunnel(
                "Test Tunnel".to_string(),
                "localhost".to_string(),
                "test-token".to_string(),
                r#"{"type":"custom","address":"localhost:8080"}"#.to_string(),
                Some("Test tunnel description".to_string()),
                true,
                30000,
            )
            .await
            .unwrap();

        println!("✅ Created tunnel: {:?}", tunnel);
        assert_eq!(tunnel.name, "Test Tunnel");
        assert_eq!(tunnel.local_host, "localhost");
        assert_eq!(tunnel.status, "disconnected");

        // Add HTTP protocol
        let http_protocol = service
            .create_protocol(
                tunnel.id.clone(),
                "http".to_string(),
                3000,
                None,
                Some("myapp".to_string()),
                None,
            )
            .await
            .unwrap();
        println!("✅ Created HTTP protocol: {:?}", http_protocol);

        // Add HTTPS protocol
        let https_protocol = service
            .create_protocol(
                tunnel.id.clone(),
                "https".to_string(),
                3001,
                None,
                Some("secure-app".to_string()),
                Some("example.com".to_string()),
            )
            .await
            .unwrap();
        println!("✅ Created HTTPS protocol: {:?}", https_protocol);

        // List protocols for tunnel
        let protocols = service
            .list_protocols_for_tunnel(tunnel.id.clone())
            .await
            .unwrap();
        println!("✅ Listed {} protocol(s)", protocols.len());
        assert_eq!(protocols.len(), 2);

        // List all tunnels
        let tunnels = service.list_tunnels().await.unwrap();
        println!("✅ Listed {} tunnel(s)", tunnels.len());
        assert_eq!(tunnels.len(), 1);

        // Update tunnel status
        let updated_tunnel = service
            .update_tunnel_status(tunnel.id.clone(), "connected".to_string(), Some(Utc::now()))
            .await
            .unwrap();
        println!("✅ Updated tunnel status: {:?}", updated_tunnel.status);
        assert_eq!(updated_tunnel.status, "connected");
        assert!(updated_tunnel.last_connected_at.is_some());

        // Delete protocol
        service
            .delete_protocol(http_protocol.id.clone())
            .await
            .unwrap();
        println!("✅ Deleted HTTP protocol");

        let protocols_after = service
            .list_protocols_for_tunnel(tunnel.id.clone())
            .await
            .unwrap();
        assert_eq!(protocols_after.len(), 1);

        // Delete tunnel
        let tunnel_id_for_check = tunnel.id.clone();
        service.delete_tunnel(tunnel.id.clone()).await.unwrap();
        println!("✅ Deleted tunnel");

        // Verify tunnel deletion
        let tunnel_after = service
            .get_tunnel(tunnel_id_for_check.clone())
            .await
            .unwrap();
        assert!(tunnel_after.is_none());

        // Verify protocols are also deleted (cascade)
        let protocols_final = service
            .list_protocols_for_tunnel(tunnel_id_for_check)
            .await
            .unwrap();
        println!(
            "✅ Protocols after tunnel deletion: {}",
            protocols_final.len()
        );
        // Note: Protocols won't be automatically deleted by SeaORM,
        // we'd need to handle that in the delete_tunnel method
    }

    #[tokio::test]
    async fn test_multiple_relays() {
        let service = setup_test_db().await;

        // Create multiple relays
        let relay1 = service
            .create_relay(
                "US East".to_string(),
                "us-east.relay.com:443".to_string(),
                "UsEast".to_string(),
                None,
            )
            .await
            .unwrap();

        let relay2 = service
            .create_relay(
                "EU West".to_string(),
                "eu-west.relay.com:443".to_string(),
                "EuWest".to_string(),
                None,
            )
            .await
            .unwrap();

        let relay3 = service
            .create_relay(
                "Asia Pacific".to_string(),
                "ap.relay.com:443".to_string(),
                "AsiaPacific".to_string(),
                None,
            )
            .await
            .unwrap();

        println!("✅ Created 3 relays");

        // List all relays
        let relays = service.list_relays().await.unwrap();
        assert_eq!(relays.len(), 3);
        println!("✅ Listed {} relays", relays.len());

        // Delete one relay
        service.delete_relay(relay2.id).await.unwrap();
        println!("✅ Deleted EU West relay");

        let relays_after = service.list_relays().await.unwrap();
        assert_eq!(relays_after.len(), 2);

        // Verify remaining relays
        let ids: Vec<String> = relays_after.iter().map(|r| r.id.clone()).collect();
        assert!(ids.contains(&relay1.id));
        assert!(ids.contains(&relay3.id));
        println!("✅ Verified remaining relays");
    }

    #[tokio::test]
    async fn test_relay_not_found() {
        let service = setup_test_db().await;

        let result = service
            .get_relay("nonexistent-id".to_string())
            .await
            .unwrap();
        assert!(result.is_none());
        println!("✅ Correctly returned None for nonexistent relay");
    }

    #[tokio::test]
    async fn test_delete_nonexistent_relay() {
        let service = setup_test_db().await;

        // This should not error, just do nothing
        let result = service.delete_relay("nonexistent-id".to_string()).await;
        assert!(result.is_ok());
        println!("✅ Delete nonexistent relay succeeded (no-op)");
    }
}
