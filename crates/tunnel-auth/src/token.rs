//! Token generation and validation

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use thiserror::Error;

/// Authentication token
#[derive(Debug, Clone, PartialEq)]
pub struct Token(String);

impl Token {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for Token {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<Token> for String {
    fn from(token: Token) -> Self {
        token.0
    }
}

/// Token errors
#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Invalid token format")]
    InvalidFormat,

    #[error("Token generation failed")]
    GenerationFailed,

    #[error("Base64 decode error: {0}")]
    Base64Error(#[from] base64::DecodeError),
}

/// Token generator
pub struct TokenGenerator;

impl TokenGenerator {
    /// Generate a random token
    pub fn generate() -> Token {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let random_bytes = format!("{:x}", now);
        let encoded = URL_SAFE_NO_PAD.encode(random_bytes.as_bytes());

        Token(encoded)
    }

    /// Generate a token with prefix
    pub fn generate_with_prefix(prefix: &str) -> Token {
        let token = Self::generate();
        Token(format!("{}_{}", prefix, token.0))
    }

    /// Validate token format (basic check)
    pub fn validate_format(token: &Token) -> Result<(), TokenError> {
        if token.0.is_empty() {
            return Err(TokenError::InvalidFormat);
        }

        // Check if it's valid base64 (after removing prefix if any)
        let token_str = if let Some((_prefix, value)) = token.0.split_once('_') {
            value
        } else {
            &token.0
        };

        URL_SAFE_NO_PAD
            .decode(token_str)
            .map_err(TokenError::Base64Error)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token1 = TokenGenerator::generate();
        let token2 = TokenGenerator::generate();

        // Tokens should be different
        assert_ne!(token1, token2);

        // Should be valid format
        assert!(TokenGenerator::validate_format(&token1).is_ok());
    }

    #[test]
    fn test_token_with_prefix() {
        let token = TokenGenerator::generate_with_prefix("tunnel");

        assert!(token.as_str().starts_with("tunnel_"));
        assert!(TokenGenerator::validate_format(&token).is_ok());
    }

    #[test]
    fn test_token_validation() {
        let valid_token = TokenGenerator::generate();
        assert!(TokenGenerator::validate_format(&valid_token).is_ok());

        let invalid_token = Token::new("not-valid-base64!@#$".to_string());
        assert!(TokenGenerator::validate_format(&invalid_token).is_err());

        let empty_token = Token::new(String::new());
        assert!(TokenGenerator::validate_format(&empty_token).is_err());
    }

    #[test]
    fn test_token_conversion() {
        let token_str = "test_token_123".to_string();
        let token: Token = token_str.clone().into();

        assert_eq!(token.as_str(), token_str);
        assert_eq!(token.into_string(), token_str);
    }
}
