//! User entity for authentication and user management

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// User role in the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum UserRole {
    /// System administrator with full access
    #[sea_orm(string_value = "admin")]
    Admin,

    /// Regular user
    #[sea_orm(string_value = "user")]
    User,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// User UUID (primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,

    /// User email (unique)
    #[sea_orm(unique)]
    pub email: String,

    /// Argon2id password hash
    pub password_hash: String,

    /// User's full name (optional)
    pub full_name: Option<String>,

    /// User role (admin or user)
    pub role: UserRole,

    /// Whether the user account is active
    pub is_active: bool,

    /// When the user account was created
    pub created_at: ChronoDateTimeUtc,

    /// When the user last updated their profile
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// User owns teams
    #[sea_orm(has_many = "super::team::Entity")]
    Teams,

    /// User is a member of teams
    #[sea_orm(has_many = "super::team_member::Entity")]
    TeamMemberships,

    /// User owns auth tokens
    #[sea_orm(has_many = "super::auth_token::Entity")]
    AuthTokens,
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Teams.def()
    }
}

impl Related<super::team_member::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TeamMemberships.def()
    }
}

impl Related<super::auth_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AuthTokens.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
