//! Password hashing and verification using Argon2id

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

/// Error types for password operations
#[derive(Error, Debug)]
pub enum PasswordError {
    /// Failed to hash password
    #[error("Failed to hash password: {0}")]
    HashingFailed(String),

    /// Failed to verify password
    #[error("Failed to verify password: {0}")]
    VerificationFailed(String),

    /// Invalid password hash format
    #[error("Invalid password hash format: {0}")]
    InvalidHashFormat(String),
}

/// Hash a password using Argon2id
///
/// This uses the OWASP-recommended Argon2id algorithm with secure defaults:
/// - Memory cost: 19456 KiB (19 MiB)
/// - Time cost: 2 iterations
/// - Parallelism: 1 thread
/// - Salt: 16 bytes (randomly generated)
///
/// # Arguments
/// * `password` - The plain text password to hash
///
/// # Returns
/// * `Ok(String)` - PHC-formatted hash string (suitable for storage)
/// * `Err(PasswordError)` - If hashing fails
///
/// # Example
/// ```
/// use localup_auth::password::hash_password;
///
/// let hash = hash_password("MySecurePassword123!").unwrap();
/// println!("Hash: {}", hash);
/// // Hash: $argon2id$v=19$m=19456,t=2,p=1$...
/// ```
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);

    // Use Argon2 with default params (Argon2id variant)
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| PasswordError::HashingFailed(e.to_string()))?;

    Ok(password_hash.to_string())
}

/// Verify a password against a hash
///
/// # Arguments
/// * `password` - The plain text password to verify
/// * `hash` - The PHC-formatted hash string (from database)
///
/// # Returns
/// * `Ok(true)` - Password matches hash
/// * `Ok(false)` - Password does not match hash
/// * `Err(PasswordError)` - If hash format is invalid or verification fails
///
/// # Example
/// ```
/// use localup_auth::password::{hash_password, verify_password};
///
/// let hash = hash_password("MyPassword123!").unwrap();
/// assert!(verify_password("MyPassword123!", &hash).unwrap());
/// assert!(!verify_password("WrongPassword", &hash).unwrap());
/// ```
pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| PasswordError::InvalidHashFormat(e.to_string()))?;

    let argon2 = Argon2::default();

    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(PasswordError::VerificationFailed(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_produces_valid_hash() {
        let password = "TestPassword123!";
        let hash = hash_password(password).expect("Failed to hash password");

        // Verify hash format starts with $argon2id$
        assert!(hash.starts_with("$argon2id$"));

        // Verify hash contains version, params, salt, and hash
        assert!(hash.contains("v=19"));
        assert!(hash.contains("m="));
        assert!(hash.contains("t="));
        assert!(hash.contains("p="));
    }

    #[test]
    fn test_verify_password_correct() {
        let password = "CorrectPassword123!";
        let hash = hash_password(password).expect("Failed to hash password");

        let result = verify_password(password, &hash).expect("Verification failed");
        assert!(result, "Correct password should verify");
    }

    #[test]
    fn test_verify_password_incorrect() {
        let password = "CorrectPassword123!";
        let wrong_password = "WrongPassword123!";
        let hash = hash_password(password).expect("Failed to hash password");

        let result = verify_password(wrong_password, &hash).expect("Verification failed");
        assert!(!result, "Wrong password should not verify");
    }

    #[test]
    fn test_verify_password_invalid_hash() {
        let result = verify_password("AnyPassword", "invalid_hash_format");
        assert!(result.is_err(), "Invalid hash should return error");
        assert!(matches!(result, Err(PasswordError::InvalidHashFormat(_))));
    }

    #[test]
    fn test_hash_password_different_salts() {
        let password = "SamePassword123!";
        let hash1 = hash_password(password).expect("Failed to hash password");
        let hash2 = hash_password(password).expect("Failed to hash password");

        // Same password should produce different hashes (different salts)
        assert_ne!(hash1, hash2, "Hashes should differ due to random salts");

        // But both should verify correctly
        assert!(verify_password(password, &hash1).unwrap());
        assert!(verify_password(password, &hash2).unwrap());
    }

    #[test]
    fn test_hash_password_empty() {
        let hash = hash_password("").expect("Failed to hash empty password");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("", &hash).unwrap());
    }

    #[test]
    fn test_hash_password_unicode() {
        let password = "🔐Password123!日本語";
        let hash = hash_password(password).expect("Failed to hash unicode password");
        assert!(verify_password(password, &hash).unwrap());
    }

    #[test]
    fn test_verify_password_case_sensitive() {
        let password = "TestPassword123!";
        let hash = hash_password(password).expect("Failed to hash password");

        assert!(verify_password("TestPassword123!", &hash).unwrap());
        assert!(!verify_password("testpassword123!", &hash).unwrap());
        assert!(!verify_password("TESTPASSWORD123!", &hash).unwrap());
    }
}
