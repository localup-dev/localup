//! Magic link token entity for passwordless email authentication

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "magic_link_tokens")]
pub struct Model {
    /// Token ID (UUID string, primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Email address the magic link was sent to
    pub email: String,

    /// When this token expires (15 minutes from creation)
    pub expires_at: ChronoDateTimeUtc,

    /// When this token was used (null if unused)
    pub used_at: Option<ChronoDateTimeUtc>,

    /// When this token was created
    pub created_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
