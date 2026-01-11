//! Route registry for managing tunnel routes with reconnection support
//!
//! Supports wildcard domain patterns with fallback matching:
//! - Exact match is tried first
//! - If no exact match, wildcard patterns are checked
//! - Wildcard patterns use `*.domain.tld` format

use crate::wildcard::{extract_parent_wildcard, WildcardPattern};
use crate::RouteKey;
use dashmap::DashMap;
use localup_proto::IpFilter;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tracing::trace;

/// Route target information
#[derive(Debug, Clone)]
pub struct RouteTarget {
    /// Tunnel ID
    pub localup_id: String,
    /// Target address (e.g., "localhost:3000")
    pub target_addr: String,
    /// Additional metadata
    pub metadata: Option<String>,
    /// IP filter for access control
    /// Empty filter allows all connections (default)
    pub ip_filter: IpFilter,
}

impl RouteTarget {
    /// Check if the given peer address is allowed to access this route
    pub fn is_ip_allowed(&self, peer_addr: &SocketAddr) -> bool {
        self.ip_filter.is_socket_allowed(peer_addr)
    }
}

// Future: Route registration with state for reconnection support
// #[derive(Debug, Clone)]
// struct RouteEntry {
//     target: RouteTarget,
//     state: RouteState,
// }
//
// #[derive(Debug, Clone)]
// enum RouteState {
//     Active,
//     Reserved { until: DateTime<Utc> },
// }

/// Route registry errors
#[derive(Debug, Error)]
pub enum RouteError {
    #[error("Route not found: {0:?}")]
    RouteNotFound(RouteKey),

    #[error("Route already exists: {0:?}")]
    RouteAlreadyExists(RouteKey),

    #[error("Invalid route key")]
    InvalidRouteKey,

    #[error("Invalid wildcard pattern: {0}")]
    InvalidWildcardPattern(String),
}

/// Route registry for managing tunnel routes
///
/// Supports both exact and wildcard route matching for HTTP hosts.
/// Wildcard routes use `*.domain.tld` format and are stored separately
/// for efficient lookup with fallback.
pub struct RouteRegistry {
    /// Exact routes (including exact matches for hostnames)
    routes: Arc<DashMap<RouteKey, RouteTarget>>,
    /// Wildcard routes (e.g., *.example.com) - stored separately for fallback lookup
    wildcard_routes: Arc<DashMap<String, RouteTarget>>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
            wildcard_routes: Arc::new(DashMap::new()),
        }
    }

    /// Register a route (exact match)
    pub fn register(&self, key: RouteKey, target: RouteTarget) -> Result<(), RouteError> {
        if self.routes.contains_key(&key) {
            return Err(RouteError::RouteAlreadyExists(key));
        }

        self.routes.insert(key, target);
        Ok(())
    }

    /// Register a wildcard route (e.g., *.example.com)
    ///
    /// Wildcard routes are used as fallback when no exact match is found.
    /// Only `*.domain.tld` format is supported.
    pub fn register_wildcard(&self, pattern: &str, target: RouteTarget) -> Result<(), RouteError> {
        // Validate the pattern
        let validated = WildcardPattern::parse(pattern)
            .map_err(|e| RouteError::InvalidWildcardPattern(e.to_string()))?;

        let pattern_str = validated.as_str().to_string();

        if self.wildcard_routes.contains_key(&pattern_str) {
            return Err(RouteError::RouteAlreadyExists(RouteKey::HttpHost(
                pattern_str,
            )));
        }

        trace!(
            "Registering wildcard route: {} -> {}",
            pattern,
            target.localup_id
        );
        self.wildcard_routes.insert(pattern_str, target);
        Ok(())
    }

    /// Lookup a route with wildcard fallback
    ///
    /// Priority order:
    /// 1. Exact match
    /// 2. Wildcard match (for HTTP hosts only)
    /// 3. Not found
    pub fn lookup(&self, key: &RouteKey) -> Result<RouteTarget, RouteError> {
        // Try exact match first
        if let Some(entry) = self.routes.get(key) {
            trace!("Found exact route match for {:?}", key);
            return Ok(entry.value().clone());
        }

        // For HTTP hosts, try wildcard fallback
        if let RouteKey::HttpHost(host) = key {
            if let Some(target) = self.lookup_wildcard(host) {
                trace!("Found wildcard route match for {}", host);
                return Ok(target);
            }
        }

        // For TLS SNI, also try wildcard fallback
        if let RouteKey::TlsSni(sni) = key {
            if let Some(target) = self.lookup_wildcard(sni) {
                trace!("Found wildcard route match for SNI {}", sni);
                return Ok(target);
            }
        }

        Err(RouteError::RouteNotFound(key.clone()))
    }

    /// Lookup a wildcard route for a hostname
    ///
    /// Tries to find a matching wildcard pattern by extracting the parent wildcard.
    pub fn lookup_wildcard(&self, hostname: &str) -> Option<RouteTarget> {
        // Extract potential wildcard pattern (e.g., api.example.com -> *.example.com)
        let wildcard = extract_parent_wildcard(hostname)?;

        // Check if we have a matching wildcard route
        if let Some(entry) = self.wildcard_routes.get(&wildcard) {
            // Verify the pattern actually matches (for safety)
            if let Ok(pattern) = WildcardPattern::parse(&wildcard) {
                if pattern.matches(hostname) {
                    return Some(entry.value().clone());
                }
            }
        }

        None
    }

    /// Unregister a route (exact match only)
    pub fn unregister(&self, key: &RouteKey) -> Result<RouteTarget, RouteError> {
        self.routes
            .remove(key)
            .map(|(_, target)| target)
            .ok_or_else(|| RouteError::RouteNotFound(key.clone()))
    }

    /// Unregister a wildcard route
    pub fn unregister_wildcard(&self, pattern: &str) -> Result<RouteTarget, RouteError> {
        self.wildcard_routes
            .remove(pattern)
            .map(|(_, target)| target)
            .ok_or_else(|| RouteError::RouteNotFound(RouteKey::HttpHost(pattern.to_string())))
    }

    /// Check if a route exists (exact match)
    pub fn exists(&self, key: &RouteKey) -> bool {
        self.routes.contains_key(key)
    }

    /// Check if a wildcard route exists
    pub fn wildcard_exists(&self, pattern: &str) -> bool {
        self.wildcard_routes.contains_key(pattern)
    }

    /// Get a wildcard route target by its exact pattern
    ///
    /// Returns the RouteTarget if the wildcard pattern is registered.
    pub fn get_wildcard_target(&self, pattern: &str) -> Option<RouteTarget> {
        self.wildcard_routes.get(pattern).map(|r| r.value().clone())
    }

    /// Check if a route exists (including wildcard fallback)
    pub fn exists_with_wildcard(&self, key: &RouteKey) -> bool {
        if self.routes.contains_key(key) {
            return true;
        }

        // Check wildcard for HTTP hosts
        if let RouteKey::HttpHost(host) = key {
            if let Some(wildcard) = extract_parent_wildcard(host) {
                return self.wildcard_routes.contains_key(&wildcard);
            }
        }

        // Check wildcard for TLS SNI
        if let RouteKey::TlsSni(sni) = key {
            if let Some(wildcard) = extract_parent_wildcard(sni) {
                return self.wildcard_routes.contains_key(&wildcard);
            }
        }

        false
    }

    /// Get all routes (exact matches only)
    pub fn all_routes(&self) -> Vec<(RouteKey, RouteTarget)> {
        self.routes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get all wildcard routes
    pub fn all_wildcard_routes(&self) -> Vec<(String, RouteTarget)> {
        self.wildcard_routes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get number of registered routes (exact matches)
    pub fn count(&self) -> usize {
        self.routes.len()
    }

    /// Get number of registered wildcard routes
    pub fn wildcard_count(&self) -> usize {
        self.wildcard_routes.len()
    }

    /// Get total number of routes (exact + wildcard)
    pub fn total_count(&self) -> usize {
        self.routes.len() + self.wildcard_routes.len()
    }

    /// Clear all routes (exact and wildcard)
    pub fn clear(&self) {
        self.routes.clear();
        self.wildcard_routes.clear();
    }
}

impl Default for RouteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_register_lookup() {
        let registry = RouteRegistry::new();
        let key = RouteKey::TcpPort(5432);
        let target = RouteTarget {
            localup_id: "localup-1".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target.clone()).unwrap();

        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "localup-1");
        assert_eq!(found.target_addr, "localhost:5432");
    }

    #[test]
    fn test_registry_duplicate() {
        let registry = RouteRegistry::new();
        let key = RouteKey::HttpHost("example.com".to_string());
        let target = RouteTarget {
            localup_id: "localup-1".to_string(),
            target_addr: "localhost:3000".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target.clone()).unwrap();

        let result = registry.register(key, target);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_unregister() {
        let registry = RouteRegistry::new();
        let key = RouteKey::TlsSni("db.example.com".to_string());
        let target = RouteTarget {
            localup_id: "localup-1".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target).unwrap();
        assert_eq!(registry.count(), 1);

        registry.unregister(&key).unwrap();
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_not_found() {
        let registry = RouteRegistry::new();
        let key = RouteKey::TcpPort(8080);

        let result = registry.lookup(&key);
        assert!(result.is_err());
    }

    #[test]
    fn test_route_target_ip_filter() {
        let target = RouteTarget {
            localup_id: "localup-1".to_string(),
            target_addr: "localhost:3000".to_string(),
            metadata: None,
            ip_filter: IpFilter::from_allowlist(vec!["192.168.1.0/24".to_string()]).unwrap(),
        };

        let allowed_addr: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let denied_addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();

        assert!(target.is_ip_allowed(&allowed_addr));
        assert!(!target.is_ip_allowed(&denied_addr));
    }

    #[test]
    fn test_route_target_empty_filter() {
        let target = RouteTarget {
            localup_id: "localup-1".to_string(),
            target_addr: "localhost:3000".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        // Empty filter allows all IPs
        let addr1: SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let addr2: SocketAddr = "10.0.0.1:12345".parse().unwrap();

        assert!(target.is_ip_allowed(&addr1));
        assert!(target.is_ip_allowed(&addr2));
    }

    #[test]
    fn test_wildcard_registration() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        assert!(registry.wildcard_exists("*.example.com"));
        assert_eq!(registry.wildcard_count(), 1);
    }

    #[test]
    fn test_wildcard_invalid_pattern() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-1".to_string(),
            target_addr: "tunnel:tunnel-1".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        // Double asterisk should fail
        assert!(registry
            .register_wildcard("**.example.com", target.clone())
            .is_err());

        // Mid-level wildcard should fail
        assert!(registry
            .register_wildcard("api.*.example.com", target.clone())
            .is_err());

        // Bare asterisk should fail
        assert!(registry.register_wildcard("*", target).is_err());
    }

    #[test]
    fn test_wildcard_duplicate() {
        let registry = RouteRegistry::new();
        let target1 = RouteTarget {
            localup_id: "tunnel-1".to_string(),
            target_addr: "tunnel:tunnel-1".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };
        let target2 = RouteTarget {
            localup_id: "tunnel-2".to_string(),
            target_addr: "tunnel:tunnel-2".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry
            .register_wildcard("*.example.com", target1)
            .unwrap();

        let result = registry.register_wildcard("*.example.com", target2);
        assert!(result.is_err());
    }

    #[test]
    fn test_wildcard_lookup_fallback() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        // Should find via wildcard fallback
        let key = RouteKey::HttpHost("api.example.com".to_string());
        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "tunnel-wildcard");

        // Different subdomain should also match
        let key2 = RouteKey::HttpHost("web.example.com".to_string());
        let found2 = registry.lookup(&key2).unwrap();
        assert_eq!(found2.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_exact_match_beats_wildcard() {
        let registry = RouteRegistry::new();

        // Register wildcard first
        let wildcard_target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };
        registry
            .register_wildcard("*.example.com", wildcard_target)
            .unwrap();

        // Register exact match
        let exact_target = RouteTarget {
            localup_id: "tunnel-api".to_string(),
            target_addr: "tunnel:tunnel-api".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };
        let exact_key = RouteKey::HttpHost("api.example.com".to_string());
        registry.register(exact_key.clone(), exact_target).unwrap();

        // Exact match should win
        let found = registry.lookup(&exact_key).unwrap();
        assert_eq!(found.localup_id, "tunnel-api");

        // Other subdomains still use wildcard
        let other_key = RouteKey::HttpHost("web.example.com".to_string());
        let found_other = registry.lookup(&other_key).unwrap();
        assert_eq!(found_other.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_wildcard_no_match_base_domain() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        // Base domain should NOT match wildcard
        let key = RouteKey::HttpHost("example.com".to_string());
        let result = registry.lookup(&key);
        assert!(result.is_err());
    }

    #[test]
    fn test_wildcard_no_match_deep_subdomain() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        // Deep subdomains should NOT match single-level wildcard
        let key = RouteKey::HttpHost("sub.api.example.com".to_string());
        let result = registry.lookup(&key);
        assert!(result.is_err());
    }

    #[test]
    fn test_wildcard_sni_fallback() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        // SNI should also use wildcard fallback
        let key = RouteKey::TlsSni("db.example.com".to_string());
        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_exists_with_wildcard() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();

        // Check exists_with_wildcard
        let key = RouteKey::HttpHost("api.example.com".to_string());
        assert!(registry.exists_with_wildcard(&key));

        // Exact exists should be false (no exact route)
        assert!(!registry.exists(&key));
    }

    #[test]
    fn test_unregister_wildcard() {
        let registry = RouteRegistry::new();
        let target = RouteTarget {
            localup_id: "tunnel-wildcard".to_string(),
            target_addr: "tunnel:tunnel-wildcard".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register_wildcard("*.example.com", target).unwrap();
        assert_eq!(registry.wildcard_count(), 1);

        registry.unregister_wildcard("*.example.com").unwrap();
        assert_eq!(registry.wildcard_count(), 0);

        // Should no longer find via wildcard
        let key = RouteKey::HttpHost("api.example.com".to_string());
        assert!(registry.lookup(&key).is_err());
    }

    #[test]
    fn test_all_wildcard_routes() {
        let registry = RouteRegistry::new();

        let target1 = RouteTarget {
            localup_id: "tunnel-1".to_string(),
            target_addr: "tunnel:tunnel-1".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };
        let target2 = RouteTarget {
            localup_id: "tunnel-2".to_string(),
            target_addr: "tunnel:tunnel-2".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry
            .register_wildcard("*.example.com", target1)
            .unwrap();
        registry.register_wildcard("*.other.com", target2).unwrap();

        let all = registry.all_wildcard_routes();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_clear_includes_wildcards() {
        let registry = RouteRegistry::new();

        let target = RouteTarget {
            localup_id: "tunnel-1".to_string(),
            target_addr: "tunnel:tunnel-1".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry
            .register(RouteKey::TcpPort(8080), target.clone())
            .unwrap();
        registry.register_wildcard("*.example.com", target).unwrap();

        assert_eq!(registry.total_count(), 2);

        registry.clear();

        assert_eq!(registry.count(), 0);
        assert_eq!(registry.wildcard_count(), 0);
        assert_eq!(registry.total_count(), 0);
    }
}
