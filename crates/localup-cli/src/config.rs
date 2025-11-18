//! Global CLI configuration management
//!
//! Stores default auth tokens and other global settings in ~/.localup/config.json

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Global CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalupConfig {
    /// Default authentication token for tunnel connections
    pub auth_token: Option<String>,
}

impl Default for LocalupConfig {
    fn default() -> Self {
        Self { auth_token: None }
    }
}

/// Configuration manager
pub struct ConfigManager;

impl ConfigManager {
    /// Get the config file path
    fn get_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".localup").join("config.json"))
    }

    /// Load the configuration from disk
    pub fn load() -> Result<LocalupConfig> {
        let path = Self::get_config_path()?;

        // Return default config if file doesn't exist
        if !path.exists() {
            return Ok(LocalupConfig::default());
        }

        let json =
            fs::read_to_string(&path).context(format!("Failed to read config file: {:?}", path))?;

        let config: LocalupConfig = serde_json::from_str(&json)
            .context(format!("Failed to parse config file: {:?}", path))?;

        Ok(config)
    }

    /// Save the configuration to disk
    pub fn save(config: &LocalupConfig) -> Result<()> {
        let path = Self::get_config_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create config directory: {:?}", parent))?;
        }

        let json = serde_json::to_string_pretty(config).context("Failed to serialize config")?;

        fs::write(&path, json).context(format!("Failed to write config file: {:?}", path))?;

        Ok(())
    }

    /// Set the default auth token
    pub fn set_token(token: String) -> Result<()> {
        let mut config = Self::load()?;
        config.auth_token = Some(token);
        Self::save(&config)
    }

    /// Get the default auth token
    pub fn get_token() -> Result<Option<String>> {
        let config = Self::load()?;
        Ok(config.auth_token)
    }

    /// Clear the default auth token
    pub fn clear_token() -> Result<()> {
        let mut config = Self::load()?;
        config.auth_token = None;
        Self::save(&config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LocalupConfig::default();
        assert!(config.auth_token.is_none());
    }

    #[test]
    fn test_config_serialization() {
        let config = LocalupConfig {
            auth_token: Some("test-token".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: LocalupConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.auth_token, Some("test-token".to_string()));
    }
}
