//! Control plane registry
use std::sync::Arc;
use tunnel_router::RouteRegistry;

pub struct ControlPlane {
    registry: Arc<RouteRegistry>,
}

impl ControlPlane {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RouteRegistry::new()),
        }
    }

    pub fn registry(&self) -> Arc<RouteRegistry> {
        self.registry.clone()
    }
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_plane() {
        let cp = ControlPlane::new();
        assert_eq!(cp.registry().count(), 0);
    }
}
