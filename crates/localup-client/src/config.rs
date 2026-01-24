//! Client configuration

use localup_proto::{ExitNodeConfig, HttpAuthConfig, TransportProtocol};
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
    /// Supports multiple patterns including wildcards (e.g., "*.example.com")
    /// No domain validation - relay simply routes based on SNI match
    Tls {
        local_port: u16,
        /// SNI hostnames/patterns for routing
        /// Examples: "api.example.com", "*.local.example.com", "*.example.com"
        #[serde(default)]
        sni_hostnames: Vec<String>,
    },
    /// HTTP with host-based routing
    Http {
        local_port: u16,
        subdomain: Option<String>,
        /// Full custom domain (e.g., "api.example.com") - requires DNS pointing to relay
        /// and certificate to be provisioned. Takes precedence over subdomain.
        #[serde(default)]
        custom_domain: Option<String>,
    },
    /// HTTPS with automatic certificate management
    Https {
        local_port: u16,
        subdomain: Option<String>,
        /// Full custom domain (e.g., "api.example.com") - requires DNS pointing to relay
        /// and valid TLS certificate. Takes precedence over subdomain.
        #[serde(default)]
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
    /// Preferred transport protocol (None = auto-discover and select best)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<TransportProtocol>,
    /// HTTP authentication configuration for incoming requests to this tunnel
    #[serde(default)]
    pub http_auth: HttpAuthConfig,
    /// IP addresses and CIDR ranges allowed to access this tunnel
    /// Empty list means all IPs are allowed
    #[serde(default)]
    pub ip_allowlist: Vec<String>,
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
            preferred_transport: None, // Auto-discover
            http_auth: HttpAuthConfig::None,
            ip_allowlist: Vec::new(), // Empty = allow all
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

    pub fn preferred_transport(mut self, transport: Option<TransportProtocol>) -> Self {
        self.config.preferred_transport = transport;
        self
    }

    /// Configure HTTP authentication for incoming requests
    pub fn http_auth(mut self, auth: HttpAuthConfig) -> Self {
        self.config.http_auth = auth;
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
    fn test_config_builder_with_custom_domain() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Https {
                local_port: 3000,
                subdomain: None,
                custom_domain: Some("api.example.com".to_string()),
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        assert_eq!(config.auth_token, "test-token");
        assert_eq!(config.protocols.len(), 1);
        match &config.protocols[0] {
            ProtocolConfig::Https { custom_domain, .. } => {
                assert_eq!(custom_domain.as_deref(), Some("api.example.com"));
            }
            _ => panic!("Expected HTTPS protocol"),
        }
    }

    #[test]
    fn test_config_builder_custom_domain_precedence() {
        // When both subdomain and custom_domain are set, custom_domain takes precedence
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Http {
                local_port: 8080,
                subdomain: Some("myapp".to_string()),
                custom_domain: Some("api.mycompany.com".to_string()),
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        match &config.protocols[0] {
            ProtocolConfig::Http {
                subdomain,
                custom_domain,
                ..
            } => {
                // Both can be set, but custom_domain takes precedence in routing
                assert_eq!(subdomain.as_deref(), Some("myapp"));
                assert_eq!(custom_domain.as_deref(), Some("api.mycompany.com"));
            }
            _ => panic!("Expected HTTP protocol"),
        }
    }

    #[test]
    fn test_config_builder_missing_token() {
        let result = TunnelConfig::builder()
            .protocol(ProtocolConfig::Http {
                local_port: 8080,
                subdomain: None,
                custom_domain: None,
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

    #[test]
    fn test_tls_config_with_single_sni_hostname() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Tls {
                local_port: 443,
                sni_hostnames: vec!["api.example.com".to_string()],
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        match &config.protocols[0] {
            ProtocolConfig::Tls {
                local_port,
                sni_hostnames,
            } => {
                assert_eq!(*local_port, 443);
                assert_eq!(sni_hostnames.len(), 1);
                assert_eq!(sni_hostnames[0], "api.example.com");
            }
            _ => panic!("Expected TLS protocol"),
        }
    }

    #[test]
    fn test_tls_config_with_multiple_sni_hostnames() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Tls {
                local_port: 443,
                sni_hostnames: vec![
                    "api.example.com".to_string(),
                    "web.example.com".to_string(),
                    "admin.example.com".to_string(),
                ],
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        match &config.protocols[0] {
            ProtocolConfig::Tls {
                local_port,
                sni_hostnames,
            } => {
                assert_eq!(*local_port, 443);
                assert_eq!(sni_hostnames.len(), 3);
                assert_eq!(sni_hostnames[0], "api.example.com");
                assert_eq!(sni_hostnames[1], "web.example.com");
                assert_eq!(sni_hostnames[2], "admin.example.com");
            }
            _ => panic!("Expected TLS protocol"),
        }
    }

    #[test]
    fn test_tls_config_with_wildcard_patterns() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Tls {
                local_port: 443,
                sni_hostnames: vec![
                    "*.example.com".to_string(),
                    "*.local.myapp.dev".to_string(),
                    "api.specific.com".to_string(),
                ],
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        match &config.protocols[0] {
            ProtocolConfig::Tls { sni_hostnames, .. } => {
                assert_eq!(sni_hostnames.len(), 3);
                assert!(sni_hostnames[0].starts_with("*."));
                assert!(sni_hostnames[1].starts_with("*."));
                assert!(!sni_hostnames[2].starts_with("*."));
            }
            _ => panic!("Expected TLS protocol"),
        }
    }

    #[test]
    fn test_tls_config_with_empty_sni_hostnames() {
        // Empty hostnames should be valid - relay will assign default
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Tls {
                local_port: 443,
                sni_hostnames: vec![],
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        match &config.protocols[0] {
            ProtocolConfig::Tls { sni_hostnames, .. } => {
                assert!(sni_hostnames.is_empty());
            }
            _ => panic!("Expected TLS protocol"),
        }
    }

    #[test]
    fn test_tls_config_serialization_roundtrip() {
        let original = TunnelConfig::builder()
            .protocol(ProtocolConfig::Tls {
                local_port: 8443,
                sni_hostnames: vec![
                    "*.local-abc123.myapp.dev".to_string(),
                    "api.production.com".to_string(),
                ],
            })
            .auth_token("test-token".to_string())
            .build()
            .unwrap();

        // Serialize to JSON
        let json = serde_json::to_string(&original).unwrap();

        // Deserialize back
        let restored: TunnelConfig = serde_json::from_str(&json).unwrap();

        match &restored.protocols[0] {
            ProtocolConfig::Tls {
                local_port,
                sni_hostnames,
            } => {
                assert_eq!(*local_port, 8443);
                assert_eq!(sni_hostnames.len(), 2);
                assert_eq!(sni_hostnames[0], "*.local-abc123.myapp.dev");
                assert_eq!(sni_hostnames[1], "api.production.com");
            }
            _ => panic!("Expected TLS protocol"),
        }
    }
}
