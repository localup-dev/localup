# Reverse Tunnels

Reverse tunnels let you access private services (behind firewalls, NAT, or on private networks) through a relay server. Unlike standard tunnels that expose a local port to the internet, reverse tunnels let you reach into a remote network.

---

## How It Works

```
You (Client)  -->  Relay Server  -->  Agent  -->  Private Service
  localup           localup           localup       192.168.1.100:5432
  connect            relay             agent
```

1. An **Agent** runs on the private network, connected to the relay
2. A **Client** connects to the relay and requests access to the agent
3. The relay bridges the connection between client and agent
4. The agent forwards traffic to the private service

---

## Quick Start

### Step 1: Start a Relay

```bash
localup relay http \
  --localup-addr 0.0.0.0:4443 \
  --http-addr 0.0.0.0:8080 \
  --jwt-secret "my-secret"
```

### Step 2: Generate Tokens

```bash
# Token for the agent
AGENT_TOKEN=$(localup generate-token --secret "my-secret" --sub agent-1 --token-only)

# Token for the client (with reverse tunnel access)
CLIENT_TOKEN=$(localup generate-token --secret "my-secret" --sub client-1 \
  --reverse-tunnel --agent private-db --token-only)
```

### Step 3: Run the Agent (on the private network)

```bash
localup agent \
  --relay relay.example.com:4443 \
  --token "$AGENT_TOKEN" \
  --target-address 192.168.1.100:5432 \
  --agent-id private-db
```

The agent connects to the relay and registers with ID `private-db`. It will forward all traffic to `192.168.1.100:5432`.

### Step 4: Connect (from anywhere)

```bash
localup connect \
  --relay relay.example.com:4443 \
  --token "$CLIENT_TOKEN" \
  --agent-id private-db \
  --remote-address 192.168.1.100:5432 \
  --local-address localhost:15432
```

Now you can access the private database at `localhost:15432`:

```bash
psql -h localhost -p 15432 -U myuser mydb
```

---

## Use Cases

### Access a Private Database

```bash
# Agent (on database network)
localup agent \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --target-address db-server:5432 \
  --agent-id prod-db

# Client (your laptop)
localup connect \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --agent-id prod-db \
  --remote-address db-server:5432 \
  --local-address localhost:15432
```

### SSH into a Private Server

```bash
# Agent (on the private network)
localup agent \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --target-address 10.0.1.50:22 \
  --agent-id office-server

# Client
localup connect \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --agent-id office-server \
  --remote-address 10.0.1.50:22 \
  --local-address localhost:2222

# Then SSH through the tunnel
ssh -p 2222 user@localhost
```

### Access an Internal Web App

```bash
# Agent (on internal network)
localup agent \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --target-address internal-app.corp:80 \
  --agent-id intranet

# Client
localup connect \
  --relay relay.example.com:4443 \
  --token "$TOKEN" \
  --agent-id intranet \
  --remote-address internal-app.corp:80 \
  --local-address localhost:18080

# Access at http://localhost:18080
```

---

## Agent Server (Standalone)

For simpler setups where you don't need a separate relay, use `agent-server`. It combines relay and agent functionality in a single process.

```bash
# On the private network (acts as both relay and agent)
localup agent-server \
  --listen 0.0.0.0:4443 \
  --jwt-secret "my-secret" \
  --target-address 192.168.1.100:8080

# Connect from anywhere
localup connect \
  --relay agent-host.example.com:4443 \
  --remote-address 192.168.1.100:8080 \
  --local-address localhost:18080 \
  --token "$TOKEN"
```

### Agent Server with Upstream Relay

The agent server can also register with an upstream relay for public accessibility:

```bash
localup agent-server \
  --listen 0.0.0.0:4443 \
  --target-address 192.168.1.100:8080 \
  --relay-addr relay.example.com:4443 \
  --relay-id my-agent-server \
  --relay-token "$RELAY_TOKEN"
```

---

## Security

### Token Restrictions

Use JWT claims to control what clients can access:

```bash
# Allow access to specific agent only
localup generate-token --secret "my-secret" --sub client-1 \
  --reverse-tunnel \
  --agent private-db

# Allow access to specific addresses only
localup generate-token --secret "my-secret" --sub client-1 \
  --reverse-tunnel \
  --allowed-address "192.168.1.100:5432" \
  --allowed-address "192.168.1.101:5432"
```

### Agent-Side Authentication

The agent can validate client tokens independently:

```bash
localup agent \
  --relay relay.example.com:4443 \
  --token "$AGENT_TOKEN" \
  --target-address 192.168.1.100:5432 \
  --agent-id private-db \
  --jwt-secret "agent-secret"    # Validates client tokens
```

### TLS Verification

In production, always use valid TLS certificates. For development:

```bash
# Skip TLS verification (development only!)
localup agent --insecure ...
localup connect --insecure ...
```

---

## Environment Variables

All agent and connect flags can be set via environment variables:

```bash
# Agent
export LOCALUP_RELAY_ADDR="relay.example.com:4443"
export LOCALUP_AUTH_TOKEN="agent-token"
export LOCALUP_TARGET_ADDRESS="192.168.1.100:5432"
export LOCALUP_AGENT_ID="private-db"
localup agent

# Connect
export LOCALUP_RELAY="relay.example.com:4443"
export LOCALUP_AUTH_TOKEN="client-token"
localup connect --agent-id private-db --remote-address 192.168.1.100:5432
```

---

## Troubleshooting

### Agent won't connect to relay

```bash
# Enable debug logging
localup agent --log-level debug ...

# Check TLS - try with insecure for testing
localup agent --insecure ...
```

Verify the agent token is signed with the relay's JWT secret.

### Client can't reach agent

1. Verify the agent is connected: check relay logs or `localup status`
2. Verify the agent ID matches between `--agent-id` flags
3. Verify the client token has `--reverse-tunnel` permission
4. Check that `--remote-address` matches the agent's `--target-address`

### Connection drops

The agent and client automatically attempt reconnection. If connections are unstable:

- Check network quality between agent and relay
- Try a different transport (`--transport websocket` for firewalled networks)
- Increase timeout in `.localup.yml` (`timeout_seconds: 60`)
