//! TLS SNI-based routing

use crate::{RouteKey, RouteRegistry, RouteTarget};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, trace};

/// SNI routing errors
#[derive(Debug, Error)]
pub enum SniRouterError {
    #[error("Route error: {0}")]
    RouteError(#[from] crate::registry::RouteError),

    #[error("Invalid SNI hostname: {0}")]
    InvalidSni(String),

    #[error("SNI extraction failed")]
    SniExtractionFailed,
}

/// SNI route information
#[derive(Debug, Clone)]
pub struct SniRoute {
    pub sni_hostname: String,
    pub tunnel_id: String,
    pub target_addr: String,
}

/// SNI router for TLS connections
pub struct SniRouter {
    registry: Arc<RouteRegistry>,
}

impl SniRouter {
    pub fn new(registry: Arc<RouteRegistry>) -> Self {
        Self { registry }
    }

    /// Register an SNI route
    pub fn register_route(&self, route: SniRoute) -> Result<(), SniRouterError> {
        debug!(
            "Registering SNI route: {} -> {}",
            route.sni_hostname, route.target_addr
        );

        let key = RouteKey::TlsSni(route.sni_hostname.clone());
        let target = RouteTarget {
            tunnel_id: route.tunnel_id,
            target_addr: route.target_addr,
            metadata: None,
        };

        self.registry.register(key, target)?;
        Ok(())
    }

    /// Lookup route by SNI hostname
    pub fn lookup(&self, sni_hostname: &str) -> Result<RouteTarget, SniRouterError> {
        trace!("Looking up SNI route for hostname: {}", sni_hostname);

        let key = RouteKey::TlsSni(sni_hostname.to_string());
        let target = self.registry.lookup(&key)?;

        Ok(target)
    }

    /// Unregister an SNI route
    pub fn unregister(&self, sni_hostname: &str) -> Result<(), SniRouterError> {
        debug!("Unregistering SNI route for hostname: {}", sni_hostname);

        let key = RouteKey::TlsSni(sni_hostname.to_string());
        self.registry.unregister(&key)?;

        Ok(())
    }

    /// Check if SNI has a route
    pub fn has_route(&self, sni_hostname: &str) -> bool {
        let key = RouteKey::TlsSni(sni_hostname.to_string());
        self.registry.exists(&key)
    }

    /// Extract SNI from TLS ClientHello
    /// This is a simplified implementation - real SNI extraction would parse TLS handshake
    pub fn extract_sni(_client_hello: &[u8]) -> Result<String, SniRouterError> {
        // This is a placeholder for SNI extraction logic
        // In a real implementation, you'd parse the TLS ClientHello message
        // to extract the Server Name Indication extension

        // For now, we'll just return an error
        // The actual implementation would use a TLS parsing library
        Err(SniRouterError::SniExtractionFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sni_router() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let route = SniRoute {
            sni_hostname: "db.example.com".to_string(),
            tunnel_id: "tunnel-db".to_string(),
            target_addr: "localhost:5432".to_string(),
        };

        router.register_route(route).unwrap();

        assert!(router.has_route("db.example.com"));

        let target = router.lookup("db.example.com").unwrap();
        assert_eq!(target.tunnel_id, "tunnel-db");

        router.unregister("db.example.com").unwrap();
        assert!(!router.has_route("db.example.com"));
    }

    #[test]
    fn test_sni_router_not_found() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let result = router.lookup("unknown.example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_wildcard_sni() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let route = SniRoute {
            sni_hostname: "*.example.com".to_string(),
            tunnel_id: "tunnel-wildcard".to_string(),
            target_addr: "localhost:3000".to_string(),
        };

        router.register_route(route).unwrap();

        // Exact match works
        assert!(router.has_route("*.example.com"));

        // Note: Wildcard matching would require additional logic
        // This test just verifies exact registration/lookup works
    }
}
