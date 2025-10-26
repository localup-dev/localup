//! Route registry for managing tunnel routes with reconnection support

use crate::RouteKey;
use dashmap::DashMap;
use std::sync::Arc;
use thiserror::Error;

/// Route target information
#[derive(Debug, Clone)]
pub struct RouteTarget {
    /// Tunnel ID
    pub tunnel_id: String,
    /// Target address (e.g., "localhost:3000")
    pub target_addr: String,
    /// Additional metadata
    pub metadata: Option<String>,
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
}

/// Route registry for managing tunnel routes
pub struct RouteRegistry {
    routes: Arc<DashMap<RouteKey, RouteTarget>>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
        }
    }

    /// Register a route
    pub fn register(&self, key: RouteKey, target: RouteTarget) -> Result<(), RouteError> {
        if self.routes.contains_key(&key) {
            return Err(RouteError::RouteAlreadyExists(key));
        }

        self.routes.insert(key, target);
        Ok(())
    }

    /// Lookup a route
    pub fn lookup(&self, key: &RouteKey) -> Result<RouteTarget, RouteError> {
        self.routes
            .get(key)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| RouteError::RouteNotFound(key.clone()))
    }

    /// Unregister a route
    pub fn unregister(&self, key: &RouteKey) -> Result<RouteTarget, RouteError> {
        self.routes
            .remove(key)
            .map(|(_, target)| target)
            .ok_or_else(|| RouteError::RouteNotFound(key.clone()))
    }

    /// Check if a route exists
    pub fn exists(&self, key: &RouteKey) -> bool {
        self.routes.contains_key(key)
    }

    /// Get all routes
    pub fn all_routes(&self) -> Vec<(RouteKey, RouteTarget)> {
        self.routes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Get number of registered routes
    pub fn count(&self) -> usize {
        self.routes.len()
    }

    /// Clear all routes
    pub fn clear(&self) {
        self.routes.clear();
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
            tunnel_id: "tunnel-1".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
        };

        registry.register(key.clone(), target.clone()).unwrap();

        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.tunnel_id, "tunnel-1");
        assert_eq!(found.target_addr, "localhost:5432");
    }

    #[test]
    fn test_registry_duplicate() {
        let registry = RouteRegistry::new();
        let key = RouteKey::HttpHost("example.com".to_string());
        let target = RouteTarget {
            tunnel_id: "tunnel-1".to_string(),
            target_addr: "localhost:3000".to_string(),
            metadata: None,
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
            tunnel_id: "tunnel-1".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
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
}
