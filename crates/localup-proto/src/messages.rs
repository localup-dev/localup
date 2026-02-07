//! Protocol message types

use serde::{Deserialize, Serialize};

/// Main tunnel protocol message enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TunnelMessage {
    // Control messages (Stream ID 0)
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
    },
    Connect {
        localup_id: String,
        auth_token: String,
        protocols: Vec<Protocol>,
        config: TunnelConfig,
    },
    Connected {
        localup_id: String,
        endpoints: Vec<Endpoint>,
    },
    Disconnect {
        reason: String,
    },
    DisconnectAck {
        localup_id: String,
    },

    // Protocol-specific messages
    TcpConnect {
        stream_id: u32,
        remote_addr: String,
        remote_port: u16,
    },
    TcpData {
        stream_id: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    TcpClose {
        stream_id: u32,
    },

    TlsConnect {
        stream_id: u32,
        sni: String,
        #[serde(with = "serde_bytes")]
        client_hello: Vec<u8>,
    },
    TlsData {
        stream_id: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    TlsClose {
        stream_id: u32,
    },

    HttpRequest {
        stream_id: u32,
        method: String,
        uri: String,
        headers: Vec<(String, String)>,
        #[serde(with = "serde_bytes_option")]
        body: Option<Vec<u8>>,
    },
    HttpResponse {
        stream_id: u32,
        status: u16,
        headers: Vec<(String, String)>,
        #[serde(with = "serde_bytes_option")]
        body: Option<Vec<u8>>,
    },
    HttpChunk {
        stream_id: u32,
        #[serde(with = "serde_bytes")]
        chunk: Vec<u8>,
        is_final: bool,
    },

    // Transparent HTTP/HTTPS streaming (for WebSocket, HTTP/2, SSE, etc.)
    HttpStreamConnect {
        stream_id: u32,
        host: String, // For routing only
        #[serde(with = "serde_bytes")]
        initial_data: Vec<u8>, // Raw HTTP request bytes (including headers)
    },
    HttpStreamData {
        stream_id: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    HttpStreamClose {
        stream_id: u32,
    },

    // Reverse tunnel messages (agent-based)
    /// Agent registers with relay and declares what specific address it forwards to
    AgentRegister {
        agent_id: String,
        auth_token: String,
        target_address: String, // Specific address to forward to, e.g., "192.168.1.100:8080"
        metadata: AgentMetadata,
    },
    /// Relay confirms agent registration
    AgentRegistered {
        agent_id: String,
    },
    /// Agent registration rejected (invalid token, etc.)
    AgentRejected {
        reason: String,
    },

    /// Client requests reverse tunnel to remote address through an agent
    ReverseTunnelRequest {
        localup_id: String,
        remote_address: String,      // IP:port format
        agent_id: String,            // Which agent to route through
        agent_token: Option<String>, // Optional JWT token for agent authentication
    },
    /// Relay accepts reverse tunnel and tells client where to bind locally
    ReverseTunnelAccept {
        localup_id: String,
        local_address: String, // Where client should listen
    },
    /// Relay rejects reverse tunnel request
    ReverseTunnelReject {
        localup_id: String,
        reason: String,
    },

    /// Client initiates a new stream for a reverse tunnel connection
    /// (sent on a NEW stream, not the control stream)
    ReverseConnect {
        localup_id: String,
        stream_id: u32,
        remote_address: String,
    },

    /// Relay validates agent token before accepting tunnel
    /// (used for early validation, not per-stream)
    ValidateAgentToken {
        agent_token: Option<String>,
    },
    /// Agent confirms token is valid
    ValidateAgentTokenOk,
    /// Agent rejects token
    ValidateAgentTokenReject {
        reason: String,
    },

    /// Relay asks agent to forward connection to remote address
    ForwardRequest {
        localup_id: String,
        stream_id: u32,
        remote_address: String,
        agent_token: Option<String>, // Optional JWT token for agent authentication
    },
    /// Agent accepts forward request
    ForwardAccept {
        localup_id: String,
        stream_id: u32,
    },
    /// Agent rejects forward request (not in allowlist, etc.)
    ForwardReject {
        localup_id: String,
        stream_id: u32,
        reason: String,
    },

    /// Data forwarding for reverse tunnels (bidirectional)
    ReverseData {
        localup_id: String,
        stream_id: u32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    /// Close reverse tunnel stream (with optional error reason)
    ReverseClose {
        localup_id: String,
        stream_id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

// Custom serde helpers for optional bytes
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(data)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u8>::deserialize(deserializer)
    }
}

mod serde_bytes_option {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match data {
            Some(bytes) => serializer.serialize_some(&bytes),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<Vec<u8>>::deserialize(deserializer)
    }
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    /// TCP tunnel - port will be allocated by server if 0
    Tcp { port: u16 },
    /// TLS tunnel with SNI routing (supports multiple patterns including wildcards)
    /// No domain validation - relay simply routes based on SNI match
    Tls {
        port: u16,
        sni_patterns: Vec<String>,
    },
    /// HTTP tunnel - subdomain is optional (auto-generated if None)
    /// If custom_domain is set, it takes precedence over subdomain
    Http {
        subdomain: Option<String>,
        /// Full custom domain (e.g., "api.example.com") - requires certificate to be provisioned first
        #[serde(default)]
        custom_domain: Option<String>,
    },
    /// HTTPS tunnel - subdomain is optional (auto-generated if None)
    /// If custom_domain is set, it takes precedence over subdomain
    Https {
        subdomain: Option<String>,
        /// Full custom domain (e.g., "api.example.com") - requires certificate to be provisioned first
        #[serde(default)]
        custom_domain: Option<String>,
    },
}

/// Tunnel endpoint information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Endpoint {
    pub protocol: Protocol,
    pub public_url: String,
    pub port: Option<u16>,
}

/// HTTP authentication configuration for incoming requests
///
/// This is extensible to support different authentication methods:
/// - Basic: HTTP Basic Auth (username:password)
/// - BearerToken: Validate specific header token
/// - OAuth/OIDC: (future) OpenID Connect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum HttpAuthConfig {
    /// No authentication required
    #[default]
    None,
    /// HTTP Basic Authentication
    /// Credentials are "username:password" pairs
    Basic { credentials: Vec<String> },
    /// Bearer token in Authorization header
    /// Validates that the header matches one of the provided tokens
    BearerToken { tokens: Vec<String> },
    /// Custom header authentication
    /// Validates a specific header against provided values
    HeaderAuth {
        header_name: String,
        values: Vec<String>,
    },
    // Future: OAuth/OIDC configuration would go here
    // Oidc { provider_url: String, client_id: String, ... }
}

/// Tunnel configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TunnelConfig {
    pub local_host: String,
    pub local_port: Option<u16>,
    pub local_https: bool,
    pub exit_node: ExitNodeConfig,
    pub failover: bool,
    pub ip_allowlist: Vec<String>,
    pub enable_compression: bool,
    pub enable_multiplexing: bool,
    /// HTTP authentication configuration for incoming requests
    #[serde(default)]
    pub http_auth: HttpAuthConfig,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            local_host: "localhost".to_string(),
            local_port: None,
            local_https: false,
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            ip_allowlist: Vec::new(),
            enable_compression: false,
            enable_multiplexing: true,
            http_auth: HttpAuthConfig::None,
        }
    }
}

/// Agent metadata for identification and monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMetadata {
    pub hostname: String,
    pub platform: String,         // e.g., "linux", "macos", "windows"
    pub version: String,          // Agent software version
    pub location: Option<String>, // Optional location info
}

impl Default for AgentMetadata {
    fn default() -> Self {
        Self {
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            platform: std::env::consts::OS.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            location: None,
        }
    }
}

/// Exit node configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExitNodeConfig {
    Auto,
    Nearest,
    Specific(Region),
    MultiRegion(Vec<Region>),
    Custom(String), // Custom relay address (e.g., "relay.example.com:8080")
}

/// Geographic regions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Region {
    UsEast,
    UsWest,
    EuWest,
    EuCentral,
    AsiaPacific,
    SouthAmerica,
}

impl Region {
    pub fn as_str(&self) -> &'static str {
        match self {
            Region::UsEast => "us-east",
            Region::UsWest => "us-west",
            Region::EuWest => "eu-west",
            Region::EuCentral => "eu-central",
            Region::AsiaPacific => "asia-pacific",
            Region::SouthAmerica => "south-america",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = TunnelMessage::Ping { timestamp: 12345 };
        let serialized = bincode::serialize(&msg).unwrap();
        let deserialized: TunnelMessage = bincode::deserialize(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_tcp_data_message() {
        let data = vec![1, 2, 3, 4, 5];
        let msg = TunnelMessage::TcpData {
            stream_id: 42,
            data: data.clone(),
        };

        let serialized = bincode::serialize(&msg).unwrap();
        let deserialized: TunnelMessage = bincode::deserialize(&serialized).unwrap();

        if let TunnelMessage::TcpData {
            stream_id,
            data: recv_data,
        } = deserialized
        {
            assert_eq!(stream_id, 42);
            assert_eq!(recv_data, data);
        } else {
            panic!("Expected TcpData message");
        }
    }

    #[test]
    fn test_protocol_config() {
        let protocol = Protocol::Https {
            subdomain: Some("myapp".to_string()),
            custom_domain: None,
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);
    }

    #[test]
    fn test_tls_protocol_single_sni_pattern() {
        let protocol = Protocol::Tls {
            port: 443,
            sni_patterns: vec!["api.example.com".to_string()],
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);

        if let Protocol::Tls { port, sni_patterns } = deserialized {
            assert_eq!(port, 443);
            assert_eq!(sni_patterns.len(), 1);
            assert_eq!(sni_patterns[0], "api.example.com");
        } else {
            panic!("Expected TLS protocol");
        }
    }

    #[test]
    fn test_tls_protocol_multiple_sni_patterns() {
        let protocol = Protocol::Tls {
            port: 8443,
            sni_patterns: vec![
                "api.example.com".to_string(),
                "web.example.com".to_string(),
                "admin.example.com".to_string(),
            ],
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);

        if let Protocol::Tls { port, sni_patterns } = deserialized {
            assert_eq!(port, 8443);
            assert_eq!(sni_patterns.len(), 3);
            assert_eq!(sni_patterns[0], "api.example.com");
            assert_eq!(sni_patterns[1], "web.example.com");
            assert_eq!(sni_patterns[2], "admin.example.com");
        } else {
            panic!("Expected TLS protocol");
        }
    }

    #[test]
    fn test_tls_protocol_wildcard_patterns() {
        let protocol = Protocol::Tls {
            port: 443,
            sni_patterns: vec![
                "*.example.com".to_string(),
                "*.local-abc123.myapp.dev".to_string(),
                "specific.domain.com".to_string(),
            ],
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);

        if let Protocol::Tls { sni_patterns, .. } = deserialized {
            assert_eq!(sni_patterns.len(), 3);
            assert!(sni_patterns[0].starts_with("*."));
            assert!(sni_patterns[1].starts_with("*."));
            assert!(!sni_patterns[2].starts_with("*."));
        } else {
            panic!("Expected TLS protocol");
        }
    }

    #[test]
    fn test_tls_protocol_empty_patterns() {
        let protocol = Protocol::Tls {
            port: 443,
            sni_patterns: vec![],
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);

        if let Protocol::Tls { sni_patterns, .. } = deserialized {
            assert!(sni_patterns.is_empty());
        } else {
            panic!("Expected TLS protocol");
        }
    }

    #[test]
    fn test_connect_message_with_tls_protocol() {
        let msg = TunnelMessage::Connect {
            localup_id: "tunnel-123".to_string(),
            auth_token: "test-token".to_string(),
            protocols: vec![Protocol::Tls {
                port: 443,
                sni_patterns: vec![
                    "*.local-rqe59t.dviejo.temps.dev".to_string(),
                    "api.production.com".to_string(),
                ],
            }],
            config: TunnelConfig::default(),
        };

        let serialized = bincode::serialize(&msg).unwrap();
        let deserialized: TunnelMessage = bincode::deserialize(&serialized).unwrap();
        assert_eq!(msg, deserialized);

        if let TunnelMessage::Connect { protocols, .. } = deserialized {
            assert_eq!(protocols.len(), 1);
            if let Protocol::Tls { sni_patterns, .. } = &protocols[0] {
                assert_eq!(sni_patterns.len(), 2);
                assert_eq!(sni_patterns[0], "*.local-rqe59t.dviejo.temps.dev");
                assert_eq!(sni_patterns[1], "api.production.com");
            } else {
                panic!("Expected TLS protocol");
            }
        } else {
            panic!("Expected Connect message");
        }
    }
}
