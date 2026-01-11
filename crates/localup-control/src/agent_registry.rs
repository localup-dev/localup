//! Agent registry for tracking connected agents in reverse tunnel routing
//!
//! This module manages agents that connect to the relay to provide access
//! to specific target addresses. Each agent declares a single target address
//! it forwards to, and the registry routes reverse tunnel requests to the appropriate agent.

use localup_proto::AgentMetadata;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A registered agent with its connection metadata
#[derive(Debug, Clone)]
pub struct RegisteredAgent {
    /// Unique identifier for this agent
    pub agent_id: String,
    /// Specific target address this agent forwards to (e.g., "192.168.1.100:8080")
    pub target_address: String,
    /// Agent metadata (hostname, platform, version, etc.)
    pub metadata: AgentMetadata,
    /// Timestamp when this agent connected
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Registry for managing connected agents
///
/// The registry tracks which agents are connected and their capabilities.
/// It provides methods to register/unregister agents and find agents
/// capable of reaching specific networks.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, RegisteredAgent>>>,
}

impl AgentRegistry {
    /// Create a new empty agent registry
    pub fn new() -> Self {
        tracing::info!("Creating new agent registry");
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new agent or re-register an existing one
    ///
    /// If an agent with the same ID is already registered, this will replace it.
    /// This is useful for handling agent reconnections after temporary network issues.
    ///
    /// # Returns
    ///
    /// Ok if registration was successful. The return value is None if this was a new registration,
    /// or Some(old_agent) if an existing agent was replaced.
    pub fn register_or_replace(
        &self,
        agent: RegisteredAgent,
    ) -> Result<Option<RegisteredAgent>, String> {
        let mut agents = self.agents.write().unwrap();

        let old_agent = agents.insert(agent.agent_id.clone(), agent.clone());

        if let Some(ref replaced) = old_agent {
            tracing::info!(
                agent_id = %agent.agent_id,
                hostname = %agent.metadata.hostname,
                target_address = %agent.target_address,
                old_connected_at = %replaced.connected_at,
                "Re-registered existing agent (replaced stale connection)"
            );
        } else {
            tracing::info!(
                agent_id = %agent.agent_id,
                hostname = %agent.metadata.hostname,
                target_address = %agent.target_address,
                "Registered new agent"
            );
        }

        Ok(old_agent)
    }

    /// Register a new agent (fails if already registered)
    ///
    /// # Errors
    ///
    /// Returns an error if an agent with the same ID is already registered.
    /// Use `register_or_replace` if you want to allow reconnections.
    pub fn register(&self, agent: RegisteredAgent) -> Result<(), String> {
        let mut agents = self.agents.write().unwrap();

        if agents.contains_key(&agent.agent_id) {
            let error = format!("Agent {} is already registered", agent.agent_id);
            tracing::warn!("{}", error);
            return Err(error);
        }

        tracing::info!(
            agent_id = %agent.agent_id,
            hostname = %agent.metadata.hostname,
            target_address = %agent.target_address,
            "Registered new agent"
        );

        agents.insert(agent.agent_id.clone(), agent);
        Ok(())
    }

    /// Unregister an agent by ID
    ///
    /// Returns the agent if it was registered, or None if not found.
    pub fn unregister(&self, agent_id: &str) -> Option<RegisteredAgent> {
        let mut agents = self.agents.write().unwrap();
        let agent = agents.remove(agent_id);

        if agent.is_some() {
            tracing::info!(agent_id = %agent_id, "Unregistered agent");
        } else {
            tracing::warn!(agent_id = %agent_id, "Attempted to unregister unknown agent");
        }

        agent
    }

    /// Get information about a specific agent
    pub fn get(&self, agent_id: &str) -> Option<RegisteredAgent> {
        let agents = self.agents.read().unwrap();
        agents.get(agent_id).cloned()
    }

    /// List all registered agents
    pub fn list(&self) -> Vec<RegisteredAgent> {
        let agents = self.agents.read().unwrap();
        agents.values().cloned().collect()
    }

    /// Find an agent that forwards to a specific target address
    ///
    /// This searches through all registered agents and finds the one
    /// whose target_address exactly matches the requested address.
    ///
    /// # Arguments
    ///
    /// * `target_address` - Target address in "host:port" format (e.g., "192.168.1.100:8080")
    ///
    /// # Returns
    ///
    /// The agent that forwards to this exact address, or None if no agent matches.
    pub fn find_by_address(&self, target_address: &str) -> Option<RegisteredAgent> {
        let agents = self.agents.read().unwrap();

        // Search for an agent with exact address match
        for agent in agents.values() {
            if agent.target_address == target_address {
                tracing::debug!(
                    agent_id = %agent.agent_id,
                    target_address = %target_address,
                    "Found agent for target address"
                );
                return Some(agent.clone());
            }
        }

        tracing::warn!(
            target_address = %target_address,
            "No agent found for target address"
        );
        None
    }

    /// Get the total count of registered agents
    pub fn count(&self) -> usize {
        let agents = self.agents.read().unwrap();
        agents.len()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_agent(id: &str, target_address: &str) -> RegisteredAgent {
        RegisteredAgent {
            agent_id: id.to_string(),
            target_address: target_address.to_string(),
            metadata: AgentMetadata {
                hostname: format!("host-{}", id),
                platform: "linux".to_string(),
                version: "1.0.0".to_string(),
                location: Some("us-east".to_string()),
            },
            connected_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_register_agent() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        let result = registry.register(agent.clone());
        assert!(result.is_ok());

        let retrieved = registry.get("agent1");
        assert!(retrieved.is_some());
        let retrieved_agent = retrieved.unwrap();
        assert_eq!(retrieved_agent.agent_id, "agent1");
        assert_eq!(retrieved_agent.target_address, "192.168.1.100:8080");
    }

    #[test]
    fn test_register_duplicate_agent() {
        let registry = AgentRegistry::new();
        let agent1 = create_test_agent("agent1", "192.168.1.100:8080");
        let agent2 = create_test_agent("agent1", "10.0.0.5:3000");

        registry.register(agent1).unwrap();
        let result = registry.register(agent2);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_unregister_agent() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        registry.register(agent).unwrap();
        assert_eq!(registry.count(), 1);

        let removed = registry.unregister("agent1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().agent_id, "agent1");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent_agent() {
        let registry = AgentRegistry::new();
        let removed = registry.unregister("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_list_agents() {
        let registry = AgentRegistry::new();
        let agent1 = create_test_agent("agent1", "192.168.1.100:8080");
        let agent2 = create_test_agent("agent2", "10.0.0.5:3000");

        registry.register(agent1).unwrap();
        registry.register(agent2).unwrap();

        let agents = registry.list();
        assert_eq!(agents.len(), 2);

        let ids: Vec<String> = agents.iter().map(|a| a.agent_id.clone()).collect();
        assert!(ids.contains(&"agent1".to_string()));
        assert!(ids.contains(&"agent2".to_string()));
    }

    #[test]
    fn test_find_by_address_match() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        registry.register(agent).unwrap();

        // Should find agent for exact address match
        let found = registry.find_by_address("192.168.1.100:8080");
        assert!(found.is_some());
        assert_eq!(found.unwrap().agent_id, "agent1");
    }

    #[test]
    fn test_find_by_address_no_match() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        registry.register(agent).unwrap();

        // Should NOT find agent for different address
        let found = registry.find_by_address("10.0.0.1:8080");
        assert!(found.is_none());
    }

    #[test]
    fn test_find_by_address_multiple_agents() {
        let registry = AgentRegistry::new();
        let agent1 = create_test_agent("agent1", "192.168.1.100:8080");
        let agent2 = create_test_agent("agent2", "10.0.0.5:3000");

        registry.register(agent1).unwrap();
        registry.register(agent2).unwrap();

        // Find first agent
        let found = registry.find_by_address("192.168.1.100:8080");
        assert!(found.is_some());
        assert_eq!(found.unwrap().agent_id, "agent1");

        // Find second agent
        let found = registry.find_by_address("10.0.0.5:3000");
        assert!(found.is_some());
        assert_eq!(found.unwrap().agent_id, "agent2");
    }

    #[test]
    fn test_find_by_address_no_match_similar() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        registry.register(agent).unwrap();

        // Should NOT match similar but different addresses
        assert!(registry.find_by_address("192.168.1.100:8081").is_none());
        assert!(registry.find_by_address("192.168.1.101:8080").is_none());
        assert!(registry.find_by_address("localhost:8080").is_none());
    }

    #[test]
    fn test_count() {
        let registry = AgentRegistry::new();
        assert_eq!(registry.count(), 0);

        let agent1 = create_test_agent("agent1", "192.168.1.100:8080");
        registry.register(agent1).unwrap();
        assert_eq!(registry.count(), 1);

        let agent2 = create_test_agent("agent2", "10.0.0.5:3000");
        registry.register(agent2).unwrap();
        assert_eq!(registry.count(), 2);

        registry.unregister("agent1");
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_agent_metadata() {
        let registry = AgentRegistry::new();
        let mut agent = create_test_agent("agent1", "192.168.1.100:8080");
        agent.metadata.hostname = "test-host".to_string();
        agent.metadata.platform = "macos".to_string();
        agent.metadata.version = "2.0.0".to_string();

        registry.register(agent).unwrap();

        let retrieved = registry.get("agent1").unwrap();
        assert_eq!(retrieved.metadata.hostname, "test-host");
        assert_eq!(retrieved.metadata.platform, "macos");
        assert_eq!(retrieved.metadata.version, "2.0.0");
    }

    #[test]
    fn test_register_or_replace_new_agent() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        let result = registry.register_or_replace(agent.clone());
        assert!(result.is_ok());

        let old_agent = result.unwrap();
        assert!(
            old_agent.is_none(),
            "First registration should not replace anything"
        );

        let retrieved = registry.get("agent1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().agent_id, "agent1");
    }

    #[test]
    fn test_register_or_replace_existing_agent() {
        let registry = AgentRegistry::new();
        let agent1 = create_test_agent("agent1", "192.168.1.100:8080");
        let agent2 = create_test_agent("agent1", "10.0.0.5:3000");

        // Register first agent
        registry.register(agent1.clone()).unwrap();
        assert_eq!(registry.count(), 1);

        // Re-register with new target
        let result = registry.register_or_replace(agent2.clone());
        assert!(result.is_ok());

        let old_agent = result.unwrap();
        assert!(old_agent.is_some(), "Should replace existing agent");
        assert_eq!(old_agent.unwrap().target_address, "192.168.1.100:8080");

        // Count should still be 1 (replaced, not added)
        assert_eq!(registry.count(), 1);

        // New agent should be registered with new target
        let retrieved = registry.get("agent1").unwrap();
        assert_eq!(retrieved.target_address, "10.0.0.5:3000");
    }

    #[test]
    fn test_register_or_replace_allows_reconnections() {
        let registry = AgentRegistry::new();
        let agent = create_test_agent("agent1", "192.168.1.100:8080");

        // Simulate multiple reconnections
        for i in 0..5 {
            let result = registry.register_or_replace(agent.clone());
            assert!(result.is_ok(), "Registration {} should succeed", i);

            let count = registry.count();
            assert_eq!(
                count, 1,
                "Should have exactly 1 agent after reconnection {}",
                i
            );
        }
    }
}
