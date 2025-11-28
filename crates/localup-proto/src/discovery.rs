//! Protocol discovery types for multi-transport support
//!
//! Clients can discover available transport protocols by fetching:
//! `GET /.well-known/localup-protocols`
//!
//! This returns a JSON document describing available transports.

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Available transport protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TransportProtocol {
    /// QUIC transport (UDP-based, best performance)
    Quic,
    /// WebSocket transport over TLS (TCP, firewall-friendly)
    WebSocket,
    /// HTTP/2 transport over TLS (TCP, most compatible)
    H2,
}

impl TransportProtocol {
    /// Returns the default port for this protocol
    pub fn default_port(&self) -> u16 {
        match self {
            TransportProtocol::Quic => 4443,
            TransportProtocol::WebSocket => 443,
            TransportProtocol::H2 => 443,
        }
    }

    /// Returns whether this protocol uses UDP
    pub fn is_udp(&self) -> bool {
        matches!(self, TransportProtocol::Quic)
    }

    /// Returns priority for automatic selection (higher = try first)
    pub fn priority(&self) -> u8 {
        match self {
            TransportProtocol::Quic => 100,     // Best performance
            TransportProtocol::WebSocket => 50, // Good compatibility
            TransportProtocol::H2 => 25,        // Fallback
        }
    }
}

impl std::fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportProtocol::Quic => write!(f, "quic"),
            TransportProtocol::WebSocket => write!(f, "websocket"),
            TransportProtocol::H2 => write!(f, "h2"),
        }
    }
}

impl std::str::FromStr for TransportProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "quic" => Ok(TransportProtocol::Quic),
            "websocket" | "ws" | "wss" => Ok(TransportProtocol::WebSocket),
            "h2" | "http2" => Ok(TransportProtocol::H2),
            _ => Err(format!("Unknown transport protocol: {}", s)),
        }
    }
}

/// Information about a transport endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TransportEndpoint {
    /// Protocol type
    pub protocol: TransportProtocol,
    /// Port number (relative to the relay's address)
    pub port: u16,
    /// Path (for WebSocket: "/localup", for H2: typically empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether this endpoint is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Protocol discovery response
///
/// Returned from `GET /.well-known/localup-protocols`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ProtocolDiscoveryResponse {
    /// Version of the discovery protocol
    pub version: u32,
    /// Relay identifier (hostname or ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_id: Option<String>,
    /// Available transport endpoints
    pub transports: Vec<TransportEndpoint>,
    /// Protocol version supported by the relay
    pub protocol_version: u32,
}

impl Default for ProtocolDiscoveryResponse {
    fn default() -> Self {
        Self {
            version: 1,
            relay_id: None,
            transports: vec![],
            protocol_version: super::PROTOCOL_VERSION,
        }
    }
}

impl ProtocolDiscoveryResponse {
    /// Create a new discovery response with default QUIC transport only
    pub fn quic_only(port: u16) -> Self {
        Self {
            version: 1,
            relay_id: None,
            transports: vec![TransportEndpoint {
                protocol: TransportProtocol::Quic,
                port,
                path: None,
                enabled: true,
            }],
            protocol_version: super::PROTOCOL_VERSION,
        }
    }

    /// Add a transport endpoint
    pub fn with_transport(mut self, endpoint: TransportEndpoint) -> Self {
        self.transports.push(endpoint);
        self
    }

    /// Add QUIC transport
    pub fn with_quic(self, port: u16) -> Self {
        self.with_transport(TransportEndpoint {
            protocol: TransportProtocol::Quic,
            port,
            path: None,
            enabled: true,
        })
    }

    /// Add WebSocket transport
    pub fn with_websocket(self, port: u16, path: &str) -> Self {
        self.with_transport(TransportEndpoint {
            protocol: TransportProtocol::WebSocket,
            port,
            path: Some(path.to_string()),
            enabled: true,
        })
    }

    /// Add HTTP/2 transport
    pub fn with_h2(self, port: u16) -> Self {
        self.with_transport(TransportEndpoint {
            protocol: TransportProtocol::H2,
            port,
            path: None,
            enabled: true,
        })
    }

    /// Set relay ID
    pub fn with_relay_id(mut self, id: &str) -> Self {
        self.relay_id = Some(id.to_string());
        self
    }

    /// Get transports sorted by priority (highest first)
    pub fn sorted_transports(&self) -> Vec<&TransportEndpoint> {
        let mut transports: Vec<_> = self.transports.iter().filter(|t| t.enabled).collect();
        transports.sort_by(|a, b| b.protocol.priority().cmp(&a.protocol.priority()));
        transports
    }

    /// Find the best available transport
    pub fn best_transport(&self) -> Option<&TransportEndpoint> {
        self.sorted_transports().first().copied()
    }

    /// Find a specific transport protocol
    pub fn find_transport(&self, protocol: TransportProtocol) -> Option<&TransportEndpoint> {
        self.transports
            .iter()
            .find(|t| t.protocol == protocol && t.enabled)
    }
}

/// Well-known path for protocol discovery
pub const WELL_KNOWN_PATH: &str = "/.well-known/localup-protocols";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_priority() {
        assert!(TransportProtocol::Quic.priority() > TransportProtocol::WebSocket.priority());
        assert!(TransportProtocol::WebSocket.priority() > TransportProtocol::H2.priority());
    }

    #[test]
    fn test_protocol_from_str() {
        assert_eq!(
            "quic".parse::<TransportProtocol>().unwrap(),
            TransportProtocol::Quic
        );
        assert_eq!(
            "websocket".parse::<TransportProtocol>().unwrap(),
            TransportProtocol::WebSocket
        );
        assert_eq!(
            "h2".parse::<TransportProtocol>().unwrap(),
            TransportProtocol::H2
        );
    }

    #[test]
    fn test_discovery_response() {
        let response = ProtocolDiscoveryResponse::default()
            .with_quic(4443)
            .with_websocket(443, "/localup")
            .with_h2(443);

        assert_eq!(response.transports.len(), 3);
        assert_eq!(
            response.best_transport().unwrap().protocol,
            TransportProtocol::Quic
        );
    }

    #[test]
    fn test_serialization() {
        let response = ProtocolDiscoveryResponse::default()
            .with_quic(4443)
            .with_relay_id("relay-001");

        let json = serde_json::to_string(&response).unwrap();
        let parsed: ProtocolDiscoveryResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.relay_id, Some("relay-001".to_string()));
        assert_eq!(parsed.transports.len(), 1);
    }
}
