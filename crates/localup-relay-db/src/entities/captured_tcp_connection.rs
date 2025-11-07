//! Captured TCP connection entity

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "captured_tcp_connections")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "String(StringLen::None)")]
    pub id: String,

    #[sea_orm(column_type = "String(StringLen::None)")]
    pub localup_id: String,

    #[sea_orm(column_type = "String(StringLen::None)")]
    pub client_addr: String,

    pub target_port: i32,

    pub bytes_received: i64,

    pub bytes_sent: i64,

    pub connected_at: DateTimeWithTimeZone,

    pub disconnected_at: Option<DateTimeWithTimeZone>,

    pub duration_ms: Option<i32>,

    #[sea_orm(column_type = "String(StringLen::None)", nullable)]
    pub disconnect_reason: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
