//! Authentication and authorization for tunnel system

pub mod jwt;
pub mod token;

pub use jwt::{JwtClaims, JwtError, JwtValidator};
pub use token::{Token, TokenError, TokenGenerator};

// Re-export useful types
pub use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
