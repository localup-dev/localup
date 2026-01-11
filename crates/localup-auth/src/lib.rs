//! Authentication and authorization for tunnel system

pub mod jwt;
pub mod password;
pub mod token;
pub mod validator;

pub use jwt::{JwtClaims, JwtError, JwtValidator};
pub use password::{hash_password, verify_password, PasswordError};
pub use token::{Token, TokenError, TokenGenerator};
pub use validator::{AuthError, AuthResult, AuthValidator};

// Re-export useful types
pub use async_trait::async_trait;
pub use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
