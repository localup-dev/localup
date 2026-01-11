# LocalUp Examples

Common usage patterns and examples for LocalUp tunnel client.

## Table of Contents

- [Standalone Mode](#standalone-mode)
- [Daemon Mode](#daemon-mode)
- [Relay Selection](#relay-selection)
- [Protocol Examples](#protocol-examples)
- [Advanced Usage](#advanced-usage)

## Standalone Mode

Run tunnels directly without daemon.

### Basic HTTP Tunnel

```bash
# Expose local HTTP server on port 3000
localup -p 3000 --protocol http --token YOUR_TOKEN
```

### HTTPS Tunnel with Custom Domain

```bash
# Expose with custom domain
localup -p 3000 --protocol https \
  --token YOUR_TOKEN \
  --domain app.yourdomain.com
```

### TCP Tunnel

```bash
# Expose TCP service (e.g., database)
localup -p 5432 --protocol tcp \
  --token YOUR_TOKEN \
  --remote-port 5432
```

### With Custom Relay

```bash
# Use specific relay server
localup -p 3000 --protocol http \
  --token YOUR_TOKEN \
  --relay tunnel.kfs.es:4443
```

### With Subdomain

```bash
# Request specific subdomain
localup -p 3000 --protocol http \
  --token YOUR_TOKEN \
  --subdomain myapp
# Public URL: https://myapp.tunnel.kfs.es
```

## Daemon Mode

Manage persistent tunnels that run in the background.

### Add and Start Tunnel

```bash
# Add tunnel configuration
localup add my-app \
  -p 3000 \
  --protocol http \
  --token YOUR_TOKEN \
  --enabled

# Install as system service
localup service install

# Start service
localup service start
```

### Manage Multiple Tunnels

```bash
# Add multiple tunnels
localup add web-app -p 3000 --protocol http --token TOKEN1 --enabled
localup add api-server -p 8080 --protocol http --token TOKEN2 --enabled
localup add database -p 5432 --protocol tcp --token TOKEN3 --remote-port 5432

# List all tunnels
localup list

# Show specific tunnel
localup show web-app

# Disable tunnel (won't auto-start)
localup disable api-server

# Enable tunnel
localup enable api-server

# Remove tunnel
localup remove database
```

### Service Management

```bash
# Install service
localup service install

# Start service
localup service start

# Check status
localup service status

# View logs
localup service logs

# Restart service
localup service restart

# Stop service
localup service stop

# Uninstall service
localup service uninstall
```

## Relay Selection

### Auto-Discovery (Default)

```bash
# Let LocalUp choose best relay
localup -p 3000 --protocol http --token YOUR_TOKEN

# Add to daemon with auto-discovery
localup add my-app -p 3000 --protocol http --token YOUR_TOKEN
```

### Custom Relay

```bash
# Specify relay directly
localup -p 3000 --protocol http \
  --token YOUR_TOKEN \
  --relay tunnel.kfs.es:4443

# Use environment variable
export RELAY=tunnel.kfs.es:4443
localup -p 3000 --protocol http --token YOUR_TOKEN

# Add to daemon with custom relay
localup add my-app \
  -p 3000 \
  --protocol http \
  --token YOUR_TOKEN \
  --relay tunnel.kfs.es:4443
```

### Private Relay

```bash
# Connect to internal relay server
localup -p 8080 --protocol https \
  --token YOUR_TOKEN \
  --relay internal-relay.corp.local:4443 \
  --domain app.internal.com
```

## Protocol Examples

### HTTP - Web Development

```bash
# React/Vue/Next.js dev server
localup -p 3000 --protocol http --token TOKEN --subdomain myapp

# Express/Node.js API
localup -p 8080 --protocol http --token TOKEN --subdomain api
```

### HTTPS - Production Apps

```bash
# With custom domain and TLS
localup -p 443 --protocol https \
  --token TOKEN \
  --domain app.example.com

# With subdomain
localup -p 8443 --protocol https \
  --token TOKEN \
  --subdomain secure-app
```

### TCP - Database Access

```bash
# PostgreSQL
localup -p 5432 --protocol tcp \
  --token TOKEN \
  --remote-port 5432

# MySQL
localup -p 3306 --protocol tcp \
  --token TOKEN \
  --remote-port 3306

# MongoDB
localup -p 27017 --protocol tcp \
  --token TOKEN \
  --remote-port 27017

# Redis
localup -p 6379 --protocol tcp \
  --token TOKEN \
  --remote-port 6379
```

### TLS - Custom TLS Services

```bash
# TLS passthrough (no termination)
localup -p 8443 --protocol tls \
  --token TOKEN \
  --subdomain secure
```

## Advanced Usage

### Development Environment

```bash
# Frontend (React)
localup add frontend \
  -p 3000 \
  --protocol http \
  --token DEV_TOKEN \
  --subdomain app-dev \
  --enabled

# Backend API
localup add backend \
  -p 8080 \
  --protocol http \
  --token DEV_TOKEN \
  --subdomain api-dev \
  --enabled

# Database (PostgreSQL)
localup add db \
  -p 5432 \
  --protocol tcp \
  --token DEV_TOKEN \
  --remote-port 5432 \
  --enabled

# Start all
localup service start
```

### Staging Environment

```bash
# Use staging relay
export RELAY=staging-relay.example.com:4443

# Add staging tunnels
localup add staging-web \
  -p 3000 \
  --protocol https \
  --token STAGING_TOKEN \
  --domain staging.example.com \
  --enabled

localup add staging-api \
  -p 8080 \
  --protocol https \
  --token STAGING_TOKEN \
  --domain api-staging.example.com \
  --enabled
```

### Testing Multiple Versions

```bash
# Version 1
localup -p 3000 --protocol http --token TOKEN --subdomain v1

# Version 2 (different terminal)
localup -p 3001 --protocol http --token TOKEN --subdomain v2

# Version 3 (different terminal)
localup -p 3002 --protocol http --token TOKEN --subdomain v3
```

### Load Balancing (Manual)

```bash
# Instance 1
localup -p 8080 --protocol http \
  --token TOKEN \
  --subdomain api \
  --relay relay1.example.com:4443

# Instance 2 (different server)
localup -p 8080 --protocol http \
  --token TOKEN \
  --subdomain api \
  --relay relay2.example.com:4443
```

### Metrics and Monitoring

```bash
# Enable metrics dashboard (default port 9090)
localup -p 3000 --protocol http --token TOKEN

# Custom metrics port
localup -p 3000 --protocol http --token TOKEN --metrics-port 8080

# Disable metrics
localup -p 3000 --protocol http --token TOKEN --no-metrics

# View metrics
open http://localhost:9090
```

### Debug and Troubleshooting

```bash
# Enable debug logging
localup -p 3000 --protocol http --token TOKEN --log-level debug

# Trace logging (very verbose)
localup -p 3000 --protocol http --token TOKEN --log-level trace

# Check daemon status
localup daemon status

# View service logs
localup service logs

# Test connection to relay
telnet tunnel.kfs.es 4443
```

### Environment Variables

```bash
# Set default relay
export RELAY=tunnel.kfs.es:4443

# Set auth token
export TUNNEL_AUTH_TOKEN=your-token-here

# Run with env vars
localup -p 3000 --protocol http

# Override env vars
localup -p 3000 --protocol http --token other-token --relay other-relay.com:4443
```

### Script Automation

```bash
#!/bin/bash
# deploy-tunnels.sh

set -e

TOKEN="${TUNNEL_TOKEN}"
RELAY="${RELAY_SERVER}"

# Add all application tunnels
localup add frontend -p 3000 --protocol http --token "$TOKEN" --relay "$RELAY" --enabled
localup add backend -p 8080 --protocol http --token "$TOKEN" --relay "$RELAY" --enabled
localup add api -p 9000 --protocol http --token "$TOKEN" --relay "$RELAY" --enabled

# Install and start service
localup service install
localup service start

echo "✅ All tunnels deployed"
localup list
```

### Docker Integration

```dockerfile
# Dockerfile
FROM rust:latest

# Copy localup binary
COPY target/release/localup /usr/local/bin/localup

# Set environment
ENV TUNNEL_AUTH_TOKEN=your-token
ENV RELAY=tunnel.kfs.es:4443

# Expose local app port
EXPOSE 3000

# Start app and tunnel
CMD ["sh", "-c", "my-app & localup -p 3000 --protocol http"]
```

```bash
# Build and run
docker build -t myapp-with-tunnel .
docker run -p 3000:3000 myapp-with-tunnel
```

### Kubernetes Sidecar

```yaml
# kubernetes-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: myapp
spec:
  template:
    spec:
      containers:
      - name: app
        image: myapp:latest
        ports:
        - containerPort: 3000

      - name: tunnel
        image: localup:latest
        env:
        - name: TUNNEL_AUTH_TOKEN
          valueFrom:
            secretKeyRef:
              name: localup-secret
              key: token
        - name: RELAY
          value: "tunnel.kfs.es:4443"
        command:
        - /usr/local/bin/localup
        - "-p"
        - "3000"
        - "--protocol"
        - "http"
```

### CI/CD Pipeline

```yaml
# .github/workflows/tunnel.yml
name: Deploy with Tunnel

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Download LocalUp
        run: |
          wget https://example.com/localup
          chmod +x localup

      - name: Start Tunnel
        env:
          TUNNEL_AUTH_TOKEN: ${{ secrets.TUNNEL_TOKEN }}
        run: |
          ./localup -p 3000 --protocol http --relay tunnel.kfs.es:4443 &
          TUNNEL_PID=$!
          echo "TUNNEL_PID=$TUNNEL_PID" >> $GITHUB_ENV

      - name: Deploy Application
        run: |
          # Your deployment steps
          npm run build
          npm run deploy

      - name: Cleanup
        if: always()
        run: kill ${{ env.TUNNEL_PID }}
```

## Common Patterns

### Development Setup

```bash
# 1. Start your local server
npm run dev  # localhost:3000

# 2. Expose via tunnel (new terminal)
localup -p 3000 --protocol http --token DEV_TOKEN --subdomain myapp-dev

# 3. Share the public URL
# https://myapp-dev.tunnel.kfs.es
```

### Testing Webhooks

```bash
# 1. Start local webhook receiver
python webhook_server.py  # localhost:8080

# 2. Expose via tunnel
localup -p 8080 --protocol https --token TOKEN --subdomain webhooks

# 3. Configure webhook URL in third-party service
# https://webhooks.tunnel.kfs.es
```

### Remote Access to Local Database

```bash
# 1. Ensure database accepts connections from localhost
# Edit postgresql.conf: listen_addresses = 'localhost'

# 2. Expose via tunnel
localup -p 5432 --protocol tcp --token TOKEN --remote-port 5432

# 3. Connect remotely
psql -h tunnel.kfs.es -p 5432 -U postgres
```

---

**More Documentation:**
- [Relay Selection](relay-selection.md)
- [Daemon Mode](daemon-mode.md)
- [Custom Relay Configuration](custom-relay-config.md)
