//! AuthToken entity for long-lived API keys used in tunnel authentication

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "auth_token")]
pub struct Model {
    /// Auth token UUID (primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    /// User who owns this token
    pub user_id: Uuid,

    /// Team this token belongs to (optional)
    pub team_id: Option<Uuid>,

    /// User-defined name for this token
    pub name: String,

    /// Description of what this token is used for
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,

    /// SHA-256 hash of the JWT token
    #[sea_orm(unique)]
    pub token_hash: String,

    /// When the token was last used
    pub last_used_at: Option<ChronoDateTimeUtc>,

    /// When the token expires (NULL = never expires)
    pub expires_at: Option<ChronoDateTimeUtc>,

    /// Whether the token is active
    pub is_active: bool,

    /// When the token was created
    pub created_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Auth token belongs to a user
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,

    /// Auth token belongs to a team (optional)
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Team,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
