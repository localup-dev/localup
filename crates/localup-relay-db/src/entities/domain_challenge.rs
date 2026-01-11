//! DomainChallenge entity for storing pending ACME challenges

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Type of ACME challenge
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ChallengeType {
    /// HTTP-01 challenge
    #[sea_orm(string_value = "http01")]
    Http01,

    /// DNS-01 challenge
    #[sea_orm(string_value = "dns01")]
    Dns01,
}

/// Status of the challenge
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ChallengeStatus {
    /// Challenge pending validation
    #[sea_orm(string_value = "pending")]
    Pending,

    /// Challenge completed successfully
    #[sea_orm(string_value = "completed")]
    Completed,

    /// Challenge failed
    #[sea_orm(string_value = "failed")]
    Failed,

    /// Challenge expired
    #[sea_orm(string_value = "expired")]
    Expired,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "domain_challenges")]
pub struct Model {
    /// Unique challenge ID (primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    /// Domain name being validated
    #[sea_orm(indexed)]
    pub domain: String,

    /// Type of challenge (http01 or dns01)
    pub challenge_type: ChallengeType,

    /// Status of the challenge
    pub status: ChallengeStatus,

    /// For HTTP-01: the token
    /// For DNS-01: the record name (e.g., _acme-challenge.example.com)
    #[sea_orm(column_type = "Text", nullable)]
    pub token_or_record_name: Option<String>,

    /// For HTTP-01: the key authorization
    /// For DNS-01: the TXT record value
    #[sea_orm(column_type = "Text", nullable)]
    pub key_auth_or_record_value: Option<String>,

    /// ACME order URL for completing the challenge
    #[sea_orm(column_type = "Text", nullable)]
    pub order_url: Option<String>,

    /// When the challenge was created
    pub created_at: ChronoDateTimeUtc,

    /// When the challenge expires
    pub expires_at: ChronoDateTimeUtc,

    /// Error message if challenge failed
    #[sea_orm(column_type = "Text", nullable)]
    pub error_message: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
