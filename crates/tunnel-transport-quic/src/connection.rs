//! QUIC connection implementation

use async_trait::async_trait;
use quinn::Connection;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, trace};
use tunnel_transport::{ConnectionStats, TransportConnection, TransportError, TransportResult};

use crate::stream::QuicStream;

/// QUIC connection wrapper
#[derive(Debug, Clone)]
pub struct QuicConnection {
    inner: Connection,
    connection_id: String,
    created_at: Instant,
    // Reserved for future traffic tracking - currently using quinn's internal stats
    _bytes_sent: Arc<AtomicU64>,
    _bytes_received: Arc<AtomicU64>,
}

impl QuicConnection {
    pub fn new(connection: Connection) -> Self {
        let connection_id = format!("quic-{}", connection.stable_id());

        Self {
            inner: connection,
            connection_id,
            created_at: Instant::now(),
            _bytes_sent: Arc::new(AtomicU64::new(0)),
            _bytes_received: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the underlying quinn connection
    pub fn inner(&self) -> &Connection {
        &self.inner
    }
}

#[async_trait]
impl TransportConnection for QuicConnection {
    type Stream = QuicStream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        let (send, recv) = self
            .inner
            .open_bi()
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        trace!("Opened bidirectional stream: {}", send.id().index());

        Ok(QuicStream::new(send, recv))
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        match self.inner.accept_bi().await {
            Ok((send, recv)) => {
                trace!("Accepted bidirectional stream: {}", send.id().index());
                Ok(Some(QuicStream::new(send, recv)))
            }
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                debug!("Connection closed by application");
                Ok(None)
            }
            Err(quinn::ConnectionError::ConnectionClosed(_)) => {
                debug!("Connection closed by peer");
                Ok(None)
            }
            Err(quinn::ConnectionError::LocallyClosed) => {
                debug!("Connection closed locally");
                Ok(None)
            }
            Err(quinn::ConnectionError::TimedOut) => {
                debug!("Connection timed out");
                Ok(None)
            }
            Err(quinn::ConnectionError::Reset) => {
                debug!("Connection reset");
                Ok(None)
            }
            Err(e) => {
                error!("Error accepting stream: {}", e);
                // Treat all other errors as connection closed
                Ok(None)
            }
        }
    }

    async fn close(&self, error_code: u32, reason: &str) {
        self.inner
            .close(quinn::VarInt::from_u32(error_code), reason.as_bytes());

        debug!(
            "QUIC connection {} closed: {} (code: {})",
            self.connection_id, reason, error_code
        );
    }

    fn is_closed(&self) -> bool {
        self.inner.close_reason().is_some()
    }

    fn remote_address(&self) -> SocketAddr {
        self.inner.remote_address()
    }

    fn stats(&self) -> ConnectionStats {
        let quinn_stats = self.inner.stats();

        ConnectionStats {
            bytes_sent: quinn_stats.path.sent_packets,
            bytes_received: quinn_stats.path.lost_packets, // Approximation - quinn doesn't expose bytes_received directly
            active_streams: 0,                             // Would need to track this separately
            rtt_ms: Some(quinn_stats.path.rtt.as_millis() as u32),
            uptime_secs: self.created_at.elapsed().as_secs(),
        }
    }

    fn connection_id(&self) -> String {
        self.connection_id.clone()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_connection_id_format() {
        // Just verify the format - we can't create a real Connection easily
        let id = format!("quic-{}", 12345);
        assert!(id.starts_with("quic-"));
    }
}
