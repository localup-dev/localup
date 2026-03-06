//! Device authorization entity for OAuth 2.0 Device Authorization Grant (RFC 8628)

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_authorizations")]
pub struct Model {
    /// Unique ID (UUID string, primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Opaque device code sent to the client app (used for polling)
    #[sea_orm(unique)]
    pub device_code: String,

    /// Short human-readable code displayed to the user (e.g., "ABCD-1234")
    #[sea_orm(unique)]
    pub user_code: String,

    /// OAuth client_id of the requesting application
    pub client_id: String,

    /// Requested scopes (space-separated, per RFC)
    pub scope: Option<String>,

    /// Authorization status: "pending", "approved", "denied", "expired"
    pub status: String,

    /// User ID who approved the authorization (set when status = "approved")
    pub user_id: Option<String>,

    /// Minimum polling interval in seconds (default: 5)
    pub polling_interval: i32,

    /// Last time the client polled for a token (for slow_down enforcement)
    pub last_polled_at: Option<ChronoDateTimeUtc>,

    /// When this authorization expires
    pub expires_at: ChronoDateTimeUtc,

    /// When this authorization was created
    pub created_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
