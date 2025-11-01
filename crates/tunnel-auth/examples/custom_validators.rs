//! Example custom authentication validators
//!
//! This example shows how to implement the `AuthValidator` trait
//! for different authentication strategies (API keys, database lookup, custom logic).
//!
//! Run with: cargo run -p tunnel-auth --example custom_validators

use async_trait::async_trait;
use std::collections::HashMap;
use tunnel_auth::{AuthError, AuthResult, AuthValidator};

// ============================================================================
// Example 1: API Key Validator
// ============================================================================

/// Simple API key validator that checks against a hashmap
pub struct ApiKeyValidator {
    /// Map of API keys to tunnel IDs
    valid_keys: HashMap<String, ApiKeyInfo>,
}

struct ApiKeyInfo {
    tunnel_id: String,
    user_id: String,
    allowed_protocols: Vec<String>,
}

impl ApiKeyValidator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ApiKeyValidator {
    fn default() -> Self {
        let mut valid_keys = HashMap::new();

        // Add some example API keys
        valid_keys.insert(
            "sk_test_123456".to_string(),
            ApiKeyInfo {
                tunnel_id: "tunnel_user1_http_3000".to_string(),
                user_id: "user1".to_string(),
                allowed_protocols: vec!["http".to_string(), "https".to_string()],
            },
        );

        valid_keys.insert(
            "sk_test_789012".to_string(),
            ApiKeyInfo {
                tunnel_id: "tunnel_user2_tcp_5432".to_string(),
                user_id: "user2".to_string(),
                allowed_protocols: vec!["tcp".to_string()],
            },
        );

        Self { valid_keys }
    }
}

#[async_trait]
impl AuthValidator for ApiKeyValidator {
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
        match self.valid_keys.get(token) {
            Some(info) => Ok(AuthResult::new(info.tunnel_id.clone())
                .with_user_id(info.user_id.clone())
                .with_protocols(info.allowed_protocols.clone())
                .with_metadata("auth_type".to_string(), "api_key".to_string())),
            None => Err(AuthError::InvalidToken("Unknown API key".to_string())),
        }
    }
}

// ============================================================================
// Example 2: Database Validator (Mock)
// ============================================================================

/// Database-backed validator that checks subscription status
///
/// In a real implementation, this would query your database
pub struct DatabaseValidator {
    // In real code, this would be a database connection pool
    _db_url: String,
}

impl DatabaseValidator {
    pub fn new(db_url: String) -> Self {
        Self { _db_url: db_url }
    }
}

#[async_trait]
impl AuthValidator for DatabaseValidator {
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
        // In real code, you would:
        // 1. Parse the token (could be UUID, signed token, etc.)
        // 2. Query database for user
        // 3. Check subscription status
        // 4. Check quota/rate limits
        // 5. Return AuthResult with user's permissions

        // Mock implementation
        if token.starts_with("db_") {
            let user_id = token.strip_prefix("db_").unwrap();
            let tunnel_id = format!("tunnel_{}_http_3000", user_id);

            // Mock database lookup result
            Ok(AuthResult::new(tunnel_id)
                .with_user_id(user_id.to_string())
                .with_metadata("plan".to_string(), "pro".to_string())
                .with_metadata("quota_remaining".to_string(), "1000".to_string()))
        } else {
            Err(AuthError::InvalidToken(
                "Invalid database token".to_string(),
            ))
        }
    }
}

// ============================================================================
// Example 3: Multi-Strategy Validator
// ============================================================================

/// Validator that tries multiple strategies in order
pub struct MultiStrategyValidator {
    strategies: Vec<Box<dyn AuthValidator>>,
}

impl MultiStrategyValidator {
    pub fn new(strategies: Vec<Box<dyn AuthValidator>>) -> Self {
        Self { strategies }
    }
}

#[async_trait]
impl AuthValidator for MultiStrategyValidator {
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
        let mut last_error = AuthError::InvalidToken("No strategies configured".to_string());

        for strategy in &self.strategies {
            match strategy.validate(token).await {
                Ok(result) => return Ok(result),
                Err(e) => last_error = e,
            }
        }

        Err(last_error)
    }
}

// ============================================================================
// Example 4: Rate-Limited Validator (Decorator Pattern)
// ============================================================================

/// Wrapper validator that adds rate limiting
pub struct RateLimitedValidator<V: AuthValidator> {
    inner: V,
    // In real code: rate limiter state (leaky bucket, token bucket, etc.)
}

impl<V: AuthValidator> RateLimitedValidator<V> {
    pub fn new(inner: V) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<V: AuthValidator + Send + Sync> AuthValidator for RateLimitedValidator<V> {
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
        // In real code: check rate limit before calling inner validator
        // For now, just pass through
        self.inner.validate(token).await
    }
}

// ============================================================================
// Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Custom Authentication Validators Examples\n");
    println!("==========================================\n");

    // Example 1: API Key Validator
    println!("1. API Key Validator");
    let api_validator = ApiKeyValidator::new();

    match api_validator.validate("sk_test_123456").await {
        Ok(result) => println!(
            "   ✅ Valid: tunnel_id={}, user_id={:?}, protocols={:?}",
            result.tunnel_id, result.user_id, result.allowed_protocols
        ),
        Err(e) => println!("   ❌ Error: {}", e),
    }

    match api_validator.validate("invalid_key").await {
        Ok(_) => println!("   ❌ Should have failed!"),
        Err(e) => println!("   ✅ Correctly rejected: {}", e),
    }

    // Example 2: Database Validator
    println!("\n2. Database Validator (Mock)");
    let db_validator = DatabaseValidator::new("postgres://localhost/mydb".to_string());

    match db_validator.validate("db_user123").await {
        Ok(result) => println!(
            "   ✅ Valid: tunnel_id={}, plan={:?}",
            result.tunnel_id,
            result.get_metadata("plan")
        ),
        Err(e) => println!("   ❌ Error: {}", e),
    }

    // Example 3: Multi-Strategy Validator
    println!("\n3. Multi-Strategy Validator");
    let multi_validator =
        MultiStrategyValidator::new(vec![Box::new(api_validator), Box::new(db_validator)]);

    // Try API key (should succeed with first strategy)
    match multi_validator.validate("sk_test_123456").await {
        Ok(result) => println!("   ✅ API key accepted: {}", result.tunnel_id),
        Err(e) => println!("   ❌ Error: {}", e),
    }

    // Try database token (should succeed with second strategy)
    match multi_validator.validate("db_user456").await {
        Ok(result) => println!("   ✅ DB token accepted: {}", result.tunnel_id),
        Err(e) => println!("   ❌ Error: {}", e),
    }

    println!("\nIntegration with Exit Nodes");
    println!("============================\n");
    println!("To use a custom validator with an exit node:");
    println!();
    println!("```rust");
    println!("// Create your custom validator");
    println!("let validator: Arc<dyn AuthValidator> = Arc::new(ApiKeyValidator::new());");
    println!();
    println!("// Use it in your control plane");
    println!("async fn handle_tunnel_connection(");
    println!("    connection: impl TransportConnection,");
    println!("    validator: Arc<dyn AuthValidator>,");
    println!(") {{");
    println!("    let token = receive_auth_token(&connection).await?;");
    println!("    let auth_result = validator.validate(&token).await?;");
    println!("    // Register tunnel with auth_result.tunnel_id");
    println!("}}");
    println!("```");

    Ok(())
}
