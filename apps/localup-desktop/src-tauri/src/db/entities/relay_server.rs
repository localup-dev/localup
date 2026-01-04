//! RelayServer entity for storing relay server configurations

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "relay_servers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Display name for the relay
    pub name: String,

    /// Relay address (e.g., "relay.localup.dev:4443")
    pub address: String,

    /// JWT authentication token
    #[sea_orm(column_type = "Text", nullable)]
    pub jwt_token: Option<String>,

    /// Connection protocol (quic, h2, websocket)
    pub protocol: String,

    /// Skip TLS verification (for self-signed certs)
    pub insecure: bool,

    /// Is this the default relay
    pub is_default: bool,

    /// Supported tunnel protocols (JSON array: ["http", "https", "tcp", "tls"])
    #[sea_orm(column_type = "Text")]
    pub supported_protocols: String,

    /// Creation timestamp
    pub created_at: ChronoDateTimeUtc,

    /// Last update timestamp
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::tunnel_config::Entity")]
    TunnelConfigs,
}

impl Related<super::tunnel_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TunnelConfigs.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
