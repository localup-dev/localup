//! Tunnel configuration storage
//!
//! Manages tunnel configurations as JSON files in ~/.localup/tunnels/

use anyhow::{Context, Result};
use localup_client::TunnelConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Stored tunnel configuration with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTunnel {
    /// Tunnel name (used as filename)
    pub name: String,
    /// Whether this tunnel should auto-start with daemon
    pub enabled: bool,
    /// Tunnel configuration
    pub config: TunnelConfig,
}

/// Tunnel configuration manager
pub struct TunnelStore {
    base_dir: PathBuf,
}

impl TunnelStore {
    /// Create a new tunnel store
    pub fn new() -> Result<Self> {
        let base_dir = Self::get_base_dir()?;
        fs::create_dir_all(&base_dir).context("Failed to create tunnel configuration directory")?;
        Ok(Self { base_dir })
    }

    /// Create a tunnel store with a custom base directory (for testing)
    #[cfg(test)]
    pub fn with_base_dir(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir).context("Failed to create tunnel configuration directory")?;
        Ok(Self { base_dir })
    }

    /// Get the base directory for tunnel configurations
    fn get_base_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".localup").join("tunnels"))
    }

    /// Get the path for a tunnel configuration file
    fn localup_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", name))
    }

    /// Validate tunnel name (alphanumeric, hyphens, underscores only)
    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            anyhow::bail!("Tunnel name cannot be empty");
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!(
                "Tunnel name must contain only alphanumeric characters, hyphens, and underscores"
            );
        }
        Ok(())
    }

    /// Save a tunnel configuration
    pub fn save(&self, tunnel: &StoredTunnel) -> Result<()> {
        Self::validate_name(&tunnel.name)?;

        let path = self.localup_path(&tunnel.name);
        let json = serde_json::to_string_pretty(tunnel)
            .context("Failed to serialize tunnel configuration")?;

        fs::write(&path, json).context(format!("Failed to write tunnel file: {:?}", path))?;

        Ok(())
    }

    /// Load a tunnel configuration by name
    pub fn load(&self, name: &str) -> Result<StoredTunnel> {
        Self::validate_name(name)?;

        let path = self.localup_path(name);
        let json =
            fs::read_to_string(&path).context(format!("Failed to read tunnel file: {:?}", path))?;

        let tunnel: StoredTunnel = serde_json::from_str(&json)
            .context(format!("Failed to parse tunnel configuration: {:?}", path))?;

        Ok(tunnel)
    }

    /// List all tunnel configurations
    pub fn list(&self) -> Result<Vec<StoredTunnel>> {
        let mut tunnels = Vec::new();

        for entry in
            fs::read_dir(&self.base_dir).context("Failed to read tunnel configuration directory")?
        {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let json = fs::read_to_string(&path)
                    .context(format!("Failed to read tunnel file: {:?}", path))?;

                let tunnel: StoredTunnel = serde_json::from_str(&json)
                    .context(format!("Failed to parse tunnel configuration: {:?}", path))?;

                tunnels.push(tunnel);
            }
        }

        // Sort by name for consistent output
        tunnels.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(tunnels)
    }

    /// List only enabled tunnels
    pub fn list_enabled(&self) -> Result<Vec<StoredTunnel>> {
        Ok(self.list()?.into_iter().filter(|t| t.enabled).collect())
    }

    /// Check if a tunnel exists
    pub fn exists(&self, name: &str) -> bool {
        Self::validate_name(name).is_ok() && self.localup_path(name).exists()
    }

    /// Remove a tunnel configuration
    pub fn remove(&self, name: &str) -> Result<()> {
        Self::validate_name(name)?;

        let path = self.localup_path(name);
        if !path.exists() {
            anyhow::bail!("Tunnel '{}' not found", name);
        }

        fs::remove_file(&path).context(format!("Failed to remove tunnel file: {:?}", path))?;

        Ok(())
    }

    /// Enable a tunnel (auto-start with daemon)
    pub fn enable(&self, name: &str) -> Result<()> {
        let mut tunnel = self.load(name)?;
        tunnel.enabled = true;
        self.save(&tunnel)
    }

    /// Disable a tunnel (don't auto-start with daemon)
    pub fn disable(&self, name: &str) -> Result<()> {
        let mut tunnel = self.load(name)?;
        tunnel.enabled = false;
        self.save(&tunnel)
    }

    /// Get the base directory path (for display purposes)
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

impl Default for TunnelStore {
    fn default() -> Self {
        Self::new().expect("Failed to create tunnel store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use localup_client::ProtocolConfig;
    use localup_proto::ExitNodeConfig;
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_store() -> (TunnelStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = TunnelStore {
            base_dir: temp_dir.path().to_path_buf(),
        };
        (store, temp_dir)
    }

    fn create_test_tunnel(name: &str) -> StoredTunnel {
        StoredTunnel {
            name: name.to_string(),
            enabled: true,
            config: TunnelConfig {
                local_host: "localhost".to_string(),
                protocols: vec![ProtocolConfig::Http {
                    local_port: 3000,
                    subdomain: Some("test".to_string()),
                }],
                auth_token: "test-token".to_string(),
                exit_node: ExitNodeConfig::Auto,
                failover: true,
                connection_timeout: Duration::from_secs(30),
                preferred_transport: None,
            },
        }
    }

    #[test]
    fn test_validate_name() {
        assert!(TunnelStore::validate_name("test").is_ok());
        assert!(TunnelStore::validate_name("test-123").is_ok());
        assert!(TunnelStore::validate_name("test_tunnel").is_ok());
        assert!(TunnelStore::validate_name("").is_err());
        assert!(TunnelStore::validate_name("test/path").is_err());
        assert!(TunnelStore::validate_name("test..tunnel").is_err());
    }

    #[test]
    fn test_save_and_load() {
        let (store, _temp) = create_test_store();
        let tunnel = create_test_tunnel("test");

        store.save(&tunnel).unwrap();
        let loaded = store.load("test").unwrap();

        assert_eq!(loaded.name, "test");
        assert!(loaded.enabled);
        assert_eq!(loaded.config.local_host, "localhost");
    }

    #[test]
    fn test_list() {
        let (store, _temp) = create_test_store();

        store.save(&create_test_tunnel("tunnel1")).unwrap();
        store.save(&create_test_tunnel("tunnel2")).unwrap();

        let tunnels = store.list().unwrap();
        assert_eq!(tunnels.len(), 2);
        assert_eq!(tunnels[0].name, "tunnel1");
        assert_eq!(tunnels[1].name, "tunnel2");
    }

    #[test]
    fn test_enable_disable() {
        let (store, _temp) = create_test_store();
        let mut tunnel = create_test_tunnel("test");
        tunnel.enabled = false;
        store.save(&tunnel).unwrap();

        store.enable("test").unwrap();
        let loaded = store.load("test").unwrap();
        assert!(loaded.enabled);

        store.disable("test").unwrap();
        let loaded = store.load("test").unwrap();
        assert!(!loaded.enabled);
    }

    #[test]
    fn test_remove() {
        let (store, _temp) = create_test_store();
        let tunnel = create_test_tunnel("test");
        store.save(&tunnel).unwrap();

        assert!(store.exists("test"));
        store.remove("test").unwrap();
        assert!(!store.exists("test"));
    }

    #[test]
    fn test_list_enabled() {
        let (store, _temp) = create_test_store();

        let mut tunnel1 = create_test_tunnel("tunnel1");
        tunnel1.enabled = true;
        store.save(&tunnel1).unwrap();

        let mut tunnel2 = create_test_tunnel("tunnel2");
        tunnel2.enabled = false;
        store.save(&tunnel2).unwrap();

        let enabled = store.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "tunnel1");
    }
}
