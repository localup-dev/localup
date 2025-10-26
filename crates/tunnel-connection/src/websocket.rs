//! WebSocket transport implementation

use crate::transport::{Transport, TransportError};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, trace};

/// WebSocket configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub url: String,
    pub timeout_secs: u64,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_secs: 30,
        }
    }
}

/// WebSocket transport
pub struct WebSocketTransport {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    connected: bool,
}

impl WebSocketTransport {
    /// Connect to WebSocket server
    pub async fn connect(config: WebSocketConfig) -> Result<Self, TransportError> {
        debug!("Connecting to WebSocket: {}", config.url);

        let (ws_stream, _response) = connect_async(&config.url)
            .await
            .map_err(|e| TransportError::WebSocketError(e.to_string()))?;

        debug!("WebSocket connected");

        Ok(Self {
            stream: ws_stream,
            connected: true,
        })
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn send(&mut self, data: Bytes) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        trace!("Sending {} bytes via WebSocket", data.len());

        self.stream
            .send(Message::Binary(data.to_vec()))
            .await
            .map_err(|e| TransportError::WebSocketError(e.to_string()))?;

        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Bytes>, TransportError> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        match self.stream.next().await {
            Some(Ok(Message::Binary(data))) => {
                trace!("Received {} bytes via WebSocket", data.len());
                Ok(Some(Bytes::from(data)))
            }
            Some(Ok(Message::Close(_))) => {
                debug!("WebSocket closed by remote");
                self.connected = false;
                Ok(None)
            }
            Some(Ok(Message::Ping(data))) => {
                // Respond to ping with pong
                self.stream
                    .send(Message::Pong(data))
                    .await
                    .map_err(|e| TransportError::WebSocketError(e.to_string()))?;
                // Recursively wait for next message
                self.recv().await
            }
            Some(Ok(Message::Pong(_))) => {
                // Ignore pong messages
                self.recv().await
            }
            Some(Ok(msg)) => {
                // Ignore text and other message types
                debug!("Ignoring WebSocket message type: {:?}", msg);
                self.recv().await
            }
            Some(Err(e)) => {
                error!("WebSocket error: {}", e);
                self.connected = false;
                Err(TransportError::WebSocketError(e.to_string()))
            }
            None => {
                debug!("WebSocket stream ended");
                self.connected = false;
                Ok(None)
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        if !self.connected {
            return Ok(());
        }

        debug!("Closing WebSocket connection");

        self.stream
            .close(None)
            .await
            .map_err(|e| TransportError::WebSocketError(e.to_string()))?;

        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_websocket_config() {
        let config = WebSocketConfig {
            url: "ws://localhost:8080".to_string(),
            timeout_secs: 60,
        };

        assert_eq!(config.url, "ws://localhost:8080");
        assert_eq!(config.timeout_secs, 60);
    }
}
