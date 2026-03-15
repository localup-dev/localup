# Tunnel Security

localup provides multiple layers of security to protect your tunnels. This guide covers JWT authentication, HTTP authentication, and IP allowlisting.

---

## JWT Authentication (Relay Access)

JWT tokens control who can create tunnels on a relay server. Every tunnel connection requires a valid token signed with the relay's secret.

### Generating Tokens

```bash
# Basic token (24-hour validity)
localup generate-token --secret "my-relay-secret" --sub myapp

# Custom validity period
localup generate-token --secret "my-relay-secret" --sub myapp --hours 48

# Restrict to specific subdomains
localup generate-token --secret "my-relay-secret" --sub myapp \
  --allowed-subdomain "myapp" \
  --allowed-subdomain "myapp-*"

# Token for reverse tunnel access
localup generate-token --secret "my-relay-secret" --sub myapp \
  --reverse-tunnel \
  --agent my-agent-id

# Script-friendly output (token only)
TOKEN=$(localup generate-token --secret "my-relay-secret" --sub myapp --token-only)
```

### Using Tokens

```bash
# Via CLI flag
localup -p 3000 -r relay.example.com:4443 -t "$TOKEN"

# Via environment variable
export TUNNEL_AUTH_TOKEN="$TOKEN"
localup -p 3000 -r relay.example.com:4443

# Via stored config
localup config set-token "$TOKEN"
localup -p 3000 -r relay.example.com:4443
```

### Relay-Side Setup

The relay validates tokens using the same secret:

```bash
localup relay http \
  --jwt-secret "my-relay-secret" \
  --http-addr 0.0.0.0:80
```

The secret must match exactly between token generation and relay validation.

### Token Claims

Tokens can restrict what the bearer is allowed to do:

| Claim | Purpose |
|-------|---------|
| `sub` | Tunnel identifier |
| `exp` | Expiration timestamp |
| `allowed-subdomain` | Glob patterns for allowed subdomains |
| `reverse-tunnel` | Whether reverse tunnel access is permitted |
| `agent` | Which agent IDs the token can access |
| `allowed-address` | Which target addresses are permitted |

---

## HTTP Authentication (Tunnel Access)

Protect your tunnel endpoints so that only authorized users can access them. This is independent of JWT authentication -- JWT controls who creates the tunnel, HTTP auth controls who accesses it.

### Basic Authentication

Add username/password protection to your tunnel:

```bash
# Single user
localup -p 3000 --basic-auth "admin:secretpassword" -r relay.example.com:4443 -t $TOKEN

# Multiple users
localup -p 3000 \
  --basic-auth "admin:adminpass" \
  --basic-auth "developer:devpass" \
  --basic-auth "viewer:viewpass" \
  -r relay.example.com:4443 -t $TOKEN
```

When enabled, visitors to your tunnel URL will see a browser login prompt. Requests without valid credentials receive a `401 Unauthorized` response.

### Bearer Token Authentication

Protect your tunnel with bearer tokens (useful for API endpoints):

```bash
# Single token
localup -p 3000 --auth-token "my-api-key-123" -r relay.example.com:4443 -t $TOKEN

# Multiple tokens
localup -p 3000 \
  --auth-token "production-key" \
  --auth-token "staging-key" \
  -r relay.example.com:4443 -t $TOKEN
```

Clients must include the `Authorization: Bearer <token>` header:

```bash
curl -H "Authorization: Bearer my-api-key-123" https://myapp.relay.example.com/api/data
```

### Combining Auth Methods

You can combine Basic Auth, Bearer tokens, and IP allowlisting. A request passes if it satisfies **any** of the configured auth methods:

```bash
localup -p 3000 \
  --basic-auth "admin:secret" \
  --auth-token "api-key-123" \
  --allow-ip 10.0.0.0/8 \
  -r relay.example.com:4443 -t $TOKEN
```

---

## IP Allowlisting

Restrict tunnel access to specific IP addresses or CIDR ranges.

### Usage

```bash
# Single IP
localup -p 3000 --allow-ip 203.0.113.50 -r relay.example.com:4443 -t $TOKEN

# CIDR range
localup -p 3000 --allow-ip 10.0.0.0/8 -r relay.example.com:4443 -t $TOKEN

# Multiple rules
localup -p 3000 \
  --allow-ip 192.168.1.0/24 \
  --allow-ip 10.0.0.0/8 \
  --allow-ip 203.0.113.50 \
  -r relay.example.com:4443 -t $TOKEN
```

### In Configuration File

```yaml
tunnels:
  - name: internal-api
    port: 8080
    protocol: http
    subdomain: internal
    allow_ips:
      - 10.0.0.0/8          # Private network
      - 192.168.0.0/16      # Private network
      - 172.16.0.0/12       # Private network
      - 203.0.113.50        # Office public IP
```

Requests from IPs not in the allowlist receive a `403 Forbidden` response.

---

## Transport Security

All tunnel traffic is encrypted regardless of the transport protocol:

| Transport | Encryption | Port |
|-----------|-----------|------|
| QUIC | TLS 1.3 (built-in) | 4443/UDP |
| WebSocket | TLS 1.2/1.3 (wss://) | 443/TCP |
| HTTP/2 | TLS 1.2/1.3 (h2) | 443/TCP |

### TLS Certificate Verification

By default, clients verify the relay's TLS certificate. For development with self-signed certificates:

```bash
# Skip verification (development only!)
localup -p 3000 -r localhost:4443 --insecure
```

Never use `--insecure` in production.

---

## Security Best Practices

1. **Use strong JWT secrets** - At least 32 characters, random. Never commit secrets to git.

2. **Set short token expiry** - Use the shortest practical validity period.

3. **Use HTTPS tunnels** when exposing web services - This provides TLS termination at the relay, encrypting traffic between external clients and the relay.

4. **Combine security layers** - Use JWT + HTTP Auth + IP allowlisting together for defense in depth.

5. **Use environment variables for secrets**:
   ```bash
   export TUNNEL_AUTH_TOKEN="..."
   export JWT_SECRET="..."
   ```

6. **Rotate tokens regularly** - Generate new tokens and update clients periodically.

7. **Use subdomain restrictions** in JWT claims to prevent token misuse:
   ```bash
   localup generate-token --secret "$SECRET" --sub myapp --allowed-subdomain "myapp"
   ```

8. **Monitor tunnel access** - Use the REST API or dashboard to review captured requests and connections.
