//! HTTP host-based routing
//!
//! Supports both exact hostname matching and wildcard patterns (e.g., `*.example.com`).
//! Wildcard routes are used as fallback when no exact match exists.

use crate::wildcard::WildcardPattern;
use crate::{RouteKey, RouteRegistry, RouteTarget};
use localup_proto::IpFilter;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, trace};

/// HTTP routing errors
#[derive(Debug, Error)]
pub enum HttpRouterError {
    #[error("Route error: {0}")]
    RouteError(#[from] crate::registry::RouteError),

    #[error("Invalid host header: {0}")]
    InvalidHost(String),

    #[error("Host header not found")]
    HostHeaderNotFound,

    #[error("Invalid wildcard pattern: {0}")]
    InvalidWildcardPattern(String),
}

/// HTTP route information
#[derive(Debug, Clone)]
pub struct HttpRoute {
    pub host: String,
    pub localup_id: String,
    pub target_addr: String,
    /// IP filter for access control (empty allows all)
    pub ip_filter: IpFilter,
}

/// HTTP router
pub struct HttpRouter {
    registry: Arc<RouteRegistry>,
}

impl HttpRouter {
    pub fn new(registry: Arc<RouteRegistry>) -> Self {
        Self { registry }
    }

    /// Register an HTTP route (exact match)
    pub fn register_route(&self, route: HttpRoute) -> Result<(), HttpRouterError> {
        debug!(
            "Registering HTTP route: {} -> {}",
            route.host, route.target_addr
        );

        let key = RouteKey::HttpHost(route.host.clone());
        let target = RouteTarget {
            localup_id: route.localup_id,
            target_addr: route.target_addr,
            metadata: None,
            ip_filter: route.ip_filter,
        };

        self.registry.register(key, target)?;
        Ok(())
    }

    /// Register a wildcard HTTP route (e.g., *.example.com)
    ///
    /// Wildcard routes are used as fallback when no exact match exists.
    /// Only `*.domain.tld` format is supported.
    ///
    /// # Example
    /// ```
    /// use localup_router::{HttpRouter, HttpRoute, RouteRegistry};
    /// use std::sync::Arc;
    ///
    /// let registry = Arc::new(RouteRegistry::new());
    /// let router = HttpRouter::new(registry);
    ///
    /// // Register wildcard route
    /// router.register_wildcard_route("*.example.com", "tunnel-1", "tunnel:tunnel-1").unwrap();
    ///
    /// // api.example.com will match the wildcard
    /// let target = router.lookup("api.example.com").unwrap();
    /// assert_eq!(target.localup_id, "tunnel-1");
    /// ```
    pub fn register_wildcard_route(
        &self,
        pattern: &str,
        localup_id: &str,
        target_addr: &str,
    ) -> Result<(), HttpRouterError> {
        // Validate pattern first
        WildcardPattern::parse(pattern)
            .map_err(|e| HttpRouterError::InvalidWildcardPattern(e.to_string()))?;

        debug!(
            "Registering wildcard HTTP route: {} -> {}",
            pattern, target_addr
        );

        let target = RouteTarget {
            localup_id: localup_id.to_string(),
            target_addr: target_addr.to_string(),
            metadata: Some("wildcard".to_string()),
            ip_filter: IpFilter::new(),
        };

        self.registry.register_wildcard(pattern, target)?;
        Ok(())
    }

    /// Lookup route by host header (with wildcard fallback)
    ///
    /// The lookup follows this priority:
    /// 1. Exact match for the hostname
    /// 2. Wildcard match (e.g., `api.example.com` matches `*.example.com`)
    /// 3. Not found
    pub fn lookup(&self, host: &str) -> Result<RouteTarget, HttpRouterError> {
        trace!("Looking up HTTP route for host: {}", host);

        // Normalize host (remove port if present)
        let normalized_host = Self::normalize_host(host);

        let key = RouteKey::HttpHost(normalized_host.to_string());
        // Registry's lookup already handles wildcard fallback
        let target = self.registry.lookup(&key)?;

        Ok(target)
    }

    /// Unregister an HTTP route (exact match)
    pub fn unregister(&self, host: &str) -> Result<(), HttpRouterError> {
        debug!("Unregistering HTTP route for host: {}", host);

        let normalized_host = Self::normalize_host(host);
        let key = RouteKey::HttpHost(normalized_host.to_string());
        self.registry.unregister(&key)?;

        Ok(())
    }

    /// Unregister a wildcard HTTP route
    pub fn unregister_wildcard(&self, pattern: &str) -> Result<(), HttpRouterError> {
        debug!("Unregistering wildcard HTTP route: {}", pattern);
        self.registry.unregister_wildcard(pattern)?;
        Ok(())
    }

    /// Check if host has an exact route (does not check wildcard)
    pub fn has_route(&self, host: &str) -> bool {
        let normalized_host = Self::normalize_host(host);
        let key = RouteKey::HttpHost(normalized_host.to_string());
        self.registry.exists(&key)
    }

    /// Check if host has a route (including wildcard fallback)
    pub fn has_route_with_wildcard(&self, host: &str) -> bool {
        let normalized_host = Self::normalize_host(host);
        let key = RouteKey::HttpHost(normalized_host.to_string());
        self.registry.exists_with_wildcard(&key)
    }

    /// Check if a wildcard pattern is registered
    pub fn has_wildcard_route(&self, pattern: &str) -> bool {
        self.registry.wildcard_exists(pattern)
    }

    /// Normalize host header (remove port if present)
    fn normalize_host(host: &str) -> &str {
        // Remove port if present (e.g., "example.com:8080" -> "example.com")
        host.split(':').next().unwrap_or(host)
    }

    /// Extract host from HTTP headers
    pub fn extract_host(headers: &[(String, String)]) -> Result<String, HttpRouterError> {
        headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("host"))
            .map(|(_, value)| value.clone())
            .ok_or(HttpRouterError::HostHeaderNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_router() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        let route = HttpRoute {
            host: "example.com".to_string(),
            localup_id: "localup-web".to_string(),
            target_addr: "localhost:3000".to_string(),
            ip_filter: IpFilter::new(),
        };

        router.register_route(route).unwrap();

        assert!(router.has_route("example.com"));

        let target = router.lookup("example.com").unwrap();
        assert_eq!(target.localup_id, "localup-web");

        router.unregister("example.com").unwrap();
        assert!(!router.has_route("example.com"));
    }

    #[test]
    fn test_http_router_with_port() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        let route = HttpRoute {
            host: "example.com".to_string(),
            localup_id: "localup-web".to_string(),
            target_addr: "localhost:3000".to_string(),
            ip_filter: IpFilter::new(),
        };

        router.register_route(route).unwrap();

        // Should match even with port in host header
        let target = router.lookup("example.com:8080").unwrap();
        assert_eq!(target.localup_id, "localup-web");
    }

    #[test]
    fn test_http_router_not_found() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        let result = router.lookup("unknown.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_host() {
        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Host".to_string(), "example.com".to_string()),
            ("User-Agent".to_string(), "test".to_string()),
        ];

        let host = HttpRouter::extract_host(&headers).unwrap();
        assert_eq!(host, "example.com");
    }

    #[test]
    fn test_extract_host_case_insensitive() {
        let headers = vec![("host".to_string(), "example.com".to_string())];

        let host = HttpRouter::extract_host(&headers).unwrap();
        assert_eq!(host, "example.com");
    }

    #[test]
    fn test_extract_host_not_found() {
        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];

        let result = HttpRouter::extract_host(&headers);
        assert!(result.is_err());
    }

    // Wildcard tests

    #[test]
    fn test_http_router_wildcard_registration() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        assert!(router.has_wildcard_route("*.example.com"));
    }

    #[test]
    fn test_http_router_wildcard_lookup() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        // Should find via wildcard
        let target = router.lookup("api.example.com").unwrap();
        assert_eq!(target.localup_id, "tunnel-wildcard");

        let target2 = router.lookup("web.example.com").unwrap();
        assert_eq!(target2.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_http_router_exact_beats_wildcard() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        // Register wildcard first
        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        // Register exact route
        let exact_route = HttpRoute {
            host: "api.example.com".to_string(),
            localup_id: "tunnel-api".to_string(),
            target_addr: "tunnel:tunnel-api".to_string(),
            ip_filter: IpFilter::new(),
        };
        router.register_route(exact_route).unwrap();

        // Exact should win
        let target = router.lookup("api.example.com").unwrap();
        assert_eq!(target.localup_id, "tunnel-api");

        // Others use wildcard
        let target2 = router.lookup("web.example.com").unwrap();
        assert_eq!(target2.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_http_router_wildcard_with_port() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        // Should work with port in host header
        let target = router.lookup("api.example.com:8080").unwrap();
        assert_eq!(target.localup_id, "tunnel-wildcard");
    }

    #[test]
    fn test_http_router_invalid_wildcard() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        // Double asterisk should fail
        let result =
            router.register_wildcard_route("**.example.com", "tunnel-1", "tunnel:tunnel-1");
        assert!(result.is_err());

        // Mid-level wildcard should fail
        let result =
            router.register_wildcard_route("api.*.example.com", "tunnel-1", "tunnel:tunnel-1");
        assert!(result.is_err());
    }

    #[test]
    fn test_http_router_has_route_with_wildcard() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        // has_route (exact only) should be false
        assert!(!router.has_route("api.example.com"));

        // has_route_with_wildcard should be true
        assert!(router.has_route_with_wildcard("api.example.com"));
    }

    #[test]
    fn test_http_router_unregister_wildcard() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        router
            .register_wildcard_route("*.example.com", "tunnel-wildcard", "tunnel:tunnel-wildcard")
            .unwrap();

        assert!(router.has_wildcard_route("*.example.com"));

        router.unregister_wildcard("*.example.com").unwrap();

        assert!(!router.has_wildcard_route("*.example.com"));

        // Should no longer find via wildcard
        let result = router.lookup("api.example.com");
        assert!(result.is_err());
    }
}
