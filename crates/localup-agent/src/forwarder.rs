use localup_proto::TunnelMessage;
use localup_transport::{TransportError, TransportStream};
use localup_transport_quic::QuicStream;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Errors that can occur during TCP forwarding
#[derive(Error, Debug)]
pub enum ForwarderError {
    #[error("Failed to connect to remote address {address}: {source}")]
    ConnectionFailed {
        address: String,
        source: std::io::Error,
    },

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("IO error during forwarding: {0}")]
    Io(#[from] std::io::Error),

    #[error("Address not allowed: {0}")]
    AddressNotAllowed(String),
}

/// Manages TCP forwarding to remote addresses
#[derive(Default)]
pub struct TcpForwarder {}

impl TcpForwarder {
    /// Create a new TCP forwarder
    pub fn new() -> Self {
        Self {}
    }

    /// Forward traffic between a tunnel stream and a remote TCP address
    ///
    /// # Arguments
    /// * `localup_id` - The tunnel identifier
    /// * `stream_id` - The stream ID for this connection
    /// * `remote_address` - The remote address to connect to (IP:port format)
    /// * `localup_stream` - The QUIC stream from the relay
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn forward(
        &self,
        localup_id: String,
        stream_id: u32,
        remote_address: String,
        localup_stream: QuicStream,
    ) -> Result<(), ForwarderError> {
        tracing::info!(
            localup_id = %localup_id,
            stream_id = stream_id,
            remote_address = %remote_address,
            "Starting TCP forward"
        );

        // Connect to the remote address
        let remote_stream = TcpStream::connect(&remote_address).await.map_err(|e| {
            ForwarderError::ConnectionFailed {
                address: remote_address.clone(),
                source: e,
            }
        })?;

        tracing::debug!(
            localup_id = %localup_id,
            stream_id = stream_id,
            "Connected to remote address"
        );

        // Forward bidirectionally
        let (to_remote, to_tunnel) =
            Self::copy_bidirectional(localup_stream, remote_stream).await?;

        tracing::info!(
            localup_id = %localup_id,
            stream_id = stream_id,
            bytes_to_remote = to_remote,
            bytes_to_tunnel = to_tunnel,
            "TCP forward completed"
        );

        Ok(())
    }

    /// Copy data bidirectionally between tunnel stream and remote TCP stream
    ///
    /// This function handles the bidirectional data transfer between:
    /// - ReverseData messages from tunnel → TCP writes to remote
    /// - TCP reads from remote → ReverseData messages to tunnel
    ///
    /// Returns (bytes_to_remote, bytes_to_tunnel)
    async fn copy_bidirectional(
        localup_stream: QuicStream,
        mut remote_stream: TcpStream,
    ) -> Result<(u64, u64), ForwarderError> {
        // Split the QUIC stream
        let stream_id = localup_stream.stream_id();
        let (mut localup_send, mut localup_recv) = localup_stream.split();

        // Split the TCP stream
        let (mut remote_read, mut remote_write) = remote_stream.split();

        // Task 1: Read from tunnel (ReverseData messages) and write to remote TCP
        let localup_to_remote = async {
            let mut total_bytes = 0u64;
            loop {
                // Read message from tunnel
                match localup_recv.recv_message().await {
                    Ok(Some(TunnelMessage::ReverseData {
                        localup_id: _,
                        stream_id: _,
                        data,
                    })) => {
                        if data.is_empty() {
                            tracing::debug!("Received empty data, closing write side");
                            break;
                        }

                        // Write to remote
                        remote_write.write_all(&data).await?;
                        total_bytes += data.len() as u64;
                    }
                    Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                        tracing::debug!("Received ReverseClose, shutting down");
                        break;
                    }
                    Ok(None) => {
                        tracing::debug!("Tunnel stream closed");
                        break;
                    }
                    Ok(Some(msg)) => {
                        tracing::warn!("Unexpected message during forwarding: {:?}", msg);
                    }
                    Err(e) => {
                        tracing::error!("Error reading from tunnel: {}", e);
                        return Err(ForwarderError::Transport(e));
                    }
                }
            }

            // Shutdown write side of remote connection
            let _ = remote_write.shutdown().await;

            Ok::<u64, ForwarderError>(total_bytes)
        };

        // Task 2: Read from remote TCP and write to tunnel (ReverseData messages)
        let remote_to_tunnel = async {
            let mut total_bytes = 0u64;
            let mut buffer = vec![0u8; 16384]; // 16KB buffer

            loop {
                // Read from remote
                match remote_read.read(&mut buffer).await {
                    Ok(0) => {
                        tracing::debug!("Remote connection closed");
                        break;
                    }
                    Ok(n) => {
                        // Send data to tunnel as ReverseData message
                        // Note: localup_id and stream_id should match the ForwardRequest
                        let msg = TunnelMessage::ReverseData {
                            localup_id: String::new(), // Will be filled by relay
                            stream_id: stream_id as u32,
                            data: buffer[..n].to_vec(),
                        };

                        localup_send.send_message(&msg).await?;
                        total_bytes += n as u64;
                    }
                    Err(e) => {
                        tracing::error!("Error reading from remote: {}", e);
                        return Err(ForwarderError::Io(e));
                    }
                }
            }

            // Send close message
            let close_msg = TunnelMessage::ReverseClose {
                localup_id: String::new(),
                stream_id: stream_id as u32,
                reason: None,
            };
            let _ = localup_send.send_message(&close_msg).await;

            // Finish the tunnel stream
            let _ = localup_send.finish().await;

            Ok::<u64, ForwarderError>(total_bytes)
        };

        // Run both tasks concurrently
        let (result1, result2) = tokio::join!(localup_to_remote, remote_to_tunnel);

        let bytes_to_remote = result1?;
        let bytes_to_tunnel = result2?;

        Ok((bytes_to_remote, bytes_to_tunnel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forwarder_error_display() {
        let err = ForwarderError::AddressNotAllowed("192.168.1.1:8080".to_string());
        assert!(err.to_string().contains("not allowed"));
    }
}
