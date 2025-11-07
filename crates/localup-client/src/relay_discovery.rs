//! Relay server discovery and selection
//!
//! This module handles discovering available relay servers and selecting
//! the best one based on region, protocol, and availability.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Relay configuration embedded at compile time
/// The path is determined by the LOCALUP_RELAYS_CONFIG environment variable at build time,
/// or defaults to workspace root relays.yaml
const RELAY_CONFIG: &str = include_str!(env!("RELAY_CONFIG_PATH"));

#[derive(Debug, Error)]
pub enum RelayError {
    #[error("Failed to parse relay configuration: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("No relays available for region: {0}")]
    NoRelaysAvailable(String),

    #[error("No relay found matching criteria")]
    NoMatchingRelay,

    #[error("Invalid protocol: {0}")]
    InvalidProtocol(String),
}

/// Root relay configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayConfig {
    pub version: u32,
    pub config: GlobalConfig,
    pub relays: Vec<RelayInfo>,
    pub region_groups: Vec<RegionGroup>,
    pub selection_policies: HashMap<String, SelectionPolicy>,
}

/// Global configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalConfig {
    pub default_protocol: String,
    pub connection_timeout: u64,
    pub health_check_interval: u64,
}

/// Relay server definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayInfo {
    pub id: String,
    pub name: String,
    pub region: String,
    pub location: Location,
    pub endpoints: Vec<RelayEndpoint>,
    pub status: String,
    pub tags: Vec<String>,
}

/// Geographic location
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Location {
    pub city: String,
    pub state: String,
    pub country: String,
    pub continent: String,
}

/// Relay endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayEndpoint {
    pub protocol: String,
    pub address: String,
    pub capacity: u32,
    pub priority: u32,
}

/// Region group for fallback
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegionGroup {
    pub name: String,
    pub regions: Vec<String>,
    pub fallback_order: Vec<String>,
}

/// Relay selection policy
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SelectionPolicy {
    pub prefer_same_region: bool,
    pub fallback_to_nearest: bool,
    pub consider_capacity: bool,
    pub only_active: bool,
    #[serde(default)]
    pub include_tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
}

/// Relay discovery and selection
pub struct RelayDiscovery {
    config: RelayConfig,
}

impl RelayDiscovery {
    /// Create a new relay discovery instance
    pub fn new() -> Result<Self, RelayError> {
        let config: RelayConfig = serde_yaml::from_str(RELAY_CONFIG)?;
        Ok(Self { config })
    }

    /// Get all available relays
    pub fn all_relays(&self) -> &[RelayInfo] {
        &self.config.relays
    }

    /// Get relays by region
    pub fn relays_by_region(&self, region: &str) -> Vec<&RelayInfo> {
        self.config
            .relays
            .iter()
            .filter(|r| r.region == region && r.status == "active")
            .collect()
    }

    /// Get relays by tag
    pub fn relays_by_tag(&self, tag: &str) -> Vec<&RelayInfo> {
        self.config
            .relays
            .iter()
            .filter(|r| r.tags.contains(&tag.to_string()) && r.status == "active")
            .collect()
    }

    /// Select best relay automatically
    pub fn select_relay(
        &self,
        protocol: &str,
        preferred_region: Option<&str>,
        policy_name: Option<&str>,
    ) -> Result<String, RelayError> {
        let policy = self
            .config
            .selection_policies
            .get(policy_name.unwrap_or("auto"))
            .ok_or(RelayError::NoMatchingRelay)?;

        // Filter relays by policy
        let mut candidates: Vec<&RelayInfo> = self
            .config
            .relays
            .iter()
            .filter(|r| {
                // Only active relays
                if policy.only_active && r.status != "active" {
                    return false;
                }

                // Check include tags
                if !policy.include_tags.is_empty()
                    && !policy.include_tags.iter().any(|tag| r.tags.contains(tag))
                {
                    return false;
                }

                // Check exclude tags
                if policy.exclude_tags.iter().any(|tag| r.tags.contains(tag)) {
                    return false;
                }

                // Must have endpoint for requested protocol
                r.endpoints.iter().any(|e| e.protocol == protocol)
            })
            .collect();

        if candidates.is_empty() {
            return Err(RelayError::NoMatchingRelay);
        }

        // Prefer same region if specified and policy allows
        if let Some(region) = preferred_region {
            if policy.prefer_same_region {
                let same_region: Vec<&RelayInfo> = candidates
                    .iter()
                    .filter(|r| r.region == region)
                    .copied()
                    .collect();

                if !same_region.is_empty() {
                    candidates = same_region;
                }
            }
        }

        // Sort by priority and capacity
        candidates.sort_by(|a, b| {
            let a_endpoint = a.endpoints.iter().find(|e| e.protocol == protocol).unwrap();
            let b_endpoint = b.endpoints.iter().find(|e| e.protocol == protocol).unwrap();

            // First by priority (lower is better)
            match a_endpoint.priority.cmp(&b_endpoint.priority) {
                std::cmp::Ordering::Equal => {
                    // Then by capacity (higher is better) if policy says so
                    if policy.consider_capacity {
                        b_endpoint.capacity.cmp(&a_endpoint.capacity)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                other => other,
            }
        });

        // Get the best relay
        let best_relay = candidates.first().ok_or(RelayError::NoMatchingRelay)?;
        let endpoint = best_relay
            .endpoints
            .iter()
            .find(|e| e.protocol == protocol)
            .ok_or_else(|| RelayError::InvalidProtocol(protocol.to_string()))?;

        Ok(endpoint.address.clone())
    }

    /// Get default protocol
    pub fn default_protocol(&self) -> &str {
        &self.config.config.default_protocol
    }

    /// List all available regions
    pub fn list_regions(&self) -> Vec<String> {
        let mut regions: Vec<String> = self
            .config
            .relays
            .iter()
            .filter(|r| r.status == "active")
            .map(|r| r.region.clone())
            .collect();
        regions.sort();
        regions.dedup();
        regions
    }

    /// Get relay by ID
    pub fn get_relay_by_id(&self, id: &str) -> Option<&RelayInfo> {
        self.config.relays.iter().find(|r| r.id == id)
    }

    /// Get fallback regions for a given region
    pub fn get_fallback_regions(&self, region: &str) -> Vec<String> {
        for group in &self.config.region_groups {
            if group.regions.contains(&region.to_string()) {
                return group.fallback_order.clone();
            }
        }
        vec![]
    }
}

impl Default for RelayDiscovery {
    fn default() -> Self {
        Self::new().expect("Failed to load embedded relay configuration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_discovery_creation() {
        let discovery = RelayDiscovery::new().unwrap();
        assert!(!discovery.all_relays().is_empty());
    }

    #[test]
    fn test_list_regions() {
        let discovery = RelayDiscovery::new().unwrap();
        let regions = discovery.list_regions();
        assert_eq!(regions.len(), 1);
        assert!(regions.contains(&"eu-west".to_string()));
    }

    #[test]
    fn test_relays_by_region() {
        let discovery = RelayDiscovery::new().unwrap();
        let eu_west_relays = discovery.relays_by_region("eu-west");
        assert!(!eu_west_relays.is_empty());
    }

    #[test]
    fn test_relays_by_tag() {
        let discovery = RelayDiscovery::new().unwrap();
        let prod_relays = discovery.relays_by_tag("production");
        assert_eq!(prod_relays.len(), 1);

        let primary_relays = discovery.relays_by_tag("primary");
        assert_eq!(primary_relays.len(), 1);
    }

    #[test]
    fn test_select_relay_auto() {
        let discovery = RelayDiscovery::new().unwrap();

        // Select HTTPS relay automatically
        let relay_addr = discovery.select_relay("https", None, None).unwrap();
        assert_eq!(relay_addr, "tunnel.kfs.es:4443");

        // Select TCP relay automatically
        let relay_addr = discovery.select_relay("tcp", None, None).unwrap();
        assert_eq!(relay_addr, "tunnel.kfs.es:5443");
    }

    #[test]
    fn test_select_relay_with_region() {
        let discovery = RelayDiscovery::new().unwrap();

        // Select relay in specific region
        let relay_addr = discovery
            .select_relay("https", Some("eu-west"), None)
            .unwrap();

        // Verify it's the eu-west relay
        assert_eq!(relay_addr, "tunnel.kfs.es:4443");
    }

    #[test]
    fn test_select_relay_by_protocol() {
        let discovery = RelayDiscovery::new().unwrap();

        // Select HTTPS relay
        let https_addr = discovery.select_relay("https", None, None).unwrap();
        assert_eq!(https_addr, "tunnel.kfs.es:4443");

        // Select TCP relay
        let tcp_addr = discovery.select_relay("tcp", None, None).unwrap();
        assert_eq!(tcp_addr, "tunnel.kfs.es:5443");
    }

    #[test]
    fn test_get_relay_by_id() {
        let discovery = RelayDiscovery::new().unwrap();

        let relay = discovery.get_relay_by_id("eu-west-1");
        assert!(relay.is_some());
        assert_eq!(relay.unwrap().id, "eu-west-1");
    }

    #[test]
    fn test_default_protocol() {
        let discovery = RelayDiscovery::new().unwrap();
        assert_eq!(discovery.default_protocol(), "https");
    }

    #[test]
    fn test_get_fallback_regions() {
        let discovery = RelayDiscovery::new().unwrap();

        let fallbacks = discovery.get_fallback_regions("eu-west");
        assert!(!fallbacks.is_empty());
        assert!(fallbacks.contains(&"eu-west".to_string()));
    }

    #[test]
    fn test_invalid_protocol() {
        let discovery = RelayDiscovery::new().unwrap();

        let result = discovery.select_relay("invalid", None, None);
        assert!(result.is_err());
    }
}
