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
        tunnel_id: String,
        auth_token: String,
        protocols: Vec<Protocol>,
        config: TunnelConfig,
    },
    Connected {
        tunnel_id: String,
        endpoints: Vec<Endpoint>,
    },
    Disconnect {
        reason: String,
    },
    DisconnectAck {
        tunnel_id: String,
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
    /// TLS tunnel with SNI routing
    Tls { port: u16, sni_pattern: String },
    /// HTTP tunnel - subdomain is optional (auto-generated if None)
    Http { subdomain: Option<String> },
    /// HTTPS tunnel - subdomain is optional (auto-generated if None)
    Https { subdomain: Option<String> },
}

/// Tunnel endpoint information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Endpoint {
    pub protocol: Protocol,
    pub public_url: String,
    pub port: Option<u16>,
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
        };
        let serialized = bincode::serialize(&protocol).unwrap();
        let deserialized: Protocol = bincode::deserialize(&serialized).unwrap();
        assert_eq!(protocol, deserialized);
    }
}
