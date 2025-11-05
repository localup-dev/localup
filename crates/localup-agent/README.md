# LocalUp Agent

The LocalUp Agent is a command-line tool for running a reverse tunnel agent that forwards connections from a relay server to private network addresses.

## What the Agent Does

The LocalUp Agent acts as a bridge between a public relay server and your private network:

1. **Connects to Relay**: Establishes a secure QUIC connection to the relay server
2. **Authenticates**: Uses JWT tokens for secure authentication
3. **Forwards Traffic**: Receives forwarding requests from the relay and forwards them to allowed destinations in your private network
4. **Enforces Security**: Only forwards to explicitly allowed networks and ports

This enables secure access to services in private networks without exposing them directly to the internet.

## Use Cases

- Access databases (PostgreSQL, MySQL) in private networks
- Connect to internal APIs and services
- Provide temporary access to development environments
- Create secure tunnels to cloud VPCs or on-premises infrastructure

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/your-org/localup-dev.git
cd localup-dev

# Build the agent
cargo build --release -p localup-agent

# The binary will be at target/release/localup-agent
```

### Using Cargo Install

```bash
cargo install --path crates/localup-agent
```

## Usage

### Basic Usage

```bash
localup-agent \
  --relay relay.example.com:4443 \
  --auth-token your-jwt-token \
  --allow-network 192.168.0.0/16 \
  --allow-port 5432,3306
```

### Using Configuration File

Create a `agent-config.yaml` file:

```yaml
relay:
  address: relay.example.com:4443
  auth_token_env: LOCALUP_AUTH_TOKEN

agent:
  id: my-agent-1

security:
  allowed_networks:
    - 192.168.0.0/16
    - 10.0.0.0/8
  allowed_ports:
    - 5432
    - 3306
    - 8080
```

Then run:

```bash
export LOCALUP_AUTH_TOKEN="your-jwt-token"
localup-agent --config agent-config.yaml
```

### Environment Variables

You can configure the agent using environment variables:

```bash
export LOCALUP_RELAY="relay.example.com:4443"
export LOCALUP_AUTH_TOKEN="your-jwt-token"
export LOCALUP_AGENT_ID="my-agent-1"

localup-agent \
  --allow-network 192.168.0.0/16 \
  --allow-port 5432
```

### Command-Line Options

```
Options:
  --relay <RELAY>
          Relay server address (e.g., relay.example.com:4443)
          [env: LOCALUP_RELAY]

  --auth-token <AUTH_TOKEN>
          Authentication token (JWT)
          [env: LOCALUP_AUTH_TOKEN]

  --allow-network <ALLOWED_NETWORKS>
          Allowed networks in CIDR notation (can specify multiple, comma-separated)

  --allow-port <ALLOWED_PORTS>
          Allowed ports (can specify multiple, comma-separated)

  --agent-id <AGENT_ID>
          Agent ID (auto-generated if not specified)
          [env: LOCALUP_AGENT_ID]

  -c, --config <CONFIG>
          Configuration file (YAML)

  --log-level <LOG_LEVEL>
          Log level (trace, debug, info, warn, error)
          [default: info]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Configuration File Format

The configuration file uses YAML format:

```yaml
# Relay server configuration
relay:
  # Relay server address (required)
  address: relay.example.com:4443

  # Auth token from environment variable (recommended)
  auth_token_env: LOCALUP_AUTH_TOKEN

  # OR: Direct auth token (not recommended for production)
  # auth_token: your-token-here

# Agent configuration
agent:
  # Agent ID (optional, auto-generated if not specified)
  id: my-agent-1

# Security configuration
security:
  # Allowed networks in CIDR notation (required)
  allowed_networks:
    - 192.168.0.0/16    # Private network A
    - 10.0.0.0/8        # Private network B
    - 172.16.0.0/12     # Private network C

  # Allowed ports (required)
  allowed_ports:
    - 5432              # PostgreSQL
    - 3306              # MySQL
    - 8080              # HTTP
    - 443               # HTTPS
```

## Security Considerations

### Network Allowlist

The agent enforces a **strict allowlist** for destination networks:

- Only destinations in explicitly allowed CIDR ranges can be reached
- Attempts to forward to non-allowed networks are rejected
- Use the most restrictive CIDR ranges possible

### Port Allowlist

The agent enforces a **strict allowlist** for destination ports:

- Only explicitly allowed ports can be forwarded to
- Attempts to forward to non-allowed ports are rejected
- Only allow ports that are absolutely necessary

### Authentication

- The agent uses **JWT tokens** for authentication with the relay server
- Store tokens securely (use environment variables or secrets management)
- Never commit tokens to version control
- Use the `auth_token_env` configuration option to reference environment variables

### Best Practices

1. **Principle of Least Privilege**: Only allow the minimum necessary networks and ports
2. **Rotate Tokens**: Regularly rotate authentication tokens
3. **Monitor Logs**: Enable debug logging to monitor forwarding activity
4. **Network Segmentation**: Run agents in isolated network segments when possible
5. **Secure Config Files**: Set appropriate file permissions on configuration files (chmod 600)

## Logging

The agent uses structured logging with configurable log levels:

```bash
# Info level (default)
localup-agent --config agent-config.yaml

# Debug level (verbose)
localup-agent --config agent-config.yaml --log-level debug

# Trace level (very verbose)
localup-agent --config agent-config.yaml --log-level trace

# Warning level (minimal)
localup-agent --config agent-config.yaml --log-level warn
```

Log output format:
```
[2024-11-05T12:34:56Z INFO  localup_agent] LocalUp Agent starting...
[2024-11-05T12:34:56Z INFO  localup_agent] Agent ID: agent-abc123
[2024-11-05T12:34:56Z INFO  localup_agent] Relay: relay.example.com:4443
[2024-11-05T12:34:56Z INFO  localup_agent] Allowed networks: ["192.168.0.0/16"]
[2024-11-05T12:34:56Z INFO  localup_agent] Allowed ports: [5432, 3306]
```

## Examples

### Access PostgreSQL in Private Network

```bash
localup-agent \
  --relay relay.example.com:4443 \
  --auth-token $LOCALUP_TOKEN \
  --allow-network 192.168.1.0/24 \
  --allow-port 5432
```

### Access Multiple Services

```yaml
# multi-service.yaml
relay:
  address: relay.example.com:4443
  auth_token_env: LOCALUP_AUTH_TOKEN

security:
  allowed_networks:
    - 192.168.0.0/16
  allowed_ports:
    - 5432  # PostgreSQL
    - 3306  # MySQL
    - 6379  # Redis
    - 8080  # Application
```

```bash
localup-agent --config multi-service.yaml
```

### Development Setup

```bash
# Allow access to common development ports
localup-agent \
  --relay localhost:4443 \
  --auth-token dev-token \
  --allow-network 127.0.0.0/8 \
  --allow-port 3000,5432,6379,8080 \
  --log-level debug
```

## Troubleshooting

### Connection Refused

If you see connection refused errors:

1. Verify the relay address is correct
2. Check network connectivity to the relay server
3. Ensure the relay server is running and accessible

### Authentication Failed

If authentication fails:

1. Verify the auth token is valid and not expired
2. Check the token has the correct permissions
3. Ensure the token is correctly set in the environment or config file

### Forwarding Rejected

If forwarding is rejected:

1. Verify the destination is in an allowed network (CIDR)
2. Verify the destination port is in the allowed ports list
3. Enable debug logging to see detailed rejection reasons

### High CPU/Memory Usage

If the agent consumes excessive resources:

1. Check the number of active connections
2. Review the allowed networks (overly broad ranges can cause issues)
3. Enable debug logging to identify problematic connections

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
