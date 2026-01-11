# localup-agent

A reverse proxy agent that connects to a relay server and forwards incoming requests to remote addresses with configurable network and port allowlists.

## Overview

The `localup-agent` crate provides a client-side agent that:

- Connects to a tunnel relay server
- Accepts forwarding requests from the relay
- Validates destination addresses against allowlists (CIDR networks and ports)
- Forwards TCP traffic bidirectionally to remote addresses
- Manages connection lifecycle and cleanup

## Features

- **CIDR-based Network Allowlist**: Control which IP networks can be accessed
- **Port-based Filtering**: Restrict forwarding to specific TCP ports
- **Bidirectional TCP Forwarding**: Full-duplex proxying between tunnel and remote address
- **Connection Tracking**: Monitor active connections and their status
- **Graceful Shutdown**: Clean connection cleanup on agent stop

## Use Cases

The agent is designed for scenarios where you want to:

1. **Expose internal services** to external users through a relay
2. **Control access** to specific networks/ports for security
3. **Audit connections** to remote addresses
4. **Route traffic** through a centralized relay point

## Example Usage

```rust
use localup_agent::{Agent, AgentConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the agent
    let config = AgentConfig {
        agent_id: "my-agent".to_string(),
        relay_addr: "relay.example.com:4443".to_string(),
        auth_token: "your-auth-token".to_string(),

        // Only allow forwarding to private networks
        allowed_networks: vec![
            "192.168.0.0/16".to_string(),
            "10.0.0.0/8".to_string(),
        ],

        // Only allow specific ports
        allowed_ports: vec![8080, 3000, 5000],
    };

    // Create and start the agent
    let mut agent = Agent::new(config)?;

    println!("Agent starting...");
    agent.start().await?;

    Ok(())
}
```

## Configuration

### AgentConfig

- **agent_id**: Unique identifier for this agent instance
- **relay_addr**: Address of the relay server (host:port)
- **auth_token**: JWT authentication token for the relay
- **allowed_networks**: List of CIDR network ranges (empty = allow all)
- **allowed_ports**: List of allowed TCP ports (empty = allow all)

### Allowlist Behavior

- If `allowed_networks` is **empty**, all networks are allowed
- If `allowed_ports` is **empty**, all ports are allowed
- An address must match **both** network and port to be allowed

## Architecture

```
┌──────────────┐         QUIC          ┌──────────────┐
│    Relay     │◄──────────────────────►│    Agent     │
│   Server     │                        │              │
└──────────────┘                        └──────┬───────┘
                                               │
                                               │ TCP Forward
                                               │
                                               ▼
                                        ┌──────────────┐
                                        │   Remote     │
                                        │   Address    │
                                        └──────────────┘
```

### Flow

1. **Connection**: Agent establishes QUIC connection to relay
2. **Registration**: Sends authentication token and receives confirmation
3. **Message Loop**: Waits for forwarding requests from relay
4. **Validation**: Checks each request against allowlist
5. **Forwarding**: Proxies traffic bidirectionally if allowed
6. **Cleanup**: Unregisters connection when complete

## Development

### Building

```bash
cargo build -p localup-agent
```

### Testing

```bash
cargo test -p localup-agent
```

### Linting

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Security Considerations

- **Authentication**: Always use a strong JWT token for relay authentication
- **Allowlists**: Configure restrictive allowlists to limit exposure
- **Network Isolation**: Consider running agent in isolated network segment
- **Monitoring**: Track active connections for unusual activity
- **TLS**: Relay connections use QUIC with built-in TLS 1.3

## Error Handling

The agent uses `thiserror` for structured error types:

- **InvalidAllowlist**: CIDR parsing or configuration errors
- **Transport**: QUIC/network errors
- **Forwarder**: TCP forwarding errors
- **RegistrationFailed**: Authentication or registration errors
- **MessageHandling**: Protocol message errors

## Dependencies

- `localup-proto`: Protocol definitions
- `localup-transport`: QUIC transport layer
- `tokio`: Async runtime
- `ipnetwork`: CIDR parsing and IP matching
- `tracing`: Structured logging
- `thiserror`: Error handling

## License

See workspace LICENSE file.
