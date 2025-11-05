//! Tunnel Agent - Reverse proxy agent for forwarding traffic to a specific target address
//!
//! The tunnel agent connects to a relay server and forwards incoming requests
//! to a single specific target address (e.g., "192.168.1.100:8080").
//!
//! # Features
//!
//! - **Single Target Address**: Each agent forwards to exactly one address
//! - **TCP Forwarding**: Bidirectional TCP proxying to the target
//! - **Connection Management**: Track and manage active forwarding connections
//! - **Authentication**: JWT-based authentication with relay server
//! - **Secure**: Address validation ensures requests only go to the configured target
//!
//! # Example Usage
//!
//! ```no_run
//! use tunnel_agent::{Agent, AgentConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = AgentConfig {
//!         agent_id: "my-agent".to_string(),
//!         relay_addr: "relay.example.com:4443".to_string(),
//!         auth_token: "your-token".to_string(),
//!         target_address: "192.168.1.100:8080".to_string(),
//!         insecure: false,
//!     };
//!
//!     let mut agent = Agent::new(config)?;
//!     agent.start().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Architecture
//!
//! The agent operates in a client-server model:
//!
//! 1. **Connection**: Agent connects to the relay server via QUIC
//! 2. **Registration**: Authenticates using JWT token and declares target address
//! 3. **Message Loop**: Accepts incoming forwarding requests from relay
//! 4. **Validation**: Verifies that requested address matches the agent's target address
//! 5. **Forwarding**: Forwards traffic to the target address
//! 6. **Cleanup**: Manages connection lifecycle and cleanup

mod agent;
mod connection;
mod forwarder;

// Re-export public API
pub use agent::{Agent, AgentConfig, AgentError};
pub use connection::{ConnectionInfo, ConnectionManager};
pub use forwarder::{ForwarderError, TcpForwarder};
