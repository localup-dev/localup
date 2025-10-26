//! HTTP host-based routing

use crate::{RouteKey, RouteRegistry, RouteTarget};
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
}

/// HTTP route information
#[derive(Debug, Clone)]
pub struct HttpRoute {
    pub host: String,
    pub tunnel_id: String,
    pub target_addr: String,
}

/// HTTP router
pub struct HttpRouter {
    registry: Arc<RouteRegistry>,
}

impl HttpRouter {
    pub fn new(registry: Arc<RouteRegistry>) -> Self {
        Self { registry }
    }

    /// Register an HTTP route
    pub fn register_route(&self, route: HttpRoute) -> Result<(), HttpRouterError> {
        debug!(
            "Registering HTTP route: {} -> {}",
            route.host, route.target_addr
        );

        let key = RouteKey::HttpHost(route.host.clone());
        let target = RouteTarget {
            tunnel_id: route.tunnel_id,
            target_addr: route.target_addr,
            metadata: None,
        };

        self.registry.register(key, target)?;
        Ok(())
    }

    /// Lookup route by host header
    pub fn lookup(&self, host: &str) -> Result<RouteTarget, HttpRouterError> {
        trace!("Looking up HTTP route for host: {}", host);

        // Normalize host (remove port if present)
        let normalized_host = Self::normalize_host(host);

        let key = RouteKey::HttpHost(normalized_host.to_string());
        let target = self.registry.lookup(&key)?;

        Ok(target)
    }

    /// Unregister an HTTP route
    pub fn unregister(&self, host: &str) -> Result<(), HttpRouterError> {
        debug!("Unregistering HTTP route for host: {}", host);

        let normalized_host = Self::normalize_host(host);
        let key = RouteKey::HttpHost(normalized_host.to_string());
        self.registry.unregister(&key)?;

        Ok(())
    }

    /// Check if host has a route
    pub fn has_route(&self, host: &str) -> bool {
        let normalized_host = Self::normalize_host(host);
        let key = RouteKey::HttpHost(normalized_host.to_string());
        self.registry.exists(&key)
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
            tunnel_id: "tunnel-web".to_string(),
            target_addr: "localhost:3000".to_string(),
        };

        router.register_route(route).unwrap();

        assert!(router.has_route("example.com"));

        let target = router.lookup("example.com").unwrap();
        assert_eq!(target.tunnel_id, "tunnel-web");

        router.unregister("example.com").unwrap();
        assert!(!router.has_route("example.com"));
    }

    #[test]
    fn test_http_router_with_port() {
        let registry = Arc::new(RouteRegistry::new());
        let router = HttpRouter::new(registry);

        let route = HttpRoute {
            host: "example.com".to_string(),
            tunnel_id: "tunnel-web".to_string(),
            target_addr: "localhost:3000".to_string(),
        };

        router.register_route(route).unwrap();

        // Should match even with port in host header
        let target = router.lookup("example.com:8080").unwrap();
        assert_eq!(target.tunnel_id, "tunnel-web");
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
}
