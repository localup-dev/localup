//! TunnelSession entity for storing runtime tunnel state

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tunnel_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Foreign key to tunnel_configs
    pub config_id: String,

    /// Current status (connecting, connected, disconnected, error)
    pub status: String,

    /// Public URL when connected
    #[sea_orm(column_type = "Text", nullable)]
    pub public_url: Option<String>,

    /// LocalUp ID assigned by relay
    #[sea_orm(column_type = "Text", nullable)]
    pub localup_id: Option<String>,

    /// Connection timestamp
    pub connected_at: Option<ChronoDateTimeUtc>,

    /// Error message if status is "error"
    #[sea_orm(column_type = "Text", nullable)]
    pub error_message: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tunnel_config::Entity",
        from = "Column::ConfigId",
        to = "super::tunnel_config::Column::Id"
    )]
    TunnelConfig,
    #[sea_orm(has_many = "super::captured_request::Entity")]
    CapturedRequests,
}

impl Related<super::tunnel_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TunnelConfig.def()
    }
}

impl Related<super::captured_request::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CapturedRequests.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
