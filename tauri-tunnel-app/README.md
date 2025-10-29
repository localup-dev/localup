# Tunnel Manager - Desktop Application

A powerful **native desktop application** built with Tauri that provides comprehensive management and monitoring for geo-distributed tunnels using the `tunnel-lib` library.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)
![Tauri](https://img.shields.io/badge/Tauri-2.x-24c8db)
![React](https://img.shields.io/badge/React-19-61dafb)

## Features

### 🚀 Tunnel Management
- **Create Multiple Tunnels**: Configure and run multiple tunnels simultaneously
- **Multi-Protocol Support**: TCP, TLS (with SNI), HTTP, and HTTPS tunnels
- **Quick Start**: One-click tunnel creation with sensible defaults
- **Advanced Configuration**: Custom domains, subdomains, and port mappings
- **Lifecycle Control**: Start, stop, and monitor tunnel status in real-time

### 📊 Real-Time Monitoring
- **Live Traffic Inspection**: View HTTP requests/responses as they happen
  - Request/response headers and bodies (JSON/text)
  - Status codes, methods, URIs
  - Response times and error tracking
- **TCP Connection Tracking**: Monitor active TCP connections
  - Bytes sent/received
  - Connection duration and state
  - Remote/local addresses
- **Performance Metrics**:
  - Request latency percentiles (p50, p90, p95, p99)
  - Success/failure rates
  - Method and status code breakdowns
  - Average response times

### 🌍 Relay Management
- **Auto/Manual Selection**: Choose between automatic selection or specific exit nodes
- **Custom Relays**: Add user-defined relay servers
- **Failover Support**: Automatic retry on connection failures

## Prerequisites

- **Rust**: 1.75+ ([Install Rust](https://rustup.rs/))
- **Node.js**: 18+ (for frontend)
- **Bun**: Latest version ([Install Bun](https://bun.sh))
- **Operating System**: macOS, Windows, or Linux

## Installation

### 1. Clone the Repository

```bash
cd tauri-tunnel-app
```

### 2. Install Dependencies

```bash
# Install frontend dependencies
bun install

# Rust dependencies are managed automatically by Cargo
```

### 3. Development Build

```bash
# Start the Tauri development server
bun run tauri dev
```

This will:
1. Start the Vite dev server (port 1420)
2. Compile the Rust backend
3. Launch the desktop application

### 4. Production Build

```bash
# Create optimized production build
bun run tauri build
```

Built applications will be in `src-tauri/target/release/bundle/`:
- **macOS**: `.app` bundle and `.dmg` installer
- **Windows**: `.exe` and `.msi` installer
- **Linux**: `.AppImage`, `.deb`, `.rpm`

## Usage

### Creating Your First Tunnel

1. **Launch the Application**
2. **Click "New Tunnel"**
3. **Configure Tunnel Settings**:
   - **Name**: A friendly name for your tunnel
   - **Local Host**: Usually `localhost`
   - **Local Port**: The port your local server runs on (e.g., 3000)
   - **Protocol**: Choose HTTP, HTTPS, TCP, or TLS
   - **Subdomain** (optional): Custom subdomain for HTTP/HTTPS tunnels
4. **Authentication**:
   - Enter your auth token (obtain from your relay provider)
5. **Exit Node**:
   - **Auto**: Let the system choose
   - **Nearest**: Select closest exit node
   - **Custom**: Specify a relay address (e.g., `localhost:9000`)
6. **Click "Create Tunnel"**

Your tunnel will start immediately, and you'll see the public URL(s) displayed.

### Example Configurations

#### HTTP Development Server
```
Name: My Dev Server
Local Host: localhost
Local Port: 3000
Protocol: HTTP
Subdomain: myapp
Auth Token: your-auth-token
Exit Node: Custom (localhost:9000)
```

## Development

### Project Structure

```
tauri-tunnel-app/
├── src/                       # React frontend
│   ├── components/            # UI components
│   │   ├── TunnelList.tsx     # List of tunnels
│   │   ├── TunnelForm.tsx     # Create/edit tunnel
│   │   ├── MetricsDashboard.tsx  # Stats and charts
│   │   └── TrafficInspector.tsx  # HTTP/TCP traffic
│   ├── hooks/                 # React hooks
│   │   └── useTunnels.ts      # Tunnel state management
│   ├── types/                 # TypeScript types
│   │   └── tunnel.ts          # Type definitions
│   ├── App.tsx                # Main app component
│   └── main.tsx               # Entry point
├── src-tauri/                 # Rust backend
│   ├── src/
│   │   ├── lib.rs             # Tauri setup
│   │   ├── main.rs            # Entry point
│   │   ├── tunnel_manager.rs # Multi-tunnel orchestration
│   │   └── commands.rs        # Tauri IPC commands
│   └── Cargo.toml             # Rust dependencies
├── package.json               # Frontend dependencies
├── vite.config.ts             # Vite configuration
└── README.md                  # This file
```

### Tech Stack

**Backend**:
- Tauri 2.x (Rust framework)
- tunnel-lib (tunnel client library)
- tokio (async runtime)
- serde (serialization)

**Frontend**:
- React 19 (UI framework)
- TypeScript 5 (type safety)
- Tailwind CSS v4 (styling)
- @tanstack/react-query (server state)
- lucide-react (icons)
- Vite 7 (build tool)

## Architecture

The application uses a three-tier architecture:

1. **Frontend (React)**: User interface with real-time updates
2. **Backend (Tauri/Rust)**: IPC commands and tunnel management
3. **tunnel-lib**: Core QUIC-based tunnel implementation

```
Frontend (React) <--IPC--> Tauri Backend <--Rust--> tunnel-lib <--QUIC--> Exit Node
```

## License

MIT OR Apache-2.0 (same as parent project)

## Related Projects

- **[tunnel-lib](../crates/tunnel-lib/)**: Core Rust library
- **[tunnel-cli](../crates/tunnel-cli/)**: Command-line interface
- **[tunnel-exit-node](../crates/tunnel-exit-node/)**: Exit node/relay server
