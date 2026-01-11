# @localup/sdk

Node.js SDK for LocalUp - expose local servers through secure tunnels.

## Installation

```bash
bun add @localup/sdk
# or
npm install @localup/sdk
```

### For QUIC transport (optional)

QUIC provides the best performance but requires native bindings:

```bash
# Install the optional QUIC dependency
npm install @matrixai/quic
# or
bun add @matrixai/quic
```

## Quick Start

```typescript
import localup from '@localup/sdk';

const listener = await localup.forward({
  // The port your app is running on
  addr: 8080,

  // Authentication token
  authtoken: process.env.LOCALUP_AUTHTOKEN,

  // Subdomain for your tunnel
  domain: 'myapp',

  // Transport protocol: 'quic', 'websocket', or 'h2'
  transport: 'quic',
});

console.log(`Ingress established at ${listener.url()}`);

// Keep the process alive
process.stdin.resume();
```

## API

### `localup.forward(options)`

Creates a tunnel and forwards traffic to a local address.

#### Options

| Option | Type | Description |
|--------|------|-------------|
| `addr` | `number \| string` | Local port or address (e.g., `8080` or `localhost:8080`) |
| `authtoken` | `string` | JWT authentication token |
| `domain` | `string` | Subdomain for the tunnel |
| `proto` | `'http' \| 'https' \| 'tcp' \| 'tls'` | Protocol type (default: `'http'`) |
| `relay` | `string` | Relay server address (default: `localhost:4443`) |
| `transport` | `'quic' \| 'websocket' \| 'h2'` | Transport protocol (default: `'quic'`) |
| `rejectUnauthorized` | `boolean` | Skip TLS verification (default: `false`) |
| `ipAllowlist` | `string[]` | IP addresses allowed to access the tunnel |

#### Returns: `Listener`

```typescript
interface Listener extends EventEmitter {
  url(): string;              // Public URL of the tunnel
  endpoints(): Endpoint[];    // All endpoints
  tunnelId(): string;         // Unique tunnel ID
  close(): Promise<void>;     // Close the tunnel
  wait(): Promise<void>;      // Wait for tunnel to close
}
```

#### Events

- `request` - Emitted for each HTTP request: `{ method, path, status }`
- `close` - Emitted when the tunnel is closed
- `disconnect` - Emitted when disconnected by the relay

## Environment Variables

| Variable | Description |
|----------|-------------|
| `LOCALUP_AUTHTOKEN` | Default authentication token |
| `LOCALUP_RELAY` | Default relay address |

## Examples

### Basic HTTP Tunnel with QUIC

```typescript
import localup from '@localup/sdk';

const listener = await localup.forward({
  addr: 3000,
  authtoken: process.env.LOCALUP_AUTHTOKEN,
  transport: 'quic',
});

console.log(`Tunnel: ${listener.url()}`);

listener.on('request', ({ method, path, status }) => {
  console.log(`[${method}] ${path} -> ${status}`);
});
```

### Using WebSocket Transport

If QUIC is not available, use WebSocket:

```typescript
const listener = await localup.forward({
  addr: 3000,
  authtoken: process.env.LOCALUP_AUTHTOKEN,
  transport: 'websocket',
  relay: 'relay.example.com:443',
});
```

### TCP Tunnel

```typescript
const listener = await localup.forward({
  addr: 5432,
  authtoken: process.env.LOCALUP_AUTHTOKEN,
  proto: 'tcp',
  transport: 'quic',
});

console.log(`PostgreSQL accessible at ${listener.url()}`);
```

### With Express

```typescript
import express from 'express';
import localup from '@localup/sdk';

const app = express();
app.get('/', (req, res) => res.json({ message: 'Hello!' }));

const server = app.listen(3000);

const listener = await localup.forward({
  addr: 3000,
  authtoken: process.env.LOCALUP_AUTHTOKEN,
  domain: 'my-express-app',
  transport: 'quic',
});

console.log(`App available at: ${listener.url()}`);
```

## Transport Protocols

The SDK supports three transport protocols:

| Protocol | Description | Installation |
|----------|-------------|--------------|
| `quic` | Best performance (UDP-based, multiplexed) | `npm install @matrixai/quic` |
| `websocket` | Good compatibility, works through firewalls | Built-in |
| `h2` | HTTP/2 based, maximum compatibility | Built-in |

### QUIC Transport

QUIC provides the best performance but requires native bindings:

```bash
npm install @matrixai/quic
```

```typescript
// Check if QUIC is available
import { isQuicAvailable, getQuicUnavailableReason } from '@localup/sdk';

if (await isQuicAvailable()) {
  console.log('QUIC transport available!');
} else {
  console.log('QUIC not available:', getQuicUnavailableReason());
  console.log('Falling back to WebSocket...');
}
```

Note: `@matrixai/quic` uses Cloudflare's quiche library and requires native compilation. It may not work on all platforms.

## Development

```bash
# Install dependencies
bun install

# For QUIC support
bun add @matrixai/quic

# Run tests
bun test

# Build
bun run build

# Run examples
bun run examples/basic.ts
```

## Testing Against a Relay

To test the SDK against a real LocalUp relay:

### 1. Start a local HTTP server (the app to expose)

```bash
python3 -m http.server 8080
```

### 2. Generate a JWT token

```bash
cargo run -p localup-cli -- generate-token \
  --secret "test-secret-key" \
  --localup-id "test-sdk"
```

### 3. Run the test script

```bash
# With QUIC (requires @matrixai/quic)
LOCALUP_AUTHTOKEN="<paste-token-here>" \
LOCALUP_RELAY="tunnel.kfs.es:4443" \
LOCALUP_TRANSPORT="quic" \
bun run examples/test-against-relay.ts

# With WebSocket (no extra dependencies)
LOCALUP_AUTHTOKEN="<paste-token-here>" \
LOCALUP_RELAY="localhost:4443" \
LOCALUP_TRANSPORT="websocket" \
bun run examples/test-against-relay.ts
```

## Protocol Details

The SDK communicates with the relay using:
- **Wire format**: Length-prefixed bincode (Rust binary format)
- **Message frame**: `[4-byte BE length][bincode payload]`

## License

MIT
