//! Integration tests for transport protocol discovery

use localup_client::{DiscoveredTransport, TransportDiscoverer};
use localup_proto::{ProtocolDiscoveryResponse, TransportProtocol, WELL_KNOWN_PATH};
use std::net::SocketAddr;

#[test]
fn test_transport_discoverer_defaults() {
    // Just verify creation works with defaults
    let _discoverer = TransportDiscoverer::new();
}

#[test]
fn test_transport_discoverer_builder_pattern() {
    use std::time::Duration;

    // Verify builder pattern compiles and chains correctly
    let _discoverer = TransportDiscoverer::new()
        .with_timeout(Duration::from_secs(10))
        .with_insecure(true);
}

#[test]
fn test_select_best_transport_quic_highest_priority() {
    let discoverer = TransportDiscoverer::new();
    let response = ProtocolDiscoveryResponse::default()
        .with_quic(4443)
        .with_websocket(443, "/localup")
        .with_h2(443);

    let base_addr: SocketAddr = "127.0.0.1:443".parse().unwrap();
    let result = discoverer.select_best(&response, base_addr, None).unwrap();

    // QUIC has highest priority (100), should be selected
    assert_eq!(result.protocol, TransportProtocol::Quic);
    assert_eq!(result.address.port(), 4443);
    assert!(result.full_response.is_some());
}

#[test]
fn test_select_preferred_transport_websocket() {
    let discoverer = TransportDiscoverer::new();
    let response = ProtocolDiscoveryResponse::default()
        .with_quic(4443)
        .with_websocket(443, "/localup")
        .with_h2(8443);

    let base_addr: SocketAddr = "192.168.1.1:443".parse().unwrap();
    let result = discoverer
        .select_best(&response, base_addr, Some(TransportProtocol::WebSocket))
        .unwrap();

    // WebSocket should be selected when preferred
    assert_eq!(result.protocol, TransportProtocol::WebSocket);
    assert_eq!(result.address.port(), 443);
    assert_eq!(result.address.ip().to_string(), "192.168.1.1");
    assert_eq!(result.path, Some("/localup".to_string()));
}

#[test]
fn test_select_preferred_transport_h2() {
    let discoverer = TransportDiscoverer::new();
    let response = ProtocolDiscoveryResponse::default()
        .with_quic(4443)
        .with_h2(8443);

    let base_addr: SocketAddr = "10.0.0.1:443".parse().unwrap();
    let result = discoverer
        .select_best(&response, base_addr, Some(TransportProtocol::H2))
        .unwrap();

    // H2 should be selected when preferred
    assert_eq!(result.protocol, TransportProtocol::H2);
    assert_eq!(result.address.port(), 8443);
    assert_eq!(result.address.ip().to_string(), "10.0.0.1");
    assert!(result.path.is_none());
}

#[test]
fn test_select_fallback_when_preferred_unavailable() {
    let discoverer = TransportDiscoverer::new();
    // Only QUIC is available
    let response = ProtocolDiscoveryResponse::default().with_quic(4443);

    let base_addr: SocketAddr = "127.0.0.1:443".parse().unwrap();
    // Request WebSocket (not available)
    let result = discoverer
        .select_best(&response, base_addr, Some(TransportProtocol::WebSocket))
        .unwrap();

    // Should fall back to QUIC
    assert_eq!(result.protocol, TransportProtocol::Quic);
    assert_eq!(result.address.port(), 4443);
}

#[test]
fn test_no_transports_error() {
    let discoverer = TransportDiscoverer::new();
    let response = ProtocolDiscoveryResponse::default(); // Empty transports

    let base_addr: SocketAddr = "127.0.0.1:443".parse().unwrap();
    let result = discoverer.select_best(&response, base_addr, None);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No transports available"));
}

#[test]
fn test_quic_only_fallback_response() {
    let response = ProtocolDiscoveryResponse::quic_only(4443);

    assert_eq!(response.transports.len(), 1);
    assert_eq!(response.transports[0].protocol, TransportProtocol::Quic);
    assert_eq!(response.transports[0].port, 4443);
    assert!(response.transports[0].enabled);
}

#[test]
fn test_transport_protocol_parsing() {
    // Test various string formats
    assert_eq!(
        "quic".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::Quic
    );
    assert_eq!(
        "QUIC".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::Quic
    );
    assert_eq!(
        "websocket".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::WebSocket
    );
    assert_eq!(
        "ws".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::WebSocket
    );
    assert_eq!(
        "wss".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::WebSocket
    );
    assert_eq!(
        "h2".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::H2
    );
    assert_eq!(
        "http2".parse::<TransportProtocol>().unwrap(),
        TransportProtocol::H2
    );

    // Invalid protocol should error
    assert!("invalid".parse::<TransportProtocol>().is_err());
}

#[test]
fn test_transport_protocol_display() {
    assert_eq!(TransportProtocol::Quic.to_string(), "quic");
    assert_eq!(TransportProtocol::WebSocket.to_string(), "websocket");
    assert_eq!(TransportProtocol::H2.to_string(), "h2");
}

#[test]
fn test_transport_protocol_default_ports() {
    assert_eq!(TransportProtocol::Quic.default_port(), 4443);
    assert_eq!(TransportProtocol::WebSocket.default_port(), 443);
    assert_eq!(TransportProtocol::H2.default_port(), 443);
}

#[test]
fn test_transport_protocol_is_udp() {
    assert!(TransportProtocol::Quic.is_udp());
    assert!(!TransportProtocol::WebSocket.is_udp());
    assert!(!TransportProtocol::H2.is_udp());
}

#[test]
fn test_protocol_priority_ordering() {
    // Verify priority order: QUIC > WebSocket > H2
    assert!(TransportProtocol::Quic.priority() > TransportProtocol::WebSocket.priority());
    assert!(TransportProtocol::WebSocket.priority() > TransportProtocol::H2.priority());
}

#[test]
fn test_discovery_response_serialization() {
    let response = ProtocolDiscoveryResponse::default()
        .with_quic(4443)
        .with_websocket(443, "/localup")
        .with_h2(8443)
        .with_relay_id("relay-test-001");

    // Serialize
    let json = serde_json::to_string(&response).unwrap();

    // Verify JSON contains expected fields
    assert!(json.contains("\"quic\""));
    assert!(json.contains("\"websocket\""));
    assert!(json.contains("\"h2\""));
    assert!(json.contains("/localup"));
    assert!(json.contains("relay-test-001"));

    // Deserialize and verify
    let parsed: ProtocolDiscoveryResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.transports.len(), 3);
    assert_eq!(parsed.relay_id, Some("relay-test-001".to_string()));
}

#[test]
fn test_sorted_transports() {
    let response = ProtocolDiscoveryResponse::default()
        .with_h2(8443) // Added first (lowest priority)
        .with_websocket(443, "/ws") // Added second
        .with_quic(4443); // Added last (highest priority)

    let sorted = response.sorted_transports();

    // Should be sorted by priority: QUIC, WebSocket, H2
    assert_eq!(sorted.len(), 3);
    assert_eq!(sorted[0].protocol, TransportProtocol::Quic);
    assert_eq!(sorted[1].protocol, TransportProtocol::WebSocket);
    assert_eq!(sorted[2].protocol, TransportProtocol::H2);
}

#[test]
fn test_disabled_transports_filtered() {
    use localup_proto::TransportEndpoint;

    let mut response = ProtocolDiscoveryResponse::default().with_quic(4443);

    // Add a disabled WebSocket transport
    response = response.with_transport(TransportEndpoint {
        protocol: TransportProtocol::WebSocket,
        port: 443,
        path: Some("/ws".to_string()),
        enabled: false, // Disabled!
    });

    let sorted = response.sorted_transports();

    // Only QUIC should appear (WebSocket is disabled)
    assert_eq!(sorted.len(), 1);
    assert_eq!(sorted[0].protocol, TransportProtocol::Quic);
}

#[test]
fn test_find_transport() {
    let response = ProtocolDiscoveryResponse::default()
        .with_quic(4443)
        .with_websocket(443, "/localup")
        .with_h2(8443);

    // Find specific protocols
    let quic = response.find_transport(TransportProtocol::Quic);
    assert!(quic.is_some());
    assert_eq!(quic.unwrap().port, 4443);

    let ws = response.find_transport(TransportProtocol::WebSocket);
    assert!(ws.is_some());
    assert_eq!(ws.unwrap().port, 443);
    assert_eq!(ws.unwrap().path, Some("/localup".to_string()));

    let h2 = response.find_transport(TransportProtocol::H2);
    assert!(h2.is_some());
    assert_eq!(h2.unwrap().port, 8443);
}

#[test]
fn test_well_known_path_constant() {
    assert_eq!(WELL_KNOWN_PATH, "/.well-known/localup-protocols");
}

#[test]
fn test_discovered_transport_struct() {
    let discovered = DiscoveredTransport {
        protocol: TransportProtocol::WebSocket,
        address: "127.0.0.1:443".parse().unwrap(),
        path: Some("/localup".to_string()),
        full_response: None,
    };

    assert_eq!(discovered.protocol, TransportProtocol::WebSocket);
    assert_eq!(discovered.address.port(), 443);
    assert!(discovered.path.is_some());
    assert!(discovered.full_response.is_none());
}
