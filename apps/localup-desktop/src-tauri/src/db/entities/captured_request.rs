//! CapturedRequest entity for storing HTTP request/response data

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "captured_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Foreign key to tunnel_sessions
    pub tunnel_session_id: String,

    /// LocalUp ID (for filtering)
    pub localup_id: String,

    /// HTTP method
    pub method: String,

    /// Request path
    pub path: String,

    /// Host header
    #[sea_orm(column_type = "Text", nullable)]
    pub host: Option<String>,

    /// JSON-encoded request headers
    #[sea_orm(column_type = "Text")]
    pub headers: String,

    /// Request body (base64 if binary)
    #[sea_orm(column_type = "Text", nullable)]
    pub body: Option<String>,

    /// Response status code
    pub status: Option<i32>,

    /// JSON-encoded response headers
    #[sea_orm(column_type = "Text", nullable)]
    pub response_headers: Option<String>,

    /// Response body (base64 if binary)
    #[sea_orm(column_type = "Text", nullable)]
    pub response_body: Option<String>,

    /// Request timestamp
    pub created_at: ChronoDateTimeUtc,

    /// Latency in milliseconds
    pub latency_ms: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tunnel_session::Entity",
        from = "Column::TunnelSessionId",
        to = "super::tunnel_session::Column::Id"
    )]
    TunnelSession,
}

impl Related<super::tunnel_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TunnelSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
