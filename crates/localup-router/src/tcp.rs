//! TCP port-based routing

use crate::{RouteKey, RouteRegistry, RouteTarget};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, trace};

/// TCP routing errors
#[derive(Debug, Error)]
pub enum TcpRouterError {
    #[error("Route error: {0}")]
    RouteError(#[from] crate::registry::RouteError),

    #[error("Invalid port: {0}")]
    InvalidPort(u16),
}

/// TCP route information
#[derive(Debug, Clone)]
pub struct TcpRoute {
    pub port: u16,
    pub localup_id: String,
    pub target_addr: String,
}

/// TCP router
pub struct TcpRouter {
    registry: Arc<RouteRegistry>,
}

impl TcpRouter {
    pub fn new(registry: Arc<RouteRegistry>) -> Self {
        Self { registry }
    }

    /// Register a TCP route
    pub fn register_route(&self, route: TcpRoute) -> Result<(), TcpRouterError> {
        debug!(
            "Registering TCP route: port {} -> {}",
            route.port, route.target_addr
        );

        let key = RouteKey::TcpPort(route.port);
        let target = RouteTarget {
            localup_id: route.localup_id,
            target_addr: route.target_addr,
            metadata: None,
        };

        self.registry.register(key, target)?;
        Ok(())
    }

    /// Lookup route by port
    pub fn lookup(&self, port: u16) -> Result<RouteTarget, TcpRouterError> {
        trace!("Looking up TCP route for port {}", port);

        let key = RouteKey::TcpPort(port);
        let target = self.registry.lookup(&key)?;

        Ok(target)
    }

    /// Unregister a TCP route
    pub fn unregister(&self, port: u16) -> Result<(), TcpRouterError> {
        debug!("Unregistering TCP route for port {}", port);

        let key = RouteKey::TcpPort(port);
        self.registry.unregister(&key)?;

        Ok(())
    }

    /// Check if port has a route
    pub fn has_route(&self, port: u16) -> bool {
        let key = RouteKey::TcpPort(port);
        self.registry.exists(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_router() {
        let registry = Arc::new(RouteRegistry::new());
        let router = TcpRouter::new(registry);

        let route = TcpRoute {
            port: 5432,
            localup_id: "localup-postgres".to_string(),
            target_addr: "localhost:5432".to_string(),
        };

        router.register_route(route).unwrap();

        assert!(router.has_route(5432));

        let target = router.lookup(5432).unwrap();
        assert_eq!(target.localup_id, "localup-postgres");

        router.unregister(5432).unwrap();
        assert!(!router.has_route(5432));
    }

    #[test]
    fn test_tcp_router_not_found() {
        let registry = Arc::new(RouteRegistry::new());
        let router = TcpRouter::new(registry);

        let result = router.lookup(9999);
        assert!(result.is_err());
    }
}
