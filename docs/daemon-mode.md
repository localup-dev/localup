# Daemon Mode and Service Installation Guide

This guide explains how to use LocalUp's daemon mode for managing multiple tunnels and installing it as a system service.

## Overview

LocalUp now supports three modes of operation:

1. **Standalone Mode**: Quick one-off tunnels (original behavior)
2. **Daemon Mode**: Run multiple tunnels concurrently in the foreground
3. **Service Mode**: Run daemon as a background system service (macOS/Linux)

## Quick Start

### Standalone Mode (Quick Tunnels)

For quick, temporary tunnels, use the original syntax:

```bash
# HTTP tunnel
localup --port 3000 --token <YOUR_TOKEN>

# HTTPS tunnel with subdomain
localup --port 3000 --protocol https --subdomain myapp --token <YOUR_TOKEN>

# TCP tunnel
localup --port 5432 --protocol tcp --remote-port 5432 --token <YOUR_TOKEN>
```

### Daemon Mode (Multiple Tunnels)

For managing multiple persistent tunnels:

```bash
# 1. Add tunnel configurations
localup add web --port 3000 --protocol http --token <TOKEN> --enabled
localup add api --port 8080 --protocol http --subdomain myapi --token <TOKEN> --enabled
localup add postgres --port 5432 --protocol tcp --token <TOKEN> --enabled

# 2. Start daemon (runs all enabled tunnels)
localup daemon start

# 3. Check status
localup daemon status
```

### Service Mode (Background Service)

For always-on tunnels that start automatically:

```bash
# 1. Add tunnel configurations (as above)
localup add web --port 3000 --protocol http --token <TOKEN> --enabled

# 2. Install as system service
localup service install

# 3. Start the service
localup service start

# 4. Check service status
localup service status

# 5. View logs
localup service logs --lines 100

# 6. Stop service
localup service stop

# 7. Uninstall service
localup service uninstall
```

## Tunnel Management

### Adding Tunnels

```bash
# HTTP tunnel
localup add myapp \
  --port 3000 \
  --protocol http \
  --subdomain myapp \
  --token <TOKEN> \
  --enabled

# HTTPS tunnel with custom domain
localup add secure-app \
  --port 8443 \
  --protocol https \
  --subdomain secure \
  --domain example.com \
  --token <TOKEN> \
  --enabled

# TCP tunnel with specific remote port
localup add database \
  --port 5432 \
  --protocol tcp \
  --remote-port 5432 \
  --token <TOKEN> \
  --enabled

# TLS tunnel with SNI
localup add tls-app \
  --port 9000 \
  --protocol tls \
  --subdomain mytls \
  --remote-port 9000 \
  --token <TOKEN> \
  --enabled

# Custom relay server
localup add custom \
  --port 3000 \
  --protocol http \
  --relay relay.example.com:8080 \
  --token <TOKEN> \
  --enabled
```

### Listing Tunnels

```bash
localup list
```

Example output:
```
Configured tunnels (3)

  ✅ Enabled web
    Protocol: HTTP, Port: 3000
    Subdomain: myapp
    Relay: Auto

  ⚪ Disabled api
    Protocol: HTTP, Port: 8080
    Subdomain: myapi
    Relay: Auto

  ✅ Enabled database
    Protocol: TCP, Port: 5432 → Remote: 5432
    Relay: Auto
```

### Managing Tunnel Status

```bash
# Enable auto-start
localup enable myapp

# Disable auto-start
localup disable myapp

# View tunnel details (JSON)
localup show myapp

# Remove tunnel
localup remove myapp
```

## Configuration Storage

Tunnel configurations are stored as JSON files in:

- **macOS/Linux**: `~/.localup/tunnels/`
- Each tunnel: `~/.localup/tunnels/<name>.json`

Example configuration file (`web.json`):
```json
{
  "name": "web",
  "enabled": true,
  "config": {
    "local_host": "localhost",
    "protocols": [
      {
        "Http": {
          "local_port": 3000,
          "subdomain": "myapp"
        }
      }
    ],
    "auth_token": "your-token-here",
    "exit_node": "Auto",
    "failover": true,
    "connection_timeout": 30
  }
}
```

## Service Installation Details

### macOS (LaunchAgent)

**Installation Location**: `~/Library/LaunchAgents/com.localup.daemon.plist`

**Log Files**:
- Standard output: `~/.localup/logs/daemon.log`
- Standard error: `~/.localup/logs/daemon.error.log`

**Service Management**:
```bash
# Install service
localup service install

# Start (also enables auto-start on login)
localup service start

# Stop
localup service stop

# Restart
localup service restart

# Check status
localup service status

# View logs (last 50 lines)
localup service logs

# View more logs
localup service logs --lines 200

# Uninstall (stops and removes)
localup service uninstall
```

**Manual Management** (if needed):
```bash
# Load service manually
launchctl load ~/Library/LaunchAgents/com.localup.daemon.plist

# Unload service manually
launchctl unload ~/Library/LaunchAgents/com.localup.daemon.plist

# Check if running
launchctl list | grep localup
```

### Linux (systemd User Service)

**Installation Location**: `~/.config/systemd/user/localup.service`

**Service Management**:
```bash
# Install service
localup service install

# Start and enable
localup service start

# Stop
localup service stop

# Restart
localup service restart

# Check status
localup service status

# View logs
localup service logs

# Uninstall
localup service uninstall
```

**Manual Management** (if needed):
```bash
# Reload systemd daemon
systemctl --user daemon-reload

# Start service
systemctl --user start localup

# Enable auto-start
systemctl --user enable localup

# Check status
systemctl --user status localup

# View logs
journalctl --user -u localup -f
```

## Common Workflows

### Development Workflow

Use standalone mode for quick testing:

```bash
# Start a quick tunnel for your dev server
localup --port 3000 --token <TOKEN>

# Press Ctrl+C to stop when done
```

### Production Workflow

Use service mode for persistent tunnels:

```bash
# 1. Set up your tunnels
localup add frontend --port 80 --protocol https --subdomain app --token <TOKEN> --enabled
localup add backend --port 8080 --protocol https --subdomain api --token <TOKEN> --enabled

# 2. Install and start service
localup service install
localup service start

# 3. Monitor
localup service status
localup service logs

# 4. Update configuration if needed
localup disable backend
localup add backend-v2 --port 8081 --protocol https --subdomain api-v2 --token <TOKEN> --enabled
localup service restart
```

### Multi-Environment Workflow

Use different tunnels for different environments:

```bash
# Development tunnels
localup add dev-web --port 3000 --subdomain dev-myapp --token <TOKEN>
localup add dev-api --port 8080 --subdomain dev-api --token <TOKEN>

# Staging tunnels
localup add staging-web --port 3001 --subdomain staging-myapp --token <TOKEN> --enabled
localup add staging-api --port 8081 --subdomain staging-api --token <TOKEN> --enabled

# Production tunnels
localup add prod-web --port 80 --subdomain myapp --token <TOKEN> --enabled
localup add prod-api --port 8080 --subdomain api --token <TOKEN> --enabled

# Enable only staging tunnels
localup disable prod-web
localup disable prod-api
localup enable staging-web
localup enable staging-api

# Start daemon
localup daemon start
```

## Troubleshooting

### Service won't start

```bash
# Check service status
localup service status

# View logs for errors
localup service logs --lines 100

# Check if tunnels are configured
localup list

# Verify at least one tunnel is enabled
localup enable <name>

# Restart service
localup service restart
```

### Port conflicts

```bash
# Check if port is already in use
lsof -i :3000  # macOS/Linux

# Change tunnel port
localup remove myapp
localup add myapp --port 3001 --protocol http --token <TOKEN> --enabled
```

### Authentication errors

```bash
# Verify your token
echo $TUNNEL_AUTH_TOKEN

# Update tunnel token (remove and re-add)
localup remove myapp
localup add myapp --port 3000 --protocol http --token <NEW_TOKEN> --enabled
```

### Daemon not seeing tunnels

```bash
# Check configuration directory
ls -la ~/.localup/tunnels/

# Verify tunnel is enabled
localup show myapp | grep enabled

# Enable if needed
localup enable myapp
```

### Service logs on macOS

```bash
# View standard output
tail -f ~/.localup/logs/daemon.log

# View errors
tail -f ~/.localup/logs/daemon.error.log

# Or use the CLI
localup service logs --lines 100
```

### Service logs on Linux

```bash
# View recent logs
localup service logs

# Follow logs in real-time
journalctl --user -u localup -f

# View logs from last boot
journalctl --user -u localup -b
```

## Advanced Configuration

### Custom Relay Server

```bash
localup add custom-relay \
  --port 3000 \
  --protocol http \
  --relay my-relay.example.com:8080 \
  --token <TOKEN> \
  --enabled
```

### Multiple Protocols for Same Service

Currently, each tunnel supports one protocol. To expose the same service via multiple protocols:

```bash
# HTTP on port 3000
localup add myapp-http \
  --port 3000 \
  --protocol http \
  --subdomain myapp-http \
  --token <TOKEN> \
  --enabled

# HTTPS on port 3000
localup add myapp-https \
  --port 3000 \
  --protocol https \
  --subdomain myapp \
  --token <TOKEN> \
  --enabled
```

### Metrics Dashboard

The standalone mode includes a built-in metrics dashboard. For daemon mode, metrics are managed per-tunnel by the tunnel client library.

```bash
# Standalone with metrics (default port 9090)
localup --port 3000 --token <TOKEN>

# Standalone with custom metrics port
localup --port 3000 --token <TOKEN> --metrics-port 9091

# Standalone without metrics
localup --port 3000 --token <TOKEN> --no-metrics
```

## Environment Variables

- `TUNNEL_AUTH_TOKEN`: Default authentication token for all commands
- `RELAY`: Default relay server address (can be overridden per-tunnel)

Example:
```bash
export TUNNEL_AUTH_TOKEN="your-token-here"
export RELAY="relay.example.com:8080"

# Now you can omit --token and --relay
localup add myapp --port 3000 --protocol http --enabled
```

## Security Considerations

1. **Token Storage**: Tunnel configurations include authentication tokens. Ensure `~/.localup/tunnels/` has appropriate permissions:
   ```bash
   chmod 700 ~/.localup/tunnels/
   chmod 600 ~/.localup/tunnels/*.json
   ```

2. **Service Permissions**: Services run as your user account, not as root.

3. **Log Files**: Be aware that logs may contain sensitive information.

## Migration from Standalone to Daemon

If you're currently using standalone mode and want to migrate to daemon mode:

```bash
# 1. Note your current flags
# Old: localup --port 3000 --protocol http --subdomain myapp --token <TOKEN>

# 2. Create equivalent tunnel
localup add myapp --port 3000 --protocol http --subdomain myapp --token <TOKEN> --enabled

# 3. Test in daemon mode
localup daemon start

# 4. If working, install as service
localup service install
localup service start

# 5. Verify
localup service status
localup service logs
```

## Next Steps

- See [CLAUDE.md](../CLAUDE.md) for the full project overview
- Check [README.md](../README.md) for basic usage
- Review [SPEC.md](../SPEC.md) for technical details
