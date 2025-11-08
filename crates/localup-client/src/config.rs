//! Client configuration

use localup_proto::ExitNodeConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Protocol-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolConfig {
    /// TCP port forwarding
    Tcp {
        local_port: u16,
        remote_port: Option<u16>,
    },
    /// TLS/SNI-based routing
    /// Routes incoming TLS connections based on Server Name Indication (SNI)
    Tls {
        local_port: u16,
        /// SNI hostname for routing (e.g., "api.example.com")
        sni_hostname: Option<String>,
        remote_port: Option<u16>,
    },
    /// HTTP with host-based routing
    Http {
        local_port: u16,
        subdomain: Option<String>,
    },
    /// HTTPS with automatic certificate management
    Https {
        local_port: u16,
        subdomain: Option<String>,
        custom_domain: Option<String>,
    },
}

/// Tunnel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub local_host: String,
    pub protocols: Vec<ProtocolConfig>,
    pub auth_token: String,
    pub exit_node: ExitNodeConfig,
    pub failover: bool,
    #[serde(with = "duration_secs")]
    pub connection_timeout: Duration,
}

/// Helper module for serializing Duration as seconds
mod duration_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            local_host: "localhost".to_string(),
            protocols: Vec::new(),
            auth_token: String::new(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
        }
    }
}

impl TunnelConfig {
    pub fn builder() -> TunnelConfigBuilder {
        TunnelConfigBuilder::default()
    }
}

/// Builder for TunnelConfig
#[derive(Default)]
pub struct TunnelConfigBuilder {
    config: TunnelConfig,
}

impl TunnelConfigBuilder {
    pub fn local_host(mut self, host: String) -> Self {
        self.config.local_host = host;
        self
    }

    pub fn protocol(mut self, protocol: ProtocolConfig) -> Self {
        self.config.protocols.push(protocol);
        self
    }

    pub fn auth_token(mut self, token: String) -> Self {
        self.config.auth_token = token;
        self
    }

    pub fn exit_node(mut self, node: ExitNodeConfig) -> Self {
        self.config.exit_node = node;
        self
    }

    pub fn failover(mut self, enabled: bool) -> Self {
        self.config.failover = enabled;
        self
    }

    pub fn build(self) -> Result<TunnelConfig, String> {
        if self.config.auth_token.is_empty() {
            return Err("auth_token is required".to_string());
        }
        if self.config.protocols.is_empty() {
            return Err("at least one protocol must be configured".to_string());
        }
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Https {
                local_port: 3000,
                subdomain: Some("myapp".to_string()),
                custom_domain: None,
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        assert_eq!(config.auth_token, "test-token");
        assert_eq!(config.protocols.len(), 1);
    }

    #[test]
    fn test_config_builder_missing_token() {
        let result = TunnelConfig::builder()
            .protocol(ProtocolConfig::Http {
                local_port: 8080,
                subdomain: None,
            })
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_config_builder_no_protocols() {
        let result = TunnelConfig::builder()
            .auth_token("token".to_string())
            .build();

        assert!(result.is_err());
    }
}
