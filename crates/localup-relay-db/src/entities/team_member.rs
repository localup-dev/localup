//! TeamMember entity for team membership and roles

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Role of a team member
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum TeamRole {
    /// Team owner with full access
    #[sea_orm(string_value = "owner")]
    Owner,

    /// Team admin with elevated permissions
    #[sea_orm(string_value = "admin")]
    Admin,

    /// Regular team member
    #[sea_orm(string_value = "member")]
    Member,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "team_members")]
pub struct Model {
    /// Team UUID (composite primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub team_id: Uuid,

    /// User UUID (composite primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,

    /// Role of the user in this team
    pub role: TeamRole,

    /// When the user joined the team
    pub joined_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Team member belongs to a team
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Team,

    /// Team member belongs to a user
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
