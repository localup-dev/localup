//! CustomDomain entity for storing custom domain certificate information

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Status of custom domain certificate provisioning
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum DomainStatus {
    /// Certificate provisioning in progress
    #[sea_orm(string_value = "pending")]
    Pending,

    /// Certificate active and valid
    #[sea_orm(string_value = "active")]
    Active,

    /// Certificate expired
    #[sea_orm(string_value = "expired")]
    Expired,

    /// Certificate provisioning failed
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "custom_domains")]
pub struct Model {
    /// Domain name (primary key)
    #[sea_orm(primary_key, auto_increment = false)]
    pub domain: String,

    /// Unique ID for URL routing
    #[sea_orm(column_type = "String(StringLen::N(36))", nullable)]
    pub id: Option<String>,

    /// Path to certificate file
    pub cert_path: Option<String>,

    /// Path to private key file
    pub key_path: Option<String>,

    /// Certificate status
    pub status: DomainStatus,

    /// When the certificate was provisioned
    pub provisioned_at: ChronoDateTimeUtc,

    /// When the certificate expires
    pub expires_at: Option<ChronoDateTimeUtc>,

    /// Whether to automatically renew the certificate
    pub auto_renew: bool,

    /// Error message if provisioning failed
    #[sea_orm(column_type = "Text", nullable)]
    pub error_message: Option<String>,

    /// Certificate in PEM format (stored directly in database)
    #[sea_orm(column_type = "Text", nullable)]
    pub cert_pem: Option<String>,

    /// Private key in PEM format (stored directly in database)
    #[sea_orm(column_type = "Text", nullable)]
    pub key_pem: Option<String>,

    /// Whether this is a wildcard domain (e.g., *.example.com)
    #[sea_orm(default_value = false)]
    pub is_wildcard: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
