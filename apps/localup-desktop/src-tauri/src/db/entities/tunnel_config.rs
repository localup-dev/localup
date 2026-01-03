//! TunnelConfig entity for storing tunnel configurations

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tunnel_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Display name for the tunnel
    pub name: String,

    /// Foreign key to relay_servers
    pub relay_server_id: String,

    /// Local host to forward to (default: localhost)
    pub local_host: String,

    /// Local port to forward to
    pub local_port: i32,

    /// Tunnel protocol (tcp, http, https, tls)
    pub protocol: String,

    /// Subdomain for HTTP/HTTPS tunnels
    #[sea_orm(column_type = "Text", nullable)]
    pub subdomain: Option<String>,

    /// Custom domain for HTTPS tunnels
    #[sea_orm(column_type = "Text", nullable)]
    pub custom_domain: Option<String>,

    /// Auto-start this tunnel on app launch
    pub auto_start: bool,

    /// Is this tunnel enabled
    pub enabled: bool,

    /// Creation timestamp
    pub created_at: ChronoDateTimeUtc,

    /// Last update timestamp
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::relay_server::Entity",
        from = "Column::RelayServerId",
        to = "super::relay_server::Column::Id"
    )]
    RelayServer,
    #[sea_orm(has_many = "super::tunnel_session::Entity")]
    TunnelSessions,
}

impl Related<super::relay_server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RelayServer.def()
    }
}

impl Related<super::tunnel_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TunnelSessions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
