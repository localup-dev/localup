# Daemon Management Guide

This guide covers managing the LocalUp daemon for running multiple concurrent tunnels.

## Table of Contents

- [Overview](#overview)
- [Daemon vs Service Mode](#daemon-vs-service-mode)
- [Starting the Daemon](#starting-the-daemon)
- [Managing Tunnels](#managing-tunnels)
- [Monitoring and Status](#monitoring-and-status)
- [Stopping the Daemon](#stopping-the-daemon)
- [Troubleshooting](#troubleshooting)
- [Advanced Usage](#advanced-usage)

## Overview

The LocalUp daemon allows you to run multiple tunnel connections simultaneously from a single process. Each tunnel:

- Runs independently with its own reconnection logic
- Has its own status tracking (Starting, Connected, Reconnecting, Failed, Stopped)
- Maintains separate connection state and metrics
- Can be started/stopped individually

**Key Concept**: The daemon loads tunnel configurations from `~/.localup/tunnels/*.json` and automatically starts all tunnels marked as `enabled: true`.

## Daemon vs Service Mode

### Daemon Mode (Foreground)

```bash
localup daemon start
```

**Use When**:
- Developing and testing tunnel configurations
- Need to see real-time logs in terminal
- Want easy Ctrl+C shutdown
- Debugging connection issues

**Characteristics**:
- Runs in foreground (blocks terminal)
- Logs to stdout/stderr
- Stops when terminal closes or Ctrl+C pressed
- No automatic restart on failure

### Service Mode (Background)

```bash
localup service install
localup service start
```

**Use When**:
- Running in production
- Need automatic restart on failure
- Want daemon to start on system boot
- Don't need real-time terminal output

**Characteristics**:
- Runs in background (detached from terminal)
- Logs to files (macOS) or journalctl (Linux)
- Survives terminal closure
- Automatic restart on failure
- Starts on login/boot

**Recommendation**: Use daemon mode for development, service mode for production.

## Starting the Daemon

### Prerequisites

1. **Configure at least one tunnel**:
   ```bash
   localup add myapp \
     --port 3000 \
     --protocol http \
     --subdomain myapp \
     --token <YOUR_TOKEN> \
     --enabled
   ```

2. **Verify tunnel configuration**:
   ```bash
   localup list
   ```

   Output:
   ```
   Configured tunnels (1)

     ✅ Enabled myapp
       Protocol: HTTP, Port: 3000
       Subdomain: myapp
       Relay: Auto
   ```

### Start Daemon

```bash
localup daemon start
```

**Expected Output**:
```
🚀 Daemon starting...
Found 1 enabled tunnel(s)
Starting tunnel: myapp
[myapp] Connecting... (attempt 1)
[myapp] ✅ Connected successfully!
[myapp] 🌐 Public URL: https://myapp.localup.dev
✅ Daemon ready
```

The daemon is now running and will:
- Keep all enabled tunnels connected
- Automatically reconnect on connection loss
- Display logs for all tunnels in the terminal
- Run until you press Ctrl+C

### Graceful Shutdown

Press **Ctrl+C** to stop the daemon:

```
^C
Shutting down daemon...
[myapp] Shutdown requested, sending disconnect...
[myapp] ✅ Closed gracefully
✅ Daemon stopped
```

All tunnels will:
1. Receive disconnect signal
2. Send `Disconnect` message to exit node
3. Wait for `DisconnectAck` confirmation
4. Close cleanly (timeout: 5 seconds)

## Managing Tunnels

### Add a New Tunnel

```bash
# HTTP tunnel
localup add web \
  --port 3000 \
  --protocol http \
  --subdomain myapp \
  --token <TOKEN> \
  --enabled

# HTTPS tunnel
localup add secure \
  --port 8443 \
  --protocol https \
  --subdomain secure \
  --domain example.com \
  --token <TOKEN> \
  --enabled

# TCP tunnel
localup add database \
  --port 5432 \
  --protocol tcp \
  --remote-port 5432 \
  --token <TOKEN> \
  --enabled

# TLS tunnel
localup add tls-app \
  --port 9000 \
  --protocol tls \
  --subdomain mytls \
  --remote-port 9000 \
  --token <TOKEN> \
  --enabled
```

### List All Tunnels

```bash
localup list
```

**Example Output**:
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
    Relay: Custom (relay.example.com:8080)
```

### View Tunnel Details

```bash
localup show web
```

**Example Output** (JSON):
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

### Enable/Disable Tunnels

**Enable** (auto-start with daemon):
```bash
localup enable api
```

**Disable** (don't auto-start):
```bash
localup disable api
```

**Important**: Changes take effect on next daemon restart:
```bash
# If running as daemon (Ctrl+C to stop, then restart)
localup daemon start

# If running as service
localup service restart
```

### Remove a Tunnel

```bash
localup remove api
```

**Warning**: This permanently deletes the tunnel configuration file.

### Update a Tunnel

To update a tunnel, remove and re-add it:

```bash
# Remove old configuration
localup remove web

# Add new configuration
localup add web \
  --port 3001 \
  --protocol http \
  --subdomain myapp-v2 \
  --token <TOKEN> \
  --enabled

# Restart daemon
localup service restart  # or Ctrl+C + restart if using daemon mode
```

## Monitoring and Status

### Real-Time Logs (Daemon Mode)

When running `localup daemon start`, you'll see:

```
[web] Connecting... (attempt 1)
[web] ✅ Connected successfully!
[web] 🌐 Public URL: https://myapp.localup.dev
[api] Connecting... (attempt 1)
[api] ✅ Connected successfully!
[api] 🌐 Public URL: https://myapi.localup.dev
```

**Log Prefixes**:
- `[tunnel-name]` identifies which tunnel the log belongs to
- `✅` indicates success
- `❌` indicates errors
- `🔄` indicates reconnection attempts
- `⏳` indicates waiting/backoff

### Connection Status

Each tunnel can be in one of these states:

1. **Starting**: Initial connection attempt
   ```
   [web] Connecting... (attempt 1)
   ```

2. **Connected**: Successfully connected to exit node
   ```
   [web] ✅ Connected successfully!
   [web] 🌐 Public URL: https://myapp.localup.dev
   ```

3. **Reconnecting**: Connection lost, attempting to reconnect
   ```
   [web] Connection lost, attempting to reconnect...
   [web] ⏳ Waiting 2 seconds before reconnecting...
   [web] Connecting... (attempt 2)
   ```

4. **Failed**: Non-recoverable error (e.g., authentication failure)
   ```
   [web] ❌ Failed to connect: Authentication failed
   [web] 🚫 Non-recoverable error, stopping tunnel
   ```

5. **Stopped**: Gracefully shut down
   ```
   [web] Shutdown requested, sending disconnect...
   [web] ✅ Closed gracefully
   ```

### Check Daemon Status

```bash
localup daemon status
```

**Current Implementation**: Shows basic status message.

**Future Enhancement**: Will query running daemon via Unix socket for detailed status of all tunnels.

### Service Status (Background Mode)

If running as a service:

```bash
localup service status
```

**macOS Output**:
```
Service status: Running ✅
```

**Linux Output**:
```
Service status: Running ✅
```

### View Service Logs

```bash
# Last 50 lines (default)
localup service logs

# Last 200 lines
localup service logs --lines 200
```

**macOS**:
```bash
# Logs are in
~/.localup/logs/daemon.log        # stdout
~/.localup/logs/daemon.error.log  # stderr

# Tail logs directly
tail -f ~/.localup/logs/daemon.log
```

**Linux**:
```bash
# Follow logs in real-time
journalctl --user -u localup -f

# View last 100 lines
journalctl --user -u localup -n 100

# View logs since last boot
journalctl --user -u localup -b
```

## Stopping the Daemon

### Foreground Daemon

**Method 1**: Press **Ctrl+C** in the terminal where daemon is running

**Method 2**: Send SIGINT from another terminal:
```bash
# Find daemon process
ps aux | grep "localup daemon start"

# Send SIGINT (graceful shutdown)
kill -INT <PID>
```

### Background Service

```bash
localup service stop
```

This will:
1. Stop all running tunnels gracefully
2. Wait for disconnect acknowledgments (timeout: 5s)
3. Clean up resources
4. Exit the daemon process

## Troubleshooting

### Daemon Won't Start

**Symptom**: Daemon exits immediately after starting

**Check**:
```bash
# Verify at least one tunnel is enabled
localup list | grep "✅ Enabled"
```

**Solution**:
```bash
# Enable at least one tunnel
localup enable myapp

# Restart daemon
localup daemon start
```

---

**Symptom**: "Failed to create daemon" error

**Cause**: Cannot access home directory or create `~/.localup/tunnels/`

**Solution**:
```bash
# Check permissions
ls -la ~/.localup/

# Create directory manually
mkdir -p ~/.localup/tunnels
chmod 700 ~/.localup/tunnels
```

### Tunnel Won't Connect

**Symptom**: `[myapp] ❌ Failed to connect: Authentication failed`

**Solution**:
```bash
# Verify token
localup show myapp | grep auth_token

# Update token
localup remove myapp
localup add myapp --port 3000 --protocol http --token <NEW_TOKEN> --enabled

# Restart daemon
localup service restart
```

---

**Symptom**: `[myapp] ❌ Failed to connect: Connection timeout`

**Causes**:
- Relay server down
- Network connectivity issues
- Firewall blocking UDP/QUIC

**Solution**:
```bash
# Test connectivity
ping relay.localup.dev

# Check if UDP is blocked
nc -u -v relay.localup.dev 8080

# Try different relay
localup remove myapp
localup add myapp \
  --port 3000 \
  --protocol http \
  --relay relay2.localup.dev:8080 \
  --token <TOKEN> \
  --enabled
```

### Reconnection Loop

**Symptom**: Tunnel constantly reconnecting

```
[myapp] Connection lost, attempting to reconnect...
[myapp] ⏳ Waiting 1 seconds before reconnecting...
[myapp] Connecting... (attempt 2)
[myapp] ❌ Failed to connect
[myapp] ⏳ Waiting 2 seconds before reconnecting...
[myapp] Connecting... (attempt 3)
[myapp] ❌ Failed to connect
...
```

**Causes**:
- Local service not running on specified port
- Exit node issues
- Network instability

**Solution**:
```bash
# 1. Verify local service is running
curl http://localhost:3000

# 2. Check local service logs
# (depends on your application)

# 3. Temporarily disable the tunnel
localup disable myapp
localup service restart

# 4. Fix local service, then re-enable
localup enable myapp
localup service restart
```

### Port Already in Use

**Symptom**: Daemon starts but tunnel fails with port conflict

**Cause**: Another process is using the specified local port

**Solution**:
```bash
# Find process using port
lsof -i :3000  # macOS/Linux

# Kill process or update tunnel port
localup remove myapp
localup add myapp --port 3001 --protocol http --token <TOKEN> --enabled
```

### Daemon Consumes Too Much Memory

**Symptom**: Daemon using excessive memory (>100MB per tunnel)

**Causes**:
- Memory leak in client library
- Too many concurrent connections
- Large request/response bodies

**Diagnosis**:
```bash
# Monitor memory usage
ps aux | grep localup

# macOS
top -pid <PID>

# Linux
htop -p <PID>
```

**Solution**:
```bash
# Restart daemon periodically (workaround)
localup service restart

# Reduce number of tunnels
localup list
localup disable <unused-tunnel>
localup service restart
```

### Logs Not Appearing

**macOS Service Mode**:
```bash
# Check if log directory exists
ls -la ~/.localup/logs/

# Create if missing
mkdir -p ~/.localup/logs

# Restart service
localup service restart

# Verify logs
tail -f ~/.localup/logs/daemon.log
```

**Linux Service Mode**:
```bash
# Check journalctl
journalctl --user -u localup

# If empty, check service status
systemctl --user status localup

# Restart service
localup service restart
```

## Advanced Usage

### Multiple Environments

Use different tunnel names for different environments:

```bash
# Development
localup add dev-web --port 3000 --subdomain dev-app --token <TOKEN>
localup add dev-api --port 8080 --subdomain dev-api --token <TOKEN>

# Staging
localup add staging-web --port 3001 --subdomain staging-app --token <TOKEN> --enabled
localup add staging-api --port 8081 --subdomain staging-api --token <TOKEN> --enabled

# Production
localup add prod-web --port 80 --subdomain app --token <TOKEN> --enabled
localup add prod-api --port 8080 --subdomain api --token <TOKEN> --enabled

# Enable only staging
localup disable prod-web
localup disable prod-api
localup enable staging-web
localup enable staging-api

localup daemon start
```

### Rotating Tokens

```bash
# 1. Create new tunnel with new token
localup add myapp-new \
  --port 3000 \
  --protocol http \
  --subdomain myapp \
  --token <NEW_TOKEN> \
  --enabled

# 2. Disable old tunnel
localup disable myapp

# 3. Restart daemon (both tunnels will be active briefly)
localup service restart

# 4. Verify new tunnel works
localup service logs | grep myapp-new

# 5. Remove old tunnel
localup remove myapp

# 6. Rename new tunnel (optional)
localup remove myapp-new
localup add myapp \
  --port 3000 \
  --protocol http \
  --subdomain myapp \
  --token <NEW_TOKEN> \
  --enabled
```

### Custom Relay Selection

```bash
# Use specific relay server
localup add custom \
  --port 3000 \
  --protocol http \
  --relay custom-relay.example.com:8080 \
  --token <TOKEN> \
  --enabled

# Verify relay in configuration
localup show custom | grep exit_node
```

### Backup and Restore

**Backup Tunnels**:
```bash
# Create backup
tar -czf localup-tunnels-$(date +%Y%m%d).tar.gz ~/.localup/tunnels/

# Or copy to safe location
cp -r ~/.localup/tunnels ~/backups/localup-tunnels-$(date +%Y%m%d)
```

**Restore Tunnels**:
```bash
# Stop daemon first
localup service stop

# Restore from backup
tar -xzf localup-tunnels-20250129.tar.gz -C ~/

# Restart daemon
localup service start
```

### Migrate to Different Machine

```bash
# On source machine
cd ~
tar -czf localup-config.tar.gz .localup/

# Transfer to target machine
scp localup-config.tar.gz user@target:~/

# On target machine
tar -xzf localup-config.tar.gz -C ~/
localup service install
localup service start
```

### Daemon Configuration as Code

Store tunnel configurations in version control:

```bash
# In your project repository
mkdir -p .localup/
cp ~/.localup/tunnels/myapp.json .localup/

# Add to git
git add .localup/myapp.json
git commit -m "Add LocalUp tunnel configuration"

# On deploy
cp .localup/myapp.json ~/.localup/tunnels/
localup service restart
```

**Security Note**: Be careful not to commit tokens to public repositories. Use environment variables or secrets management:

```json
{
  "name": "myapp",
  "enabled": true,
  "config": {
    "auth_token": "${LOCALUP_TOKEN}",
    ...
  }
}
```

Then substitute before copying:
```bash
envsubst < .localup/myapp.json > ~/.localup/tunnels/myapp.json
```

### Monitoring with External Tools

#### Prometheus Metrics

Currently, metrics are per-tunnel and available via the client library. For daemon-wide metrics, you would need to aggregate them.

**Future Enhancement**: Expose daemon metrics on `/metrics` endpoint.

#### Health Checks

Create a simple health check script:

```bash
#!/bin/bash
# health-check.sh

# Check if service is running
if ! localup service status | grep -q "Running"; then
  echo "Service not running"
  exit 1
fi

# Check logs for errors in last 5 minutes
if localup service logs --lines 500 | grep "❌.*Failed to connect" | grep -q "$(date -d '5 minutes ago' '+%Y-%m-%d %H:%M')"; then
  echo "Recent connection failures detected"
  exit 1
fi

echo "Health check passed"
exit 0
```

Run periodically:
```bash
# Add to crontab
*/5 * * * * /path/to/health-check.sh || mail -s "LocalUp Daemon Alert" admin@example.com
```

## Environment Variables

- `HOME`: User home directory (used for finding `~/.localup/tunnels/`)
- `TUNNEL_AUTH_TOKEN`: Default token for tunnel commands (can be overridden per-tunnel)
- `RELAY`: Default relay server (can be overridden per-tunnel)

Example:
```bash
export TUNNEL_AUTH_TOKEN="your-token-here"
export RELAY="relay.example.com:8080"

# Now you can omit --token and --relay
localup add myapp --port 3000 --protocol http --enabled
```

## Best Practices

1. **Use descriptive tunnel names**: `web-production`, `api-staging`, not `tunnel1`, `tunnel2`
2. **Enable only needed tunnels**: Disable unused tunnels to save resources
3. **Monitor logs regularly**: Check for connection issues or errors
4. **Restart daemon after config changes**: Changes take effect on restart
5. **Use service mode in production**: For automatic restart and boot persistence
6. **Backup configurations**: Before making major changes
7. **Test in daemon mode first**: Before installing as service
8. **Set file permissions**: Protect tunnel configurations with `chmod 600`
9. **Rotate tokens periodically**: Update tokens every 90 days
10. **Document your tunnels**: Keep notes on what each tunnel is for

## Related Documentation

- [Daemon Mode Guide](daemon-mode.md) - Complete guide including service installation
- [CLAUDE.md](../CLAUDE.md) - Project architecture and development guide
- [README.md](../README.md) - Quick start and basic usage

## Support

For issues or questions:
- Check [Troubleshooting](#troubleshooting) section above
- Review logs: `localup service logs`
- Report issues: GitHub issues
- Community: Discord server (if available)
