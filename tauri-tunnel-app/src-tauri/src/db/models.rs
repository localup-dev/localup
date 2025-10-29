use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Relay entity - stores exit node/relay server configurations
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "relays")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub address: String, // host:port
    pub region: String,  // UsEast, UsWest, EuWest, EuCentral, AsiaPacific, SouthAmerica
    pub is_default: bool,
    pub status: String, // active, inactive, error
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Tunnel entity - stores tunnel configurations
pub mod tunnel {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "tunnels")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub description: Option<String>,
        pub local_host: String,
        pub auth_token: String,
        pub exit_node_config: String, // JSON: { type: 'auto' | 'nearest' | 'specific' | 'multi' | 'custom', value?: string | string[] }
        pub failover: bool,
        pub connection_timeout: i32, // milliseconds
        pub status: String,          // connecting, connected, disconnected, error
        pub last_connected_at: Option<DateTime<Utc>>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::protocol::Entity")]
        Protocols,
    }

    impl Related<super::protocol::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Protocols.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// Protocol entity - stores protocol configurations for each tunnel
pub mod protocol {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "protocols")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub tunnel_id: String,
        pub protocol_type: String, // tcp, tls, http, https
        pub local_port: i32,
        pub remote_port: Option<i32>,
        pub subdomain: Option<String>,
        pub custom_domain: Option<String>,
        pub created_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::tunnel::Entity",
            from = "Column::TunnelId",
            to = "super::tunnel::Column::Id",
            on_update = "Cascade",
            on_delete = "Cascade"
        )]
        Tunnel,
    }

    impl Related<super::tunnel::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Tunnel.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// Re-export for easier access
pub type Relay = Model;
pub type Tunnel = tunnel::Model;
pub type Protocol = protocol::Model;
