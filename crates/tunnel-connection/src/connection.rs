//! QUIC connection implementation using quinn

use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, VarInt};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, trace};
use tunnel_proto::{TunnelCodec, TunnelMessage};

/// QUIC connection errors
#[derive(Debug, Error)]
pub enum QuicError {
    #[error("Connection error: {0}")]
    ConnectionError(#[from] quinn::ConnectionError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Stream write error: {0}")]
    WriteError(#[from] quinn::WriteError),

    #[error("Stream read error: {0}")]
    ReadError(#[from] quinn::ReadError),

    #[error("Stream read to end error: {0}")]
    ReadToEndError(#[from] quinn::ReadToEndError),

    #[error("Stream closed: {0}")]
    ClosedStream(#[from] quinn::ClosedStream),

    #[error("Codec error: {0}")]
    CodecError(#[from] tunnel_proto::CodecError),

    #[error("TLS configuration error: {0}")]
    TlsError(String),

    #[error("Connect error: {0}")]
    ConnectError(#[from] quinn::ConnectError),

    #[error("No streams available")]
    NoStreamsAvailable,

    #[error("Connection closed")]
    ConnectionClosed,
}

/// QUIC stream wrapper
pub struct QuicStream {
    send: SendStream,
    recv: RecvStream,
}

impl QuicStream {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        Self { send, recv }
    }

    /// Send raw bytes
    pub async fn send_bytes(&mut self, data: &[u8]) -> Result<(), QuicError> {
        self.send.write_all(data).await?;
        Ok(())
    }

    /// Receive raw bytes (reads all data until EOF or error)
    pub async fn recv_bytes(&mut self) -> Result<Vec<u8>, QuicError> {
        let data = self.recv.read_to_end(65536).await?;
        Ok(data)
    }

    /// Send a tunnel message
    pub async fn send_message(&mut self, msg: &TunnelMessage) -> Result<(), QuicError> {
        let encoded = TunnelCodec::encode(msg)?;
        self.send_bytes(&encoded).await?;
        Ok(())
    }

    /// Receive a tunnel message
    pub async fn recv_message(&mut self) -> Result<Option<TunnelMessage>, QuicError> {
        let data = self.recv_bytes().await?;

        if data.is_empty() {
            return Ok(None);
        }

        let mut bytes_mut = bytes::BytesMut::from(&data[..]);
        let msg = TunnelCodec::decode(&mut bytes_mut)?;

        Ok(msg)
    }

    /// Finish sending (close the send side)
    pub async fn finish(&mut self) -> Result<(), QuicError> {
        self.send.finish()?;
        Ok(())
    }

    /// Get stream ID
    pub fn id(&self) -> u64 {
        self.send.id().index()
    }
}

/// QUIC connection wrapper
pub struct QuicConnection {
    connection: Connection,
}

impl QuicConnection {
    /// Create from an existing quinn connection
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Connect to a QUIC server
    pub async fn connect(server_addr: SocketAddr, server_name: &str) -> Result<Self, QuicError> {
        debug!(
            "Connecting to QUIC server: {} ({})",
            server_name, server_addr
        );

        // Create client configuration using quinn's re-exported rustls
        let mut roots = quinn::rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let client_crypto = quinn::rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| QuicError::TlsError(e.to_string()))?,
        ));

        let mut transport_config = quinn::TransportConfig::default();
        transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
        client_config.transport_config(Arc::new(transport_config));

        // Create endpoint
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
        endpoint.set_default_client_config(client_config);

        // Connect
        let connection = endpoint.connect(server_addr, server_name)?.await?;

        debug!("QUIC connection established");

        Ok(Self { connection })
    }

    /// Open a new bidirectional stream
    pub async fn open_stream(&self) -> Result<QuicStream, QuicError> {
        let (send, recv) = self.connection.open_bi().await?;

        trace!("Opened bidirectional stream: {}", send.id().index());

        Ok(QuicStream::new(send, recv))
    }

    /// Accept an incoming bidirectional stream
    pub async fn accept_stream(&self) -> Result<QuicStream, QuicError> {
        match self.connection.accept_bi().await {
            Ok((send, recv)) => {
                trace!("Accepted bidirectional stream: {}", send.id().index());
                Ok(QuicStream::new(send, recv))
            }
            Err(e) => {
                error!("Error accepting stream: {}", e);
                Err(QuicError::ConnectionError(e))
            }
        }
    }

    /// Open a unidirectional stream (send only)
    pub async fn open_uni_stream(&self) -> Result<SendStream, QuicError> {
        let send = self.connection.open_uni().await?;
        trace!("Opened unidirectional stream: {}", send.id().index());
        Ok(send)
    }

    /// Accept an incoming unidirectional stream
    pub async fn accept_uni_stream(&self) -> Result<RecvStream, QuicError> {
        let recv = self.connection.accept_uni().await?;
        trace!("Accepted unidirectional stream");
        Ok(recv)
    }

    /// Get the underlying connection
    pub fn inner(&self) -> &Connection {
        &self.connection
    }

    /// Close the connection gracefully
    pub async fn close(&self, error_code: VarInt, reason: &[u8]) {
        self.connection.close(error_code, reason);
        debug!("QUIC connection closed");
    }

    /// Check if connection is closed
    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }

    /// Get remote address
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Get stable connection ID
    pub fn stable_id(&self) -> usize {
        self.connection.stable_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quic_stream_id() {
        // This is mostly a compilation test
        // Real tests would require a running QUIC server
    }

    #[tokio::test]
    async fn test_tunnel_message_encoding() {
        let msg = TunnelMessage::Ping { timestamp: 12345 };
        let encoded = TunnelCodec::encode(&msg).unwrap();
        assert!(!encoded.is_empty());
    }
}
