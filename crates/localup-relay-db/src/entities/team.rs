//! Team entity for multi-tenancy and organization management

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "teams")]
pub struct Model {
    /// Team UUID (primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    /// Team name (unique, human-readable)
    #[sea_orm(unique)]
    pub name: String,

    /// Team slug (unique, URL-friendly)
    #[sea_orm(unique)]
    pub slug: String,

    /// User ID of the team owner
    pub owner_id: Uuid,

    /// When the team was created
    pub created_at: ChronoDateTimeUtc,

    /// When the team was last updated
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Team belongs to a user (owner)
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerId",
        to = "super::user::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Owner,

    /// Team has members
    #[sea_orm(has_many = "super::team_member::Entity")]
    Members,

    /// Team owns auth tokens
    #[sea_orm(has_many = "super::auth_token::Entity")]
    AuthTokens,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}

impl Related<super::team_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Members.def()
    }
}

impl Related<super::auth_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AuthTokens.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
