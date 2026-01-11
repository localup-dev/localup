# Relay Server Selection

LocalUp supports two methods for selecting relay servers: **automatic discovery** and **manual specification**.

## Quick Reference

| Method | Command Example | When to Use |
|--------|----------------|-------------|
| **Auto** (default) | `localup -p 3000` | Let LocalUp choose the best relay |
| **Manual** | `localup -p 3000 -r tunnel.example.com:4443` | Specify exact relay server |

## Automatic Relay Selection

When you **don't specify a relay**, LocalUp uses automatic discovery based on the embedded relay configuration.

### Standalone Mode

```bash
# Auto-select relay (uses embedded relays.yaml)
localup -p 3000 --protocol http --token YOUR_TOKEN

# Logs will show:
# INFO Using automatic relay selection
```

### Daemon Mode

```bash
# Add tunnel without relay (uses auto-discovery)
localup add my-app -p 3000 --protocol http --token YOUR_TOKEN

# Show config
localup show my-app
# Output:
#   Relay: Auto
```

### How Auto-Discovery Works

1. **Embedded Configuration**: Relay servers are embedded in the binary at compile time (from `relays.yaml`)
2. **Selection Policy**: Uses the `auto` policy from the configuration:
   - Prefers production-tagged relays
   - Considers relay capacity
   - Only selects active relays
3. **Protocol Matching**: Selects relay based on your protocol (HTTP/HTTPS → HTTPS endpoint, TCP/TLS → TCP endpoint)

### Benefits of Auto-Discovery

✅ **Zero configuration** - Works out of the box
✅ **Best relay selection** - Automatically picks optimal server
✅ **Future-proof** - Updates when you rebuild with new relay config
✅ **Load balancing** - Considers relay capacity and priority

## Manual Relay Specification

When you **do specify a relay**, LocalUp uses your custom relay server instead of auto-discovery.

### Standalone Mode

```bash
# Specify custom relay (HTTPS)
localup -p 3000 --protocol http \
  --token YOUR_TOKEN \
  --relay tunnel.example.com:4443

# Specify custom relay (TCP)
localup -p 8080 --protocol tcp \
  --token YOUR_TOKEN \
  --relay tunnel.example.com:5443

# Using environment variable
export RELAY=tunnel.example.com:4443
localup -p 3000 --protocol http --token YOUR_TOKEN

# Logs will show:
# INFO Using custom relay: tunnel.example.com:4443
```

### Daemon Mode

```bash
# Add tunnel with custom relay
localup add my-app \
  -p 3000 \
  --protocol http \
  --token YOUR_TOKEN \
  --relay tunnel.example.com:4443

# Show config
localup show my-app
# Output:
#   Relay: tunnel.example.com:4443
```

### Relay Address Format

Must be in format: `host:port` or `ip:port`

**Valid examples:**
```bash
--relay tunnel.kfs.es:4443           # Domain with port
--relay 192.168.1.100:8080           # IP address with port
--relay relay.example.com:443        # Domain with standard port
```

**Invalid examples:**
```bash
--relay tunnel.kfs.es                # ❌ Missing port
--relay http://tunnel.kfs.es:4443    # ❌ Don't include protocol
--relay tunnel.kfs.es:4443/path      # ❌ Don't include path
```

### Benefits of Manual Specification

✅ **Full control** - Use any relay server you want
✅ **Private relays** - Connect to internal/private relay servers
✅ **Testing** - Test against specific relay instances
✅ **Bypass discovery** - Skip auto-discovery logic

## Use Cases

### Use Case 1: Production with Auto-Discovery

**Scenario:** You've built a binary with your production relay embedded.

```bash
# relays.yaml contains:
# relays:
#   - id: prod-1
#     endpoints:
#       - protocol: https
#         address: tunnel.kfs.es:4443

# Build binary
cargo build --release -p localup-cli

# Use auto-discovery (picks tunnel.kfs.es:4443)
localup -p 3000 --protocol http --token TOKEN
```

### Use Case 2: Development with Local Relay

**Scenario:** Testing against a local relay server during development.

```bash
# Start local relay server on port 8443
./target/release/localup-relay --port 8443

# Connect to local relay
localup -p 3000 --protocol http \
  --token dev-token \
  --relay localhost:8443
```

### Use Case 3: Multi-Region Deployment

**Scenario:** Your binary has multiple relays embedded, but you want to force a specific region.

```bash
# Force EU relay even if auto-discovery would pick US
localup -p 3000 --protocol http \
  --token TOKEN \
  --relay eu-relay.example.com:443

# Force US relay
localup -p 3000 --protocol http \
  --token TOKEN \
  --relay us-relay.example.com:443
```

### Use Case 4: Private Corporate Relay

**Scenario:** Using an internal relay server behind firewall.

```bash
# Add corporate tunnel
localup add corp-app \
  -p 8080 \
  --protocol https \
  --token CORP_TOKEN \
  --relay internal-relay.corp.local:4443 \
  --domain app.corp.local

# Start daemon
localup service install
localup service start
```

### Use Case 5: Testing Different Relay Versions

**Scenario:** Testing your app against staging vs production relays.

```bash
# Test against staging relay
localup -p 3000 --protocol http \
  --token STAGING_TOKEN \
  --relay staging-relay.example.com:4443

# Test against production relay
localup -p 3000 --protocol http \
  --token PROD_TOKEN \
  --relay prod-relay.example.com:4443
```

## Relay Selection Priority

LocalUp determines which relay to use in this order:

1. **CLI flag**: `--relay host:port` (highest priority)
2. **Environment variable**: `RELAY=host:port`
3. **Stored configuration**: If using daemon mode with saved config
4. **Auto-discovery**: Embedded relay configuration (lowest priority)

## Protocol-Specific Relay Selection

When using auto-discovery, LocalUp selects relay endpoints based on protocol:

| Protocol | Relay Endpoint Type | Default Port |
|----------|---------------------|--------------|
| HTTP     | HTTPS endpoint      | 4443         |
| HTTPS    | HTTPS endpoint      | 4443         |
| TCP      | TCP endpoint        | 5443         |
| TLS      | TCP endpoint        | 5443         |

**Example:** If your embedded `relays.yaml` has:
```yaml
endpoints:
  - protocol: https
    address: tunnel.kfs.es:4443
  - protocol: tcp
    address: tunnel.kfs.es:5443
```

Then:
```bash
# Uses HTTPS endpoint (tunnel.kfs.es:4443)
localup -p 3000 --protocol http --token TOKEN

# Uses TCP endpoint (tunnel.kfs.es:5443)
localup -p 8080 --protocol tcp --token TOKEN
```

## Verifying Relay Selection

### Check Logs

```bash
# Enable debug logging to see relay selection
localup -p 3000 --log-level debug --token TOKEN

# Look for:
# INFO Using automatic relay selection
# or
# INFO Using custom relay: tunnel.kfs.es:4443
```

### Check Stored Configuration

```bash
# Show tunnel config (daemon mode)
localup show my-app

# Output includes:
#   Relay: Auto
#   or
#   Relay: tunnel.kfs.es:4443
```

### Test Connection

```bash
# Test with custom relay
localup -p 3000 --protocol http \
  --token TOKEN \
  --relay tunnel.kfs.es:4443

# If successful, you'll see:
# ✅ Tunnel established
# 📡 Public URL: https://your-subdomain.tunnel.kfs.es
```

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `RELAY` | Custom relay address | `RELAY=tunnel.kfs.es:4443` |
| `LOCALUP_RELAYS_CONFIG` | Custom relay config for **build time** | `LOCALUP_RELAYS_CONFIG=my-relays.yaml` |

**Important:** `LOCALUP_RELAYS_CONFIG` is used **at build time** to embed relay configuration. `RELAY` is used **at runtime** to override auto-discovery.

## Troubleshooting

### Error: Invalid relay address

```
Error: Invalid relay address: tunnel.kfs.es. Expected format: host:port or ip:port
```

**Solution:** Include the port number:
```bash
localup -p 3000 --relay tunnel.kfs.es:4443  # ✅ Correct
```

### Error: Connection refused

```
Error: Failed to connect to relay server: Connection refused
```

**Solutions:**
1. Verify relay server is running
2. Check firewall rules
3. Verify port is correct (4443 for HTTPS, 5443 for TCP)
4. Test with telnet: `telnet tunnel.kfs.es 4443`

### Auto-discovery not working

If auto-discovery doesn't work:

1. **Check binary was built correctly:**
   ```bash
   # Should show relay config message during build
   cargo build --release -p localup-cli 2>&1 | grep "📡"
   ```

2. **Verify relay config is valid:**
   ```bash
   # Check YAML syntax
   python3 -c "import yaml; yaml.safe_load(open('relays.yaml'))"
   ```

3. **Force a specific relay:**
   ```bash
   # Override with custom relay as workaround
   localup -p 3000 --relay tunnel.kfs.es:4443 --token TOKEN
   ```

### Relay changes not reflected

If you modified `relays.yaml` but changes aren't reflected:

**Problem:** Configuration is embedded at **compile time**, not runtime.

**Solution:** Rebuild the binary:
```bash
# Clean and rebuild
cargo clean -p localup-client
cargo build --release -p localup-cli
```

## Best Practices

### ✅ Do

- **Use auto-discovery for production** - Simpler and more maintainable
- **Use custom relay for testing** - Test specific relay versions
- **Document relay addresses** - Keep track of relay endpoints
- **Use environment variables** - `RELAY=host:port` for flexibility
- **Validate relay addresses** - Use `host:port` format

### ❌ Don't

- **Don't hardcode relay in code** - Use CLI flags or env vars instead
- **Don't include protocol** - Use `host:port`, not `https://host:port`
- **Don't expose relay tokens** - Keep auth tokens secure
- **Don't use auto-discovery for private relays** - Explicitly specify private relay addresses

## Examples Summary

```bash
# 1. Auto-discovery (default)
localup -p 3000 --protocol http --token TOKEN

# 2. Custom relay (CLI flag)
localup -p 3000 --protocol http --token TOKEN --relay tunnel.kfs.es:4443

# 3. Custom relay (environment variable)
export RELAY=tunnel.kfs.es:4443
localup -p 3000 --protocol http --token TOKEN

# 4. Daemon mode with auto-discovery
localup add my-app -p 3000 --protocol http --token TOKEN

# 5. Daemon mode with custom relay
localup add my-app -p 3000 --protocol http --token TOKEN --relay tunnel.kfs.es:4443

# 6. Override existing daemon config
# Edit ~/.localup/tunnels/my-app.json manually or:
localup remove my-app
localup add my-app -p 3000 --protocol http --token TOKEN --relay new-relay.com:4443
```

---

**Related Documentation:**
- [Custom Relay Configuration](custom-relay-config.md) - Building with custom embedded relays
- [Daemon Mode](daemon-mode.md) - Managing tunnels with daemon
- [BUILD-CUSTOM.md](../BUILD-CUSTOM.md) - Building custom binaries
