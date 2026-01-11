//! Wildcard domain pattern matching utilities
//!
//! Provides validation and matching for wildcard domain patterns.
//! Only single-level wildcards at the leftmost position are supported (e.g., `*.example.com`).
//!
//! # Supported patterns
//! - `*.example.com` - matches `api.example.com`, `web.example.com`
//! - `*.sub.example.com` - matches `api.sub.example.com`
//!
//! # Unsupported patterns (will be rejected)
//! - `**.example.com` - double asterisk
//! - `api.*.example.com` - mid-level wildcard
//! - `example.*` - right-side wildcard
//! - `*` - bare asterisk

use thiserror::Error;

/// Errors that can occur during wildcard pattern operations
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WildcardError {
    #[error("Invalid wildcard pattern: {0}")]
    InvalidPattern(String),

    #[error("Empty pattern")]
    EmptyPattern,

    #[error("Double asterisk patterns (**.domain) are not supported")]
    DoubleAsterisk,

    #[error("Mid-level wildcards (api.*.domain) are not supported")]
    MidLevelWildcard,

    #[error("Right-side wildcards (domain.*) are not supported")]
    RightSideWildcard,

    #[error("Bare asterisk (*) is not a valid pattern")]
    BareAsterisk,

    #[error("Pattern must have at least two domain parts after the wildcard")]
    InsufficientDomainParts,
}

/// A validated wildcard domain pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WildcardPattern {
    /// The full pattern string (e.g., "*.example.com")
    pattern: String,
    /// The base domain without the wildcard prefix (e.g., "example.com")
    base_domain: String,
}

impl WildcardPattern {
    /// Parse and validate a wildcard pattern string
    ///
    /// # Examples
    /// ```
    /// use localup_router::wildcard::WildcardPattern;
    ///
    /// let pattern = WildcardPattern::parse("*.example.com").unwrap();
    /// assert!(pattern.matches("api.example.com"));
    /// assert!(!pattern.matches("example.com"));
    /// ```
    pub fn parse(pattern: &str) -> Result<Self, WildcardError> {
        if pattern.is_empty() {
            return Err(WildcardError::EmptyPattern);
        }

        // Check for bare asterisk
        if pattern == "*" {
            return Err(WildcardError::BareAsterisk);
        }

        // Check for double asterisk
        if pattern.contains("**") {
            return Err(WildcardError::DoubleAsterisk);
        }

        // Check for right-side wildcard
        if pattern.ends_with(".*") || pattern.ends_with("*") && !pattern.starts_with("*.") {
            return Err(WildcardError::RightSideWildcard);
        }

        // Must start with *.
        if !pattern.starts_with("*.") {
            // Check for mid-level wildcard
            if pattern.contains(".*.")
                || pattern.contains("*.") && !pattern.starts_with("*.")
                || pattern.contains('*')
            {
                return Err(WildcardError::MidLevelWildcard);
            }
            return Err(WildcardError::InvalidPattern(
                "Pattern must start with *. for wildcard domains".to_string(),
            ));
        }

        // Extract base domain (everything after *.)
        let base_domain = &pattern[2..];

        // Base domain must have at least one dot (e.g., "example.com", not just "com")
        if !base_domain.contains('.') {
            return Err(WildcardError::InsufficientDomainParts);
        }

        // Validate base domain doesn't contain wildcards
        if base_domain.contains('*') {
            return Err(WildcardError::MidLevelWildcard);
        }

        // Validate base domain parts
        for part in base_domain.split('.') {
            if part.is_empty() {
                return Err(WildcardError::InvalidPattern(
                    "Domain parts cannot be empty".to_string(),
                ));
            }
        }

        Ok(Self {
            pattern: pattern.to_string(),
            base_domain: base_domain.to_string(),
        })
    }

    /// Check if a hostname matches this wildcard pattern
    ///
    /// Only matches single-level subdomains. For example, `*.example.com` matches
    /// `api.example.com` but NOT `sub.api.example.com`.
    pub fn matches(&self, hostname: &str) -> bool {
        // Must end with the base domain
        if !hostname.ends_with(&self.base_domain) {
            return false;
        }

        // Must have exactly one more subdomain level
        // e.g., for *.example.com, "api.example.com" has prefix "api"
        let prefix_len = hostname.len() - self.base_domain.len();

        // Must have a prefix (can't match the base domain itself)
        if prefix_len == 0 {
            return false;
        }

        // Prefix must end with a dot
        if prefix_len < 2 || !hostname[..prefix_len].ends_with('.') {
            return false;
        }

        // The subdomain part (without trailing dot)
        let subdomain = &hostname[..prefix_len - 1];

        // Subdomain must not contain dots (single-level only)
        !subdomain.contains('.')
    }

    /// Get the full pattern string
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Get the base domain without the wildcard prefix
    pub fn base_domain(&self) -> &str {
        &self.base_domain
    }

    /// Check if a given domain string is a wildcard pattern
    pub fn is_wildcard_pattern(domain: &str) -> bool {
        domain.starts_with("*.")
    }
}

impl std::fmt::Display for WildcardPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pattern)
    }
}

/// Extract the parent wildcard pattern from a hostname
///
/// For example, `api.example.com` returns `Some("*.example.com")`.
/// Returns `None` if the hostname doesn't have enough parts.
///
/// # Examples
/// ```
/// use localup_router::wildcard::extract_parent_wildcard;
///
/// assert_eq!(extract_parent_wildcard("api.example.com"), Some("*.example.com".to_string()));
/// assert_eq!(extract_parent_wildcard("sub.api.example.com"), Some("*.api.example.com".to_string()));
/// assert_eq!(extract_parent_wildcard("example.com"), None);
/// assert_eq!(extract_parent_wildcard("localhost"), None);
/// ```
pub fn extract_parent_wildcard(hostname: &str) -> Option<String> {
    // Find the first dot
    let first_dot = hostname.find('.')?;

    // Get the parent domain (everything after the first dot)
    let parent = &hostname[first_dot + 1..];

    // Parent must have at least one dot (be a valid domain)
    if !parent.contains('.') {
        return None;
    }

    Some(format!("*.{}", parent))
}

/// Check if a hostname could match any wildcard pattern
///
/// Returns true if the hostname has enough parts to potentially match a wildcard.
pub fn could_match_wildcard(hostname: &str) -> bool {
    // Count dots - need at least 2 for a wildcard match
    // e.g., "api.example.com" has 2 dots
    hostname.matches('.').count() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_wildcard_patterns() {
        assert!(WildcardPattern::parse("*.example.com").is_ok());
        assert!(WildcardPattern::parse("*.sub.example.com").is_ok());
        assert!(WildcardPattern::parse("*.localup.io").is_ok());
        assert!(WildcardPattern::parse("*.a.b.c.d.example.com").is_ok());
    }

    #[test]
    fn test_invalid_patterns() {
        // Double asterisk
        assert_eq!(
            WildcardPattern::parse("**.example.com"),
            Err(WildcardError::DoubleAsterisk)
        );

        // Mid-level wildcard
        assert_eq!(
            WildcardPattern::parse("api.*.example.com"),
            Err(WildcardError::MidLevelWildcard)
        );

        // Right-side wildcard
        assert_eq!(
            WildcardPattern::parse("example.*"),
            Err(WildcardError::RightSideWildcard)
        );

        // Bare asterisk
        assert_eq!(
            WildcardPattern::parse("*"),
            Err(WildcardError::BareAsterisk)
        );

        // Empty
        assert_eq!(WildcardPattern::parse(""), Err(WildcardError::EmptyPattern));

        // Insufficient domain parts (need at least 2 parts after wildcard)
        assert_eq!(
            WildcardPattern::parse("*.com"),
            Err(WildcardError::InsufficientDomainParts)
        );
    }

    #[test]
    fn test_wildcard_matching() {
        let pattern = WildcardPattern::parse("*.example.com").unwrap();

        // Should match single-level subdomains
        assert!(pattern.matches("api.example.com"));
        assert!(pattern.matches("web.example.com"));
        assert!(pattern.matches("a.example.com"));
        assert!(pattern.matches("test-123.example.com"));

        // Should NOT match multi-level subdomains
        assert!(!pattern.matches("sub.api.example.com"));
        assert!(!pattern.matches("deep.sub.api.example.com"));

        // Should NOT match the base domain itself
        assert!(!pattern.matches("example.com"));

        // Should NOT match different domains
        assert!(!pattern.matches("api.other.com"));
        assert!(!pattern.matches("api.example.org"));
    }

    #[test]
    fn test_nested_wildcard_matching() {
        let pattern = WildcardPattern::parse("*.api.example.com").unwrap();

        // Should match
        assert!(pattern.matches("v1.api.example.com"));
        assert!(pattern.matches("v2.api.example.com"));

        // Should NOT match
        assert!(!pattern.matches("api.example.com"));
        assert!(!pattern.matches("sub.v1.api.example.com"));
        assert!(!pattern.matches("web.example.com"));
    }

    #[test]
    fn test_extract_parent_wildcard() {
        assert_eq!(
            extract_parent_wildcard("api.example.com"),
            Some("*.example.com".to_string())
        );
        assert_eq!(
            extract_parent_wildcard("sub.api.example.com"),
            Some("*.api.example.com".to_string())
        );
        assert_eq!(
            extract_parent_wildcard("deep.sub.api.example.com"),
            Some("*.sub.api.example.com".to_string())
        );

        // Not enough parts
        assert_eq!(extract_parent_wildcard("example.com"), None);
        assert_eq!(extract_parent_wildcard("localhost"), None);
        assert_eq!(extract_parent_wildcard(""), None);
    }

    #[test]
    fn test_could_match_wildcard() {
        assert!(could_match_wildcard("api.example.com"));
        assert!(could_match_wildcard("sub.api.example.com"));

        assert!(!could_match_wildcard("example.com"));
        assert!(!could_match_wildcard("localhost"));
    }

    #[test]
    fn test_is_wildcard_pattern() {
        assert!(WildcardPattern::is_wildcard_pattern("*.example.com"));
        assert!(WildcardPattern::is_wildcard_pattern("*.sub.example.com"));

        assert!(!WildcardPattern::is_wildcard_pattern("example.com"));
        assert!(!WildcardPattern::is_wildcard_pattern("api.example.com"));
        assert!(!WildcardPattern::is_wildcard_pattern("**.example.com"));
    }

    #[test]
    fn test_pattern_display() {
        let pattern = WildcardPattern::parse("*.example.com").unwrap();
        assert_eq!(pattern.to_string(), "*.example.com");
        assert_eq!(pattern.as_str(), "*.example.com");
        assert_eq!(pattern.base_domain(), "example.com");
    }
}
