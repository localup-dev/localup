# Tunnel Exit Node Portal

Web-based management portal for monitoring and managing tunnel exit nodes.

## Features

- **Real-time Tunnel Monitoring**: View all active tunnels with their status, endpoints, and connection details
- **HTTP Traffic Inspection**: Monitor HTTP requests passing through each tunnel (method, path, status code, latency)
- **TCP Connection Tracking**: View TCP connections with bytes sent/received metrics
- **Tunnel Management**: Delete tunnels directly from the UI
- **Auto-refresh**: Data automatically refreshes every 3-5 seconds

## Tech Stack

- React 19 with TypeScript
- Tailwind CSS v4
- Vite 7
- Bun package manager
- Type-safe API client generation with `@hey-api/openapi-ts`

## Development

### Prerequisites

- Bun installed
- Tunnel exit node running on `localhost:3080` (API port)

### Setup

```bash
bun install
```

### Generate API Client

Generate TypeScript types from the OpenAPI spec:

```bash
bunx @hey-api/openapi-ts
```

### Development Server

```bash
bun run dev
```

The portal will run on `http://localhost:3001` and proxy API requests to `http://localhost:3080`.

### Build for Production

```bash
bun run build
```

Output will be in the `dist/` directory.

## Embedded Deployment

The portal is automatically embedded into the `localup-exit-node` binary using `rust-embed`. When you build the exit node binary, the latest built portal assets are included.

To rebuild the portal and update the embedded version:

```bash
# Build the portal
cd webapps/exit-node-portal
bun run build

# Build the exit node with embedded portal
cd ../..
cargo build -p localup-exit-node --release
```

## Usage

Once the exit node is running, access the portal at:

- Portal UI: `http://localhost:3080/` (or your configured API address)
- API Docs: `http://localhost:3080/swagger-ui`
- OpenAPI Spec: `http://localhost:3080/api/openapi.json`

## Project Structure

```
src/
├── App.tsx              # Main application component
├── index.css            # Tailwind imports
├── main.tsx             # Application entry point
└── api/
    └── generated/       # Auto-generated API client (don't edit)
```

## API Integration

The portal communicates with the localup-exit-node API:

- `GET /api/tunnels` - List all active tunnels
- `GET /api/requests?localup_id={id}` - Get HTTP requests for a tunnel
- `GET /api/tcp-connections?localup_id={id}` - Get TCP connections for a tunnel
- `DELETE /api/tunnels/{id}` - Delete a tunnel

All endpoints are documented in the Swagger UI.
