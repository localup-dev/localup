# Configuration Guide

localup supports three levels of configuration:

1. **CLI flags** - Per-invocation settings
2. **Project config** (`.localup.yml`) - Multi-tunnel definitions per project
3. **Stored config** (`~/.localup/config.json`) - Persistent defaults (tokens)

---

## Project Configuration (`.localup.yml`)

Define multiple tunnels in a single file and manage them together.

### Creating a Config File

```bash
localup init
```

This creates a `.localup.yml` in the current directory with a skeleton structure.

### File Format

```yaml
# Default settings applied to all tunnels
defaults:
  relay: relay.example.com:4443
  token: ${TUNNEL_AUTH_TOKEN}    # Environment variable expansion
  transport: quic                # quic, h2, websocket
  local_host: localhost
  timeout_seconds: 30

# Tunnel definitions
tunnels:
  - name: web
    port: 3000
    protocol: http
    subdomain: myapp
    enabled: true

  - name: api
    port: 8080
    protocol: https
    subdomain: api
    allow_ips:
      - 10.0.0.0/8
      - 192.168.1.0/24

  - name: database
    port: 5432
    protocol: tcp
    remote_port: 15432
    enabled: false              # Won't start with `localup up`

  - name: tls-service
    port: 9443
    protocol: tls
    custom_domain: service.example.com
    sni_hostnames:
      - service.example.com
      - "*.service.example.com"
    http_port: 9080             # HTTP backend for TLS passthrough

  - name: staging
    port: 3001
    protocol: http
    subdomain: staging
    relay: staging-relay.example.com:4443   # Override default relay
    token: ${STAGING_TOKEN}                 # Override default token
    transport: websocket                    # Override default transport
    local_host: 0.0.0.0                    # Override default local host
```

### Tunnel Fields

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | Yes | | Tunnel name (alphanumeric, hyphens, underscores) |
| `port` | Yes | | Local port to expose |
| `protocol` | No | `http` | `http`, `https`, `tcp`, `tls` |
| `subdomain` | No | | Subdomain for HTTP/HTTPS/TLS tunnels |
| `custom_domain` | No | | Custom domain (supports wildcards) |
| `remote_port` | No | | Remote port for TCP tunnels |
| `sni_hostnames` | No | | SNI hostnames for TLS tunnels (list) |
| `http_port` | No | | HTTP backend port for TLS passthrough |
| `relay` | No | from defaults | Override relay address |
| `token` | No | from defaults | Override auth token |
| `transport` | No | from defaults | Override transport protocol |
| `local_host` | No | from defaults | Override local host |
| `enabled` | No | `true` | Auto-start with `localup up` |
| `allow_ips` | No | | IP allowlist (CIDR format) |

### Environment Variable Expansion

Token and other string values support `${ENV_VAR}` syntax:

```yaml
defaults:
  token: ${TUNNEL_AUTH_TOKEN}

tunnels:
  - name: web
    port: 3000
    token: ${WEB_TUNNEL_TOKEN}
```

### Config File Discovery

localup searches for `.localup.yml` (or `.localup.yaml`) starting from the current directory and walking up to the filesystem root:

```
/home/user/projects/myapp/.localup.yml    # Found first
/home/user/projects/.localup.yml          # Checked second
/home/user/.localup.yml                   # Checked third
/home/.localup.yml                        # ...
```

### Using the Config

```bash
# Start all enabled tunnels
localup up

# Start specific tunnels only
localup up --tunnels web,api

# Stop all tunnels
localup down

# Check status
localup status
```

### Managing Tunnels

```bash
# Add a tunnel to config
localup add frontend -p 3000 --protocol http -s frontend

# List all tunnels
localup list

# Show tunnel details
localup show frontend

# Remove a tunnel
localup remove frontend

# Enable/disable auto-start
localup enable frontend
localup disable frontend
```

---

## Daemon Mode

For long-running tunnel management, use the daemon. It reads from `.localup.yml` and keeps tunnels alive.

```bash
# Start daemon (foreground)
localup daemon start

# Start with specific config
localup daemon start -c /path/to/.localup.yml

# Check status
localup daemon status

# Manage individual tunnels
localup daemon tunnel-start web
localup daemon tunnel-stop web
localup daemon tunnel-reload web

# Reload all configs
localup daemon reload

# Stop daemon
localup daemon stop
```

### System Service

Install as a system service for automatic startup:

```bash
# Install (launchd on macOS, systemd on Linux)
localup service install

# Manage
localup service start
localup service stop
localup service restart
localup service status
localup service logs -n 100

# Uninstall
localup service uninstall
```

---

## Stored Configuration

localup stores persistent configuration in `~/.localup/config.json`.

### Managing Tokens

```bash
# Store a default token
localup config set-token "eyJ0eXAiOiJKV1Qi..."

# View stored token
localup config get-token

# Clear stored token
localup config clear-token
```

The stored token is used when no `--token` flag or `TUNNEL_AUTH_TOKEN` env var is provided.

### Priority Order

Configuration values are resolved in this order (highest priority first):

1. CLI flags (`--token`, `--relay`, etc.)
2. Environment variables (`TUNNEL_AUTH_TOKEN`, `RELAY`)
3. Per-tunnel overrides in `.localup.yml`
4. Defaults section in `.localup.yml`
5. Stored config (`~/.localup/config.json`)

---

## Example: Full Project Setup

```bash
# 1. Initialize config
cd ~/projects/myapp
localup init

# 2. Add tunnels
localup add web -p 3000 --protocol http -s myapp
localup add api -p 8080 --protocol https -s myapp-api
localup add db -p 5432 --protocol tcp --remote-port 15432

# 3. Set default token
localup config set-token "$(localup generate-token --secret my-secret --sub myapp --token-only)"

# 4. Start all tunnels
localup up

# 5. Check status
localup status

# 6. Stop when done
localup down
```

## Example: Team Setup with `.localup.yml` in Git

```yaml
# .localup.yml - Commit this to your repo
defaults:
  relay: relay.mycompany.com:4443
  token: ${TUNNEL_AUTH_TOKEN}   # Each dev sets their own token

tunnels:
  - name: frontend
    port: 3000
    protocol: http
    subdomain: ${USER}-frontend   # Unique per developer

  - name: backend
    port: 8080
    protocol: http
    subdomain: ${USER}-backend
```

Each developer sets their own token:

```bash
export TUNNEL_AUTH_TOKEN="their-personal-token"
localup up
```
