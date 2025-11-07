//! Tests for transport abstraction layer

use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use localup_proto::TunnelMessage;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Mock transport stream for testing
#[derive(Debug)]
pub struct MockStream {
    id: u64,
    closed: bool,
    messages: Arc<Mutex<Vec<TunnelMessage>>>,
}

impl MockStream {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            closed: false,
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_messages(&self) -> Vec<TunnelMessage> {
        self.messages.lock().await.clone()
    }
}

#[async_trait]
impl TransportStream for MockStream {
    async fn send_message(&mut self, message: &TunnelMessage) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }
        self.messages.lock().await.push(message.clone());
        Ok(())
    }

    async fn recv_message(&mut self) -> TransportResult<Option<TunnelMessage>> {
        if self.closed {
            return Ok(None);
        }
        let mut msgs = self.messages.lock().await;
        Ok(msgs.pop())
    }

    async fn send_bytes(&mut self, _data: &[u8]) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }
        Ok(())
    }

    async fn recv_bytes(&mut self, _max_size: usize) -> TransportResult<Bytes> {
        if self.closed {
            return Ok(Bytes::new());
        }
        Ok(Bytes::from("test data"))
    }

    async fn finish(&mut self) -> TransportResult<()> {
        self.closed = true;
        Ok(())
    }

    fn stream_id(&self) -> u64 {
        self.id
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

/// Mock connection for testing
#[derive(Debug, Clone)]
pub struct MockConnection {
    remote_addr: SocketAddr,
    closed: Arc<Mutex<bool>>,
    next_stream_id: Arc<Mutex<u64>>,
}

impl MockConnection {
    pub fn new(remote_addr: SocketAddr) -> Self {
        Self {
            remote_addr,
            closed: Arc::new(Mutex::new(false)),
            next_stream_id: Arc::new(Mutex::new(1)),
        }
    }
}

#[async_trait]
impl TransportConnection for MockConnection {
    type Stream = MockStream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        let mut id = self.next_stream_id.lock().await;
        let stream = MockStream::new(*id);
        *id += 1;
        Ok(stream)
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        if *self.closed.lock().await {
            return Ok(None);
        }
        let mut id = self.next_stream_id.lock().await;
        let stream = MockStream::new(*id);
        *id += 1;
        Ok(Some(stream))
    }

    async fn close(&self, _error_code: u32, _reason: &str) {
        *self.closed.lock().await = true;
    }

    fn is_closed(&self) -> bool {
        false // Would need to check async mutex
    }

    fn remote_address(&self) -> SocketAddr {
        self.remote_addr
    }

    fn stats(&self) -> ConnectionStats {
        ConnectionStats {
            bytes_sent: 1024,
            bytes_received: 2048,
            active_streams: 5,
            rtt_ms: Some(10),
            uptime_secs: 60,
        }
    }

    fn connection_id(&self) -> String {
        format!("mock-{}", self.remote_addr)
    }
}

#[cfg(test)]
mod test_mock {
    use super::*;
    use localup_proto::Protocol;

    #[tokio::test]
    async fn test_mock_stream_send_receive() {
        let mut stream = MockStream::new(1);

        let msg = TunnelMessage::Ping { timestamp: 12345 };
        stream.send_message(&msg).await.unwrap();

        let messages = stream.get_messages().await;
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0],
            TunnelMessage::Ping { timestamp: 12345 }
        ));
    }

    #[tokio::test]
    async fn test_mock_stream_close() {
        let mut stream = MockStream::new(1);
        assert!(!stream.is_closed());

        stream.finish().await.unwrap();
        assert!(stream.is_closed());

        // Should fail after close
        let result = stream
            .send_message(&TunnelMessage::Ping { timestamp: 0 })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_stream_id() {
        let stream = MockStream::new(42);
        assert_eq!(stream.stream_id(), 42);
    }

    #[tokio::test]
    async fn test_mock_connection_open_stream() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = MockConnection::new(addr);

        let stream1 = connection.open_stream().await.unwrap();
        let stream2 = connection.open_stream().await.unwrap();

        assert_eq!(stream1.stream_id(), 1);
        assert_eq!(stream2.stream_id(), 2);
    }

    #[tokio::test]
    async fn test_mock_connection_accept_stream() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = MockConnection::new(addr);

        let stream = connection.accept_stream().await.unwrap();
        assert!(stream.is_some());
        assert_eq!(stream.unwrap().stream_id(), 1);
    }

    #[tokio::test]
    async fn test_mock_connection_stats() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let connection = MockConnection::new(addr);

        let stats = connection.stats();
        assert_eq!(stats.bytes_sent, 1024);
        assert_eq!(stats.bytes_received, 2048);
        assert_eq!(stats.active_streams, 5);
        assert_eq!(stats.rtt_ms, Some(10));
        assert_eq!(stats.uptime_secs, 60);
    }

    #[tokio::test]
    async fn test_mock_connection_remote_address() {
        let addr: SocketAddr = "192.168.1.100:9090".parse().unwrap();
        let connection = MockConnection::new(addr);

        assert_eq!(connection.remote_address(), addr);
    }

    #[tokio::test]
    async fn test_mock_connection_id() {
        let addr: SocketAddr = "10.0.0.1:4433".parse().unwrap();
        let connection = MockConnection::new(addr);

        let id = connection.connection_id();
        assert!(id.contains("mock"));
        assert!(id.contains("10.0.0.1:4433"));
    }

    #[tokio::test]
    async fn test_connection_stats_default() {
        let stats = ConnectionStats::default();
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.active_streams, 0);
        assert_eq!(stats.rtt_ms, None);
    }

    #[tokio::test]
    async fn test_transport_security_config_default() {
        let config = TransportSecurityConfig::default();
        assert!(config.verify_server_cert);
        assert!(config.client_cert.is_none());
        assert_eq!(config.alpn_protocols, vec!["localup-v1"]);
    }

    #[tokio::test]
    async fn test_client_certificate() {
        let cert = ClientCertificate {
            cert_chain: vec![1, 2, 3],
            private_key: vec![4, 5, 6],
        };

        assert_eq!(cert.cert_chain, vec![1, 2, 3]);
        assert_eq!(cert.private_key, vec![4, 5, 6]);
    }

    #[tokio::test]
    async fn test_transport_errors() {
        let err = TransportError::ConnectionError("test".to_string());
        assert!(err.to_string().contains("Connection error"));

        let err = TransportError::StreamClosed;
        assert!(err.to_string().contains("Stream closed"));

        let err = TransportError::Timeout;
        assert!(err.to_string().contains("Timeout"));
    }

    #[tokio::test]
    async fn test_message_exchange() {
        let mut stream = MockStream::new(1);

        // Send Connect message
        let connect_msg = TunnelMessage::Connect {
            localup_id: "test-tunnel".to_string(),
            auth_token: "token123".to_string(),
            protocols: vec![Protocol::Http {
                subdomain: Some("test".to_string()),
            }],
            config: Default::default(),
        };

        stream.send_message(&connect_msg).await.unwrap();

        // Verify it was stored
        let messages = stream.get_messages().await;
        assert_eq!(messages.len(), 1);
        if let TunnelMessage::Connect { localup_id, .. } = &messages[0] {
            assert_eq!(localup_id, "test-tunnel");
        } else {
            panic!("Expected Connect message");
        }
    }
}
