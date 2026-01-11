//! CapturedRequest entity for storing HTTP request/response data

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "captured_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    pub localup_id: String,
    pub method: String,
    pub path: String,
    pub host: Option<String>,

    /// JSON-encoded headers: Vec<(String, String)>
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

    /// Request timestamp (for TimescaleDB hypertable)
    pub created_at: ChronoDateTimeUtc,

    /// Response timestamp
    pub responded_at: Option<ChronoDateTimeUtc>,

    /// Latency in milliseconds
    pub latency_ms: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
