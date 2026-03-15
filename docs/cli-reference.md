# CLI Reference

Complete reference for all `localup` commands, flags, and environment variables.

## Quick Overview

```
localup [OPTIONS]                    # Run a single tunnel
localup init                         # Create .localup.yml config
localup up [--tunnels name1,name2]   # Start tunnels from config
localup down                         # Stop tunnels from config
localup status                       # Show tunnel status
localup relay <tcp|tls|http>         # Run a relay server
localup agent                        # Run a reverse tunnel agent
localup agent-server                 # Run a standalone agent server
localup connect                      # Connect through a reverse tunnel
localup generate-token               # Generate JWT tokens
localup daemon <subcommand>          # Manage the daemon
localup service <subcommand>         # Manage the system service
localup config <subcommand>          # Manage stored configuration
```

---

## Standalone Tunnel

Run a single tunnel without a configuration file.

```bash
localup [OPTIONS]
```

### Connection Options

| Flag | Short | Env Var | Default | Description |
|------|-------|---------|---------|-------------|
| `--port <PORT>` | `-p` | | | Local port to expose |
| `--address <HOST:PORT>` | | | | Local address to expose (alternative to `--port`) |
| `--protocol <PROTO>` | | | `http` | Protocol: `http`, `https`, `tcp`, `tls` |
| `--relay <ADDRESS>` | `-r` | `RELAY` | | Relay server address |
| `--token <TOKEN>` | `-t` | `TUNNEL_AUTH_TOKEN` | | Authentication token |
| `--transport <PROTO>` | | | `quic` | Transport: `quic`, `h2`, `websocket` |
| `--subdomain <NAME>` | `-s` | | | Subdomain for HTTP/HTTPS tunnels |
| `--custom-domain <DOMAIN>` | | | | Custom domain (repeatable, supports wildcards) |
| `--remote-port <PORT>` | | | | Remote port for TCP/TLS tunnels |
| `--http-port <PORT>` | | | | HTTP backend port for TLS passthrough |

### Security Options

| Flag | Description |
|------|-------------|
| `--basic-auth <USER:PASS>` | HTTP Basic Auth credentials (repeatable) |
| `--auth-token <TOKEN>` | HTTP Bearer token for tunnel access (repeatable) |
| `--allow-ip <IP_OR_CIDR>` | Allowed IP addresses/ranges (repeatable) |

### Other Options

| Flag | Default | Description |
|------|---------|-------------|
| `--log-level <LEVEL>` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `--metrics-port <PORT>` | `9090` | Metrics web dashboard port |
| `--no-metrics` | | Disable metrics collection |

### Examples

```bash
# HTTP tunnel
localup -p 3000 -r relay.example.com:4443 -t $TOKEN

# HTTPS tunnel with subdomain
localup -p 3000 --protocol https -s myapp -r relay.example.com:4443 -t $TOKEN

# TCP tunnel on specific remote port
localup -p 5432 --protocol tcp --remote-port 15432 -r relay.example.com:4443 -t $TOKEN

# HTTP tunnel with Basic Auth
localup -p 3000 --basic-auth "admin:secret" --basic-auth "user:pass" -r relay.example.com:4443 -t $TOKEN

# HTTP tunnel with IP allowlisting
localup -p 3000 --allow-ip 192.168.1.0/24 --allow-ip 10.0.0.5 -r relay.example.com:4443 -t $TOKEN

# TLS tunnel with custom domain and HTTP passthrough
localup -p 9443 --protocol tls --custom-domain "*.example.com" --http-port 9080 -r relay.example.com:4443 -t $TOKEN

# Use WebSocket transport (firewall-friendly)
localup -p 3000 --transport websocket -r relay.example.com:4443 -t $TOKEN
```

---

## Project Config Commands

### `localup init`

Create a new `.localup.yml` config file in the current directory.

```bash
localup init
```

### `localup up`

Start tunnels defined in `.localup.yml`.

```bash
# Start all enabled tunnels
localup up

# Start specific tunnels only
localup up --tunnels web,api
```

### `localup down`

Stop all tunnels started from `.localup.yml`.

```bash
localup down
```

### `localup status`

Show status of running tunnels.

```bash
localup status
```

---

## Tunnel Config Management

### `localup add <NAME>`

Add a tunnel to `.localup.yml`.

```bash
localup add web -p 3000 --protocol http -s myapp
localup add api -p 8080 --protocol https -s api --allow-ip 10.0.0.0/8
localup add db -p 5432 --protocol tcp --remote-port 15432
```

Accepts the same flags as standalone mode plus:

| Flag | Description |
|------|-------------|
| `--enabled` | Auto-start with daemon |

### `localup list`

List all tunnel configurations.

### `localup show <NAME>`

Show details of a specific tunnel.

### `localup remove <NAME>`

Remove a tunnel configuration.

### `localup enable <NAME>` / `localup disable <NAME>`

Enable or disable auto-start with daemon.

---

## Daemon Commands

### `localup daemon start`

Start the daemon in foreground.

```bash
localup daemon start
localup daemon start -c /path/to/.localup.yml
```

### `localup daemon stop`

Stop the running daemon.

### `localup daemon status`

Show daemon status and running tunnels.

### `localup daemon list`

List all configured tunnels.

### `localup daemon reload`

Reload all tunnel configurations.

### `localup daemon tunnel-start <NAME>`

Start a specific tunnel by name.

### `localup daemon tunnel-stop <NAME>`

Stop a specific tunnel by name.

### `localup daemon tunnel-reload <NAME>`

Reload a specific tunnel (stop + start with new config).

### `localup daemon add <NAME>` / `localup daemon remove <NAME>`

Add or remove a tunnel from the daemon configuration.

```bash
localup daemon add frontend -p 3000 --protocol https -s frontend
localup daemon remove frontend
```

---

## System Service Commands

Install localup as a system service (launchd on macOS, systemd on Linux).

```bash
localup service install      # Install service
localup service uninstall    # Uninstall service
localup service start        # Start service
localup service stop         # Stop service
localup service restart      # Restart service
localup service status       # Check service status
localup service logs         # View logs (default: 50 lines)
localup service logs -n 100  # View last 100 lines
```

---

## Relay Server Commands

Run a relay (exit node) server. See [Relay Configuration](custom-relay-config.md) for advanced setup.

### `localup relay tcp`

TCP relay with port-based routing.

```bash
localup relay tcp \
  --localup-addr 0.0.0.0:4443 \
  --tcp-port-range 10000-20000 \
  --domain relay.example.com \
  --jwt-secret "my-secret" \
  --database-url "sqlite://./tunnel.db?mode=rwc"
```

### `localup relay tls`

TLS/SNI relay with SNI-based routing (no TLS termination).

```bash
localup relay tls \
  --localup-addr 0.0.0.0:4443 \
  --tls-addr 0.0.0.0:443 \
  --domain relay.example.com \
  --jwt-secret "my-secret"
```

### `localup relay http`

HTTP/HTTPS relay with host-based routing and TLS termination.

```bash
localup relay http \
  --localup-addr 0.0.0.0:4443 \
  --http-addr 0.0.0.0:80 \
  --https-addr 0.0.0.0:443 \
  --tls-cert /path/to/cert.pem \
  --tls-key /path/to/key.pem \
  --domain relay.example.com \
  --jwt-secret "my-secret"
```

### Common Relay Options

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--localup-addr <ADDR>` | | `0.0.0.0:4443` | QUIC control plane address |
| `--domain <DOMAIN>` | | `localhost` | Public domain name |
| `--jwt-secret <SECRET>` | `JWT_SECRET` | | JWT validation secret |
| `--tls-cert <PATH>` | | auto-generated | TLS certificate |
| `--tls-key <PATH>` | | auto-generated | TLS private key |
| `--database-url <URL>` | `DATABASE_URL` | in-memory SQLite | Database connection |
| `--log-level <LEVEL>` | | `info` | Log level |
| `--transport <PROTO>` | | `quic` | Control plane transport |

### API Server Options (All Relay Types)

| Flag | Env Var | Description |
|------|---------|-------------|
| `--api-http-addr <ADDR>` | `API_HTTP_ADDR` | HTTP API server address |
| `--api-https-addr <ADDR>` | `API_HTTPS_ADDR` | HTTPS API server address |
| `--api-tls-cert <PATH>` | `API_TLS_CERT` | TLS cert for API server |
| `--api-tls-key <PATH>` | `API_TLS_KEY` | TLS key for API server |
| `--no-api` | | Disable API server |

### User Management Options (All Relay Types)

| Flag | Env Var | Description |
|------|---------|-------------|
| `--admin-email <EMAIL>` | `ADMIN_EMAIL` | Create admin user on startup |
| `--admin-password <PASS>` | `ADMIN_PASSWORD` | Admin password |
| `--admin-username <NAME>` | `ADMIN_USERNAME` | Admin username |
| `--allow-signup` | `ALLOW_SIGNUP` | Allow public user registration |

### HTTP Relay Extras

| Flag | Env Var | Description |
|------|---------|-------------|
| `--acme-email <EMAIL>` | `ACME_EMAIL` | Let's Encrypt email |
| `--acme-staging` | | Use Let's Encrypt staging |
| `--acme-cert-dir <PATH>` | | ACME cert directory (default: `/opt/localup/certs/acme`) |
| `--websocket-path <PATH>` | | WebSocket endpoint (default: `/localup`) |
| `--smtp-host <HOST>` | `SMTP_HOST` | SMTP server for email features |
| `--smtp-port <PORT>` | `SMTP_PORT` | SMTP port (default: `587`) |
| `--smtp-username <USER>` | `SMTP_USERNAME` | SMTP username |
| `--smtp-password <PASS>` | `SMTP_PASSWORD` | SMTP password |
| `--smtp-from <EMAIL>` | `SMTP_FROM` | Sender email address |

---

## Agent & Reverse Tunnel Commands

See [Reverse Tunnels](reverse-tunnels.md) for detailed usage guide.

### `localup agent`

Run as a reverse tunnel agent, exposing a private service through the relay.

```bash
localup agent \
  --relay relay.example.com:4443 \
  --token $TOKEN \
  --target-address 192.168.1.100:8080 \
  --agent-id my-service
```

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--relay <ADDR>` | `LOCALUP_RELAY_ADDR` | `localhost:4443` | Relay server |
| `--token <TOKEN>` | `LOCALUP_AUTH_TOKEN` | | Auth token (required) |
| `--target-address <ADDR>` | `LOCALUP_TARGET_ADDRESS` | | Target to forward to (required) |
| `--agent-id <ID>` | `LOCALUP_AGENT_ID` | auto-generated | Agent identifier |
| `--insecure` | `LOCALUP_INSECURE` | | Skip TLS verification |
| `--jwt-secret <SECRET>` | `LOCALUP_JWT_SECRET` | | JWT secret for client auth |
| `--log-level <LEVEL>` | `RUST_LOG` | `info` | Log level |

### `localup connect`

Connect through a reverse tunnel to access a private service.

```bash
localup connect \
  --relay relay.example.com:4443 \
  --agent-id my-service \
  --remote-address 192.168.1.100:8080 \
  --local-address localhost:18080 \
  --token $TOKEN
```

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--relay <ADDR>` | `LOCALUP_RELAY` | | Relay server (required) |
| `--agent-id <ID>` | | | Agent ID to route through (required) |
| `--remote-address <ADDR>` | | | Remote address to connect to (required) |
| `--local-address <ADDR>` | | `localhost:0` | Local address to bind |
| `--token <TOKEN>` | `LOCALUP_AUTH_TOKEN` | | Auth token for relay |
| `--agent-token <TOKEN>` | `LOCALUP_AGENT_TOKEN` | | Auth token for agent |
| `--insecure` | | | Skip TLS verification |

### `localup agent-server`

Run a standalone agent server (combined relay + agent).

```bash
localup agent-server \
  --listen 0.0.0.0:4443 \
  --jwt-secret "my-secret" \
  --target-address 192.168.1.100:8080
```

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--listen <ADDR>` | `LOCALUP_LISTEN` | `0.0.0.0:4443` | QUIC listen address |
| `--cert <PATH>` | `LOCALUP_CERT` | auto-generated | TLS certificate |
| `--key <PATH>` | `LOCALUP_KEY` | auto-generated | TLS key |
| `--jwt-secret <SECRET>` | `LOCALUP_JWT_SECRET` | | JWT authentication secret |
| `--target-address <ADDR>` | `LOCALUP_TARGET_ADDRESS` | | Backend service address |
| `--relay-addr <ADDR>` | `LOCALUP_RELAY_ADDR` | | Upstream relay address |
| `--relay-id <ID>` | `LOCALUP_RELAY_ID` | | Server ID on upstream relay |
| `--relay-token <TOKEN>` | `LOCALUP_RELAY_TOKEN` | | Auth token for upstream relay |
| `--verbose` | | | Enable verbose logging |

---

## Token Generation

### `localup generate-token`

Generate JWT tokens for client authentication.

```bash
# Basic token (24-hour validity)
localup generate-token --secret "my-secret" --sub myapp

# 48-hour token
localup generate-token --secret "my-secret" --sub myapp --hours 48

# Token with subdomain restrictions
localup generate-token --secret "my-secret" --sub myapp --allowed-subdomain "myapp-*"

# Token for reverse tunnel access
localup generate-token --secret "my-secret" --sub myapp --reverse-tunnel --agent my-agent

# Script-friendly (token only, no extra output)
localup generate-token --secret "my-secret" --sub myapp --token-only
```

| Flag | Env Var | Description |
|------|---------|-------------|
| `--secret <SECRET>` | `TUNNEL_JWT_SECRET` | JWT signing secret (required) |
| `--sub <ID>` | | Subject/tunnel ID (auto-generated UUID if omitted) |
| `--user-id <UUID>` | | User ID who owns token |
| `--hours <HOURS>` | | Token validity (default: `24`) |
| `--reverse-tunnel` | | Enable reverse tunnel access |
| `--agent <AGENT_ID>` | | Allowed agent IDs (repeatable) |
| `--allowed-address <ADDR>` | | Allowed target addresses (repeatable) |
| `--allowed-subdomain <PATTERN>` | | Allowed subdomain patterns (repeatable, glob) |
| `--token-only` | | Output only JWT token |

---

## Config Commands

### `localup config set-token <TOKEN>`

Store a default authentication token.

### `localup config get-token`

Display the stored authentication token.

### `localup config clear-token`

Remove the stored authentication token.

---

## Environment Variables

### Client

| Variable | Description |
|----------|-------------|
| `TUNNEL_AUTH_TOKEN` | Default authentication token |
| `RELAY` | Default relay server address |

### Relay Server

| Variable | Description |
|----------|-------------|
| `JWT_SECRET` | JWT validation secret |
| `DATABASE_URL` | Database connection string |
| `API_HTTP_ADDR` | HTTP API server address |
| `API_HTTPS_ADDR` | HTTPS API server address |
| `API_TLS_CERT` | API TLS certificate path |
| `API_TLS_KEY` | API TLS key path |
| `ADMIN_EMAIL` | Auto-create admin email |
| `ADMIN_PASSWORD` | Auto-create admin password |
| `ADMIN_USERNAME` | Auto-create admin username |
| `ALLOW_SIGNUP` | Allow public user registration |
| `ACME_EMAIL` | Let's Encrypt email |
| `SMTP_HOST` | SMTP server hostname |
| `SMTP_PORT` | SMTP port |
| `SMTP_USERNAME` | SMTP username |
| `SMTP_PASSWORD` | SMTP password |
| `SMTP_FROM` | Sender email address |

### Agent

| Variable | Description |
|----------|-------------|
| `LOCALUP_RELAY_ADDR` | Relay server address |
| `LOCALUP_AUTH_TOKEN` | Auth token |
| `LOCALUP_AGENT_TOKEN` | Agent-specific auth token |
| `LOCALUP_TARGET_ADDRESS` | Target address to forward to |
| `LOCALUP_AGENT_ID` | Agent identifier |
| `LOCALUP_INSECURE` | Skip TLS verification |
| `LOCALUP_JWT_SECRET` | JWT secret |

### Agent Server

| Variable | Description |
|----------|-------------|
| `LOCALUP_LISTEN` | QUIC listen address |
| `LOCALUP_CERT` | TLS certificate path |
| `LOCALUP_KEY` | TLS key path |
| `LOCALUP_RELAY_ID` | Server ID on upstream relay |
| `LOCALUP_RELAY_TOKEN` | Upstream relay auth token |

### Logging

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Log level filter (e.g., `info`, `localup=debug`) |

---

## File Locations

| Path | Description |
|------|-------------|
| `.localup.yml` | Project tunnel configuration (discovered hierarchically) |
| `~/.localup/config.json` | Stored tokens and IPC socket info |
| `~/.localup/` | Auto-generated certificates (if not provided) |
| `/opt/localup/certs/acme` | ACME certificate directory (default) |
