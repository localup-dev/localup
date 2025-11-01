# Relay Usage - Quick Reference

## TL;DR

✅ **Users CAN specify their own relay server**
✅ **Auto-discovery is optional, not mandatory**
✅ **Both methods work in all modes (standalone and daemon)**

## Three Ways to Specify Relay

### 1. Use Auto-Discovery (Default)

Let LocalUp pick the best relay from embedded configuration:

```bash
localup -p 3000 --protocol http --token YOUR_TOKEN
```

### 2. Use CLI Flag

Specify exact relay server:

```bash
localup -p 3000 --protocol http --token YOUR_TOKEN --relay tunnel.kfs.es:4443
```

### 3. Use Environment Variable

Set default relay for all commands:

```bash
export RELAY=tunnel.kfs.es:4443
localup -p 3000 --protocol http --token YOUR_TOKEN
```

## Comparison

| Method | Command | Pros | Cons |
|--------|---------|------|------|
| **Auto** | `localup -p 3000 --token TOKEN` | Zero config, automatic selection | Limited to embedded relays |
| **CLI Flag** | `localup -p 3000 --token TOKEN -r host:port` | Full control, any relay | Must specify each time |
| **Env Var** | `RELAY=host:port localup -p 3000 --token TOKEN` | Set once, use everywhere | Affects all commands |

## Examples

### Standalone Mode

```bash
# Auto-discovery
localup -p 3000 --protocol http --token TOKEN

# Custom relay
localup -p 3000 --protocol http --token TOKEN --relay custom.relay.com:4443

# Environment variable
export RELAY=custom.relay.com:4443
localup -p 3000 --protocol http --token TOKEN
```

### Daemon Mode

```bash
# Auto-discovery
localup add myapp -p 3000 --protocol http --token TOKEN

# Custom relay
localup add myapp -p 3000 --protocol http --token TOKEN --relay custom.relay.com:4443

# Show which relay is being used
localup show myapp
# Output:
#   Relay: custom.relay.com:4443
#   or
#   Relay: Auto
```

## Priority Order

If multiple relay specifications exist, LocalUp uses this priority (highest to lowest):

1. **`--relay` CLI flag** ← Highest priority
2. **`RELAY` environment variable**
3. **Stored configuration** (daemon mode)
4. **Auto-discovery** ← Lowest priority

## Verification

Check which relay is being used:

```bash
# Enable debug logging
localup -p 3000 --log-level debug --token TOKEN

# Look for:
# INFO Using automatic relay selection
# or
# INFO Using custom relay: tunnel.kfs.es:4443
```

## Key Points

✅ **Flexibility**: Use auto-discovery OR custom relay, your choice
✅ **Override**: CLI flag always overrides auto-discovery
✅ **Environment**: `RELAY` env var sets default for all commands
✅ **Per-tunnel**: Each daemon tunnel can have different relay
✅ **Testing**: Easy to test against different relays

## Use Cases

### Use Auto-Discovery When:
- You trust the embedded relay configuration
- You want zero-configuration setup
- You want automatic best-relay selection

### Use Custom Relay When:
- You have your own relay server
- You need to test specific relay instance
- You want to use internal/private relay
- You need full control over routing

---

**📖 Detailed Documentation:**
- [Relay Selection Guide](docs/relay-selection.md)
- [Examples](docs/examples.md)
- [Custom Relay Configuration](docs/custom-relay-config.md)
