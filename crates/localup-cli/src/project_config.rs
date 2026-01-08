//! Project-level configuration file support
//!
//! Enables defining multiple tunnels in a single `.localup.yml` file
//! with hierarchical discovery from current directory to home.

use anyhow::{Context, Result};
use localup_client::{ExitNodeConfig, ProtocolConfig, TunnelConfig};
use localup_proto::{HttpAuthConfig, TransportProtocol};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

use crate::config::ConfigManager;

/// Project-level configuration file format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    /// Global default settings applied to all tunnels
    #[serde(default)]
    pub defaults: ProjectDefaults,

    /// Tunnel definitions
    #[serde(default)]
    pub tunnels: Vec<ProjectTunnel>,
}

/// Default settings applied to all tunnels unless overridden
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectDefaults {
    /// Default relay server address
    pub relay: Option<String>,

    /// Default authentication token (supports ${ENV_VAR} expansion)
    pub token: Option<String>,

    /// Default transport protocol (quic, h2, websocket)
    pub transport: Option<String>,

    /// Default local host
    #[serde(default = "default_local_host")]
    pub local_host: String,

    /// Default connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_local_host() -> String {
    "localhost".to_string()
}

fn default_timeout() -> u64 {
    30
}

/// A single tunnel definition in the project config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTunnel {
    /// Tunnel name (required, must be unique)
    pub name: String,

    /// Local port to expose
    pub port: u16,

    /// Protocol: http, https, tcp, tls
    #[serde(default = "default_protocol")]
    pub protocol: String,

    /// Subdomain for HTTP/HTTPS/TLS tunnels
    pub subdomain: Option<String>,

    /// Custom domain for HTTP/HTTPS tunnels (e.g., "api.example.com")
    /// Requires DNS pointing to relay and valid TLS certificate.
    /// Takes precedence over subdomain when specified.
    #[serde(default)]
    pub custom_domain: Option<String>,

    /// Remote port for TCP tunnels
    pub remote_port: Option<u16>,

    /// SNI hostname for TLS tunnels
    pub sni_hostname: Option<String>,

    /// Override relay server for this tunnel
    pub relay: Option<String>,

    /// Override auth token for this tunnel
    pub token: Option<String>,

    /// Override transport for this tunnel
    pub transport: Option<String>,

    /// Whether tunnel is enabled (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Local host override
    pub local_host: Option<String>,

    /// Allowed IP addresses or CIDR ranges
    /// If empty or not specified, all IPs are allowed
    #[serde(default, rename = "allow_ips")]
    pub ip_allowlist: Vec<String>,
}

fn default_protocol() -> String {
    "http".to_string()
}

fn default_enabled() -> bool {
    true
}

impl Default for ProjectTunnel {
    fn default() -> Self {
        Self {
            name: String::new(),
            port: 0,
            protocol: default_protocol(),
            subdomain: None,
            custom_domain: None,
            remote_port: None,
            sni_hostname: None,
            relay: None,
            token: None,
            transport: None,
            enabled: true,
            local_host: None,
            ip_allowlist: Vec::new(),
        }
    }
}

/// Type alias for use in CLI add/remove commands
pub type TunnelEntry = ProjectTunnel;

impl ProjectConfig {
    /// Discover and load project config by walking up the directory tree
    ///
    /// Searches for `.localup.yml` or `.localup.yaml` starting from
    /// the current directory and walking up to the filesystem root.
    pub fn discover() -> Result<Option<(PathBuf, Self)>> {
        let current_dir = std::env::current_dir()?;
        Self::discover_from(&current_dir)
    }

    /// Save config to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self).context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;
        Ok(())
    }

    /// Discover config starting from a specific directory
    pub fn discover_from(start_dir: &Path) -> Result<Option<(PathBuf, Self)>> {
        let mut current = start_dir.to_path_buf();

        loop {
            // Check for .localup.yml
            let yml_path = current.join(".localup.yml");
            if yml_path.exists() {
                let config = Self::load(&yml_path)?;
                return Ok(Some((yml_path, config)));
            }

            // Check for .localup.yaml
            let yaml_path = current.join(".localup.yaml");
            if yaml_path.exists() {
                let config = Self::load(&yaml_path)?;
                return Ok(Some((yaml_path, config)));
            }

            // Move to parent directory
            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    /// Load config from a specific file path
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        Self::parse(&content)
    }

    /// Parse config from YAML string
    pub fn parse(content: &str) -> Result<Self> {
        let config: ProjectConfig =
            serde_yaml::from_str(content).context("Failed to parse YAML config")?;

        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        // Check for duplicate tunnel names
        let mut names = std::collections::HashSet::new();
        for tunnel in &self.tunnels {
            if !names.insert(&tunnel.name) {
                anyhow::bail!("Duplicate tunnel name: {}", tunnel.name);
            }

            // Validate tunnel name format
            if !is_valid_tunnel_name(&tunnel.name) {
                anyhow::bail!(
                    "Invalid tunnel name '{}': must be alphanumeric with hyphens/underscores only",
                    tunnel.name
                );
            }

            // Validate protocol
            let protocol = tunnel.protocol.to_lowercase();
            if !["http", "https", "tcp", "tls"].contains(&protocol.as_str()) {
                anyhow::bail!(
                    "Invalid protocol '{}' for tunnel '{}': must be http, https, tcp, or tls",
                    tunnel.protocol,
                    tunnel.name
                );
            }
        }

        Ok(())
    }

    /// Get enabled tunnels only
    pub fn enabled_tunnels(&self) -> Vec<&ProjectTunnel> {
        self.tunnels.iter().filter(|t| t.enabled).collect()
    }

    /// Find a tunnel by name
    pub fn get_tunnel(&self, name: &str) -> Option<&ProjectTunnel> {
        self.tunnels.iter().find(|t| t.name == name)
    }

    /// Generate a template config file content
    pub fn template() -> String {
        r#"# Localup Project Configuration
# See: https://github.com/example/localup for documentation

defaults:
  # relay: "relay.localup.io:4443"
  # token: "${LOCALUP_TOKEN}"
  local_host: "localhost"
  timeout_seconds: 30

tunnels:
  - name: api
    port: 3000
    protocol: http
    subdomain: my-api

  # - name: db
  #   port: 5432
  #   protocol: tcp
  #   remote_port: 15432

  # - name: frontend
  #   port: 3001
  #   protocol: https
  #   subdomain: my-frontend
  #   enabled: false

  # Custom domain example (requires DNS pointing to relay and valid certificate)
  # - name: production-api
  #   port: 8080
  #   protocol: https
  #   custom_domain: api.example.com
"#
        .to_string()
    }
}

impl ProjectTunnel {
    /// Convert to TunnelConfig using defaults from ProjectDefaults
    pub fn to_tunnel_config(&self, defaults: &ProjectDefaults) -> Result<TunnelConfig> {
        // Resolve values with defaults
        let relay = self
            .relay
            .as_ref()
            .or(defaults.relay.as_ref())
            .cloned()
            .unwrap_or_else(|| "localhost:4443".to_string());

        // Token resolution order:
        // 1. Tunnel-specific token (with env var expansion)
        // 2. Defaults token (with env var expansion)
        // 3. ConfigManager::get_token() (from `config set-token`)
        let raw_token = self
            .token
            .as_ref()
            .or(defaults.token.as_ref())
            .cloned()
            .unwrap_or_default();

        let expanded_token = expand_env_vars(&raw_token);

        // If token is empty after expansion, try to get from config
        let token = if expanded_token.is_empty() {
            match ConfigManager::get_token() {
                Ok(Some(t)) => {
                    info!("Using saved auth token from ~/.localup/config.json");
                    t
                }
                _ => expanded_token,
            }
        } else {
            expanded_token
        };

        let local_host = self
            .local_host
            .as_ref()
            .cloned()
            .unwrap_or_else(|| defaults.local_host.clone());

        // Build protocol config
        let protocol = self.protocol.to_lowercase();
        let protocol_config = match protocol.as_str() {
            "http" => ProtocolConfig::Http {
                local_port: self.port,
                subdomain: self.subdomain.clone(),
                custom_domain: self.custom_domain.clone(),
            },
            "https" => ProtocolConfig::Https {
                local_port: self.port,
                subdomain: self.subdomain.clone(),
                custom_domain: self.custom_domain.clone(),
            },
            "tcp" => ProtocolConfig::Tcp {
                local_port: self.port,
                remote_port: self.remote_port,
            },
            "tls" => ProtocolConfig::Tls {
                local_port: self.port,
                sni_hostname: self.sni_hostname.clone(),
            },
            _ => anyhow::bail!("Unknown protocol: {}", self.protocol),
        };

        // Parse preferred transport
        let preferred_transport = self
            .transport
            .as_ref()
            .or(defaults.transport.as_ref())
            .and_then(|t| match t.to_lowercase().as_str() {
                "quic" => Some(TransportProtocol::Quic),
                "h2" | "http2" => Some(TransportProtocol::H2),
                "websocket" | "ws" => Some(TransportProtocol::WebSocket),
                _ => None,
            });

        Ok(TunnelConfig {
            local_host,
            protocols: vec![protocol_config],
            auth_token: token,
            exit_node: ExitNodeConfig::Custom(relay),
            failover: true,
            connection_timeout: Duration::from_secs(defaults.timeout_seconds),
            preferred_transport,
            http_auth: HttpAuthConfig::None,
            ip_allowlist: self.ip_allowlist.clone(),
        })
    }
}

/// Check if a tunnel name is valid (alphanumeric, hyphens, underscores)
fn is_valid_tunnel_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Expand environment variables in a string
///
/// Supports `${VAR}` syntax. If the variable is not set, returns empty string.
pub fn expand_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    let re = regex_lite::Regex::new(r"\$\{([^}]+)\}").unwrap();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        let var_value = std::env::var(var_name).unwrap_or_default();
        result = result.replace(&cap[0], &var_value);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
tunnels:
  - name: api
    port: 3000
"#;
        let config = ProjectConfig::parse(yaml).unwrap();
        assert_eq!(config.tunnels.len(), 1);
        assert_eq!(config.tunnels[0].name, "api");
        assert_eq!(config.tunnels[0].port, 3000);
        assert_eq!(config.tunnels[0].protocol, "http"); // default
        assert!(config.tunnels[0].enabled); // default true
    }

    #[test]
    fn test_parse_full_config() {
        let yaml = r#"
defaults:
  relay: "relay.example.com:4443"
  token: "my-token"
  transport: "quic"
  local_host: "127.0.0.1"
  timeout_seconds: 60

tunnels:
  - name: api
    port: 3000
    protocol: http
    subdomain: my-api

  - name: db
    port: 5432
    protocol: tcp
    remote_port: 15432
    enabled: false

  - name: frontend
    port: 3001
    protocol: https
    subdomain: my-frontend
    relay: "other-relay.example.com:4443"
    token: "other-token"
"#;
        let config = ProjectConfig::parse(yaml).unwrap();

        // Check defaults
        assert_eq!(
            config.defaults.relay,
            Some("relay.example.com:4443".to_string())
        );
        assert_eq!(config.defaults.token, Some("my-token".to_string()));
        assert_eq!(config.defaults.transport, Some("quic".to_string()));
        assert_eq!(config.defaults.local_host, "127.0.0.1");
        assert_eq!(config.defaults.timeout_seconds, 60);

        // Check tunnels
        assert_eq!(config.tunnels.len(), 3);

        let api = &config.tunnels[0];
        assert_eq!(api.name, "api");
        assert_eq!(api.port, 3000);
        assert_eq!(api.protocol, "http");
        assert_eq!(api.subdomain, Some("my-api".to_string()));
        assert!(api.enabled);

        let db = &config.tunnels[1];
        assert_eq!(db.name, "db");
        assert_eq!(db.port, 5432);
        assert_eq!(db.protocol, "tcp");
        assert_eq!(db.remote_port, Some(15432));
        assert!(!db.enabled);

        let frontend = &config.tunnels[2];
        assert_eq!(frontend.name, "frontend");
        assert_eq!(
            frontend.relay,
            Some("other-relay.example.com:4443".to_string())
        );
        assert_eq!(frontend.token, Some("other-token".to_string()));
    }

    #[test]
    fn test_enabled_tunnels() {
        let yaml = r#"
tunnels:
  - name: enabled1
    port: 3000
  - name: disabled
    port: 3001
    enabled: false
  - name: enabled2
    port: 3002
"#;
        let config = ProjectConfig::parse(yaml).unwrap();
        let enabled = config.enabled_tunnels();
        assert_eq!(enabled.len(), 2);
        assert_eq!(enabled[0].name, "enabled1");
        assert_eq!(enabled[1].name, "enabled2");
    }

    #[test]
    fn test_get_tunnel() {
        let yaml = r#"
tunnels:
  - name: api
    port: 3000
  - name: db
    port: 5432
"#;
        let config = ProjectConfig::parse(yaml).unwrap();

        assert!(config.get_tunnel("api").is_some());
        assert!(config.get_tunnel("db").is_some());
        assert!(config.get_tunnel("unknown").is_none());
    }

    #[test]
    fn test_duplicate_tunnel_names() {
        let yaml = r#"
tunnels:
  - name: api
    port: 3000
  - name: api
    port: 3001
"#;
        let result = ProjectConfig::parse(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate"));
    }

    #[test]
    fn test_invalid_tunnel_name() {
        let yaml = r#"
tunnels:
  - name: "my app"
    port: 3000
"#;
        let result = ProjectConfig::parse(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid tunnel name"));
    }

    #[test]
    fn test_invalid_protocol() {
        let yaml = r#"
tunnels:
  - name: api
    port: 3000
    protocol: ftp
"#;
        let result = ProjectConfig::parse(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid protocol"));
    }

    #[test]
    fn test_valid_tunnel_names() {
        assert!(is_valid_tunnel_name("api"));
        assert!(is_valid_tunnel_name("my-api"));
        assert!(is_valid_tunnel_name("my_api"));
        assert!(is_valid_tunnel_name("api123"));
        assert!(is_valid_tunnel_name("API"));
        assert!(is_valid_tunnel_name("my-api-v2"));

        assert!(!is_valid_tunnel_name(""));
        assert!(!is_valid_tunnel_name("my api"));
        assert!(!is_valid_tunnel_name("my.api"));
        assert!(!is_valid_tunnel_name("api@test"));
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "test_value");
        std::env::set_var("ANOTHER_VAR", "another");

        assert_eq!(expand_env_vars("${TEST_VAR}"), "test_value");
        assert_eq!(
            expand_env_vars("prefix_${TEST_VAR}_suffix"),
            "prefix_test_value_suffix"
        );
        assert_eq!(
            expand_env_vars("${TEST_VAR}_${ANOTHER_VAR}"),
            "test_value_another"
        );
        assert_eq!(expand_env_vars("no_vars"), "no_vars");
        assert_eq!(expand_env_vars("${NONEXISTENT_VAR}"), "");

        std::env::remove_var("TEST_VAR");
        std::env::remove_var("ANOTHER_VAR");
    }

    #[test]
    fn test_to_tunnel_config_http() {
        let defaults = ProjectDefaults {
            relay: Some("relay.example.com:4443".to_string()),
            token: Some("default-token".to_string()),
            transport: None,
            local_host: "localhost".to_string(),
            timeout_seconds: 30,
        };

        let tunnel = ProjectTunnel {
            name: "api".to_string(),
            port: 3000,
            protocol: "http".to_string(),
            subdomain: Some("my-api".to_string()),
            custom_domain: None,
            remote_port: None,
            sni_hostname: None,
            relay: None,
            token: None,
            transport: None,
            enabled: true,
            local_host: None,
        };

        let config = tunnel.to_tunnel_config(&defaults).unwrap();

        assert_eq!(config.local_host, "localhost");
        assert_eq!(config.auth_token, "default-token");
        assert_eq!(config.protocols.len(), 1);

        if let ProtocolConfig::Http {
            local_port,
            subdomain,
            custom_domain: _,
        } = &config.protocols[0]
        {
            assert_eq!(*local_port, 3000);
            assert_eq!(subdomain, &Some("my-api".to_string()));
        } else {
            panic!("Expected HTTP protocol");
        }
    }

    #[test]
    fn test_to_tunnel_config_tcp() {
        let defaults = ProjectDefaults::default();

        let tunnel = ProjectTunnel {
            name: "db".to_string(),
            port: 5432,
            protocol: "tcp".to_string(),
            subdomain: None,
            custom_domain: None,
            remote_port: Some(15432),
            sni_hostname: None,
            relay: Some("custom-relay:4443".to_string()),
            token: Some("custom-token".to_string()),
            transport: Some("quic".to_string()),
            enabled: true,
            local_host: Some("127.0.0.1".to_string()),
        };

        let config = tunnel.to_tunnel_config(&defaults).unwrap();

        assert_eq!(config.local_host, "127.0.0.1");
        assert_eq!(config.auth_token, "custom-token");
        assert!(config.preferred_transport.is_some());

        if let ProtocolConfig::Tcp {
            local_port,
            remote_port,
        } = &config.protocols[0]
        {
            assert_eq!(*local_port, 5432);
            assert_eq!(*remote_port, Some(15432));
        } else {
            panic!("Expected TCP protocol");
        }
    }

    #[test]
    fn test_to_tunnel_config_with_env_var_token() {
        std::env::set_var("MY_TOKEN", "secret-from-env");

        let defaults = ProjectDefaults {
            token: Some("${MY_TOKEN}".to_string()),
            ..Default::default()
        };

        let tunnel = ProjectTunnel {
            name: "api".to_string(),
            port: 3000,
            protocol: "http".to_string(),
            subdomain: None,
            custom_domain: None,
            remote_port: None,
            sni_hostname: None,
            relay: None,
            token: None,
            transport: None,
            enabled: true,
            local_host: None,
        };

        let config = tunnel.to_tunnel_config(&defaults).unwrap();
        assert_eq!(config.auth_token, "secret-from-env");

        std::env::remove_var("MY_TOKEN");
    }

    #[test]
    fn test_template_is_valid_yaml() {
        let template = ProjectConfig::template();
        let config = ProjectConfig::parse(&template).unwrap();
        assert!(!config.tunnels.is_empty());
    }

    #[test]
    fn test_discover_from_creates_path() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".localup.yml");

        // Create a config file
        let yaml = r#"
tunnels:
  - name: test
    port: 3000
"#;
        std::fs::write(&config_path, yaml).unwrap();

        // Discover from temp dir
        let result = ProjectConfig::discover_from(temp_dir.path()).unwrap();
        assert!(result.is_some());

        let (path, config) = result.unwrap();
        assert_eq!(path, config_path);
        assert_eq!(config.tunnels[0].name, "test");
    }

    #[test]
    fn test_discover_from_nested_dir() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".localup.yml");
        let nested_dir = temp_dir.path().join("nested").join("deep");
        std::fs::create_dir_all(&nested_dir).unwrap();

        // Create config at root
        let yaml = r#"
tunnels:
  - name: root-tunnel
    port: 3000
"#;
        std::fs::write(&config_path, yaml).unwrap();

        // Discover from nested dir should find parent config
        let result = ProjectConfig::discover_from(&nested_dir).unwrap();
        assert!(result.is_some());

        let (path, config) = result.unwrap();
        assert_eq!(path, config_path);
        assert_eq!(config.tunnels[0].name, "root-tunnel");
    }

    #[test]
    fn test_discover_no_config() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // No config file exists
        let result = ProjectConfig::discover_from(temp_dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_yaml_extension_variants() {
        use tempfile::TempDir;

        // Test .yaml extension
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".localup.yaml");

        let yaml = r#"
tunnels:
  - name: yaml-ext
    port: 3000
"#;
        std::fs::write(&config_path, yaml).unwrap();

        let result = ProjectConfig::discover_from(temp_dir.path()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().1.tunnels[0].name, "yaml-ext");
    }

    #[test]
    fn test_empty_tunnels() {
        let yaml = r#"
defaults:
  relay: "relay.example.com:4443"
tunnels: []
"#;
        let config = ProjectConfig::parse(yaml).unwrap();
        assert!(config.tunnels.is_empty());
        assert!(config.enabled_tunnels().is_empty());
    }

    #[test]
    fn test_tls_protocol() {
        let yaml = r#"
tunnels:
  - name: secure
    port: 443
    protocol: tls
    sni_hostname: "example.com"
"#;
        let config = ProjectConfig::parse(yaml).unwrap();
        let tunnel = &config.tunnels[0];

        let tunnel_config = tunnel
            .to_tunnel_config(&ProjectDefaults::default())
            .unwrap();

        if let ProtocolConfig::Tls {
            local_port,
            sni_hostname,
        } = &tunnel_config.protocols[0]
        {
            assert_eq!(*local_port, 443);
            assert_eq!(sni_hostname, &Some("example.com".to_string()));
        } else {
            panic!("Expected TLS protocol");
        }
    }

    #[test]
    fn test_https_protocol() {
        let yaml = r#"
tunnels:
  - name: frontend
    port: 3001
    protocol: https
    subdomain: my-frontend
"#;
        let config = ProjectConfig::parse(yaml).unwrap();
        let tunnel = &config.tunnels[0];

        let tunnel_config = tunnel
            .to_tunnel_config(&ProjectDefaults::default())
            .unwrap();

        if let ProtocolConfig::Https {
            local_port,
            subdomain,
            custom_domain: _,
        } = &tunnel_config.protocols[0]
        {
            assert_eq!(*local_port, 3001);
            assert_eq!(subdomain, &Some("my-frontend".to_string()));
        } else {
            panic!("Expected HTTPS protocol");
        }
    }

    #[test]
    fn test_transport_parsing() {
        let defaults = ProjectDefaults {
            transport: Some("h2".to_string()),
            ..Default::default()
        };

        let tunnel = ProjectTunnel {
            name: "api".to_string(),
            port: 3000,
            protocol: "http".to_string(),
            subdomain: None,
            custom_domain: None,
            remote_port: None,
            sni_hostname: None,
            relay: None,
            token: None,
            transport: None,
            enabled: true,
            local_host: None,
        };

        let config = tunnel.to_tunnel_config(&defaults).unwrap();
        assert_eq!(config.preferred_transport, Some(TransportProtocol::H2));

        // Test websocket
        let defaults_ws = ProjectDefaults {
            transport: Some("websocket".to_string()),
            ..Default::default()
        };
        let config_ws = tunnel.to_tunnel_config(&defaults_ws).unwrap();
        assert_eq!(
            config_ws.preferred_transport,
            Some(TransportProtocol::WebSocket)
        );

        // Test quic
        let defaults_quic = ProjectDefaults {
            transport: Some("quic".to_string()),
            ..Default::default()
        };
        let config_quic = tunnel.to_tunnel_config(&defaults_quic).unwrap();
        assert_eq!(
            config_quic.preferred_transport,
            Some(TransportProtocol::Quic)
        );
    }
}
