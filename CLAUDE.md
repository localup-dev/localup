# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a **geo-distributed tunnel library** written in Rust that enables developers to expose local servers to the internet through geographically distributed exit nodes. The system supports multiple protocols (TCP, TLS with SNI, HTTP, HTTPS) with automatic HTTPS certificates, wildcard domains, and QUIC-based multiplexing.

**Key Concept:** The system uses QUIC multiplexing to handle multiple logical streams over a single physical connection, reducing overhead and improving performance.

## Workspace Structure

This is a Rust workspace with 12 focused crates, each with a single responsibility:

- **`localup-proto`**: Protocol definitions, message types, frame format, codec
- **`localup-client`**: Public client library API (main entry point for users)
- **`localup-connection`**: Connection management, QUIC transport, multiplexing, reconnection
- **`localup-auth`**: Authentication and JWT handling
- **`localup-router`**: Routing logic (TCP port-based, SNI-based, HTTP host-based)
- **`localup-server-tcp`**: TCP tunnel server implementation
- **`localup-server-tls`**: TLS/SNI tunnel server with passthrough (no termination)
- **`localup-server-https`**: HTTPS server with TLS termination and HTTP/1.1, HTTP/2
- **`localup-cert`**: Certificate management, ACME/Let's Encrypt integration
- **`localup-control`**: Control plane orchestration, tunnel registry, exit node selection
- **`localup-exit-node`**: Exit node orchestrator that coordinates all server types
- **`localup-cli`**: Command-line tool for users
- **`localup-relay-db`**: Database layer using SeaORM for request/response storage and traffic inspection
- **`localup-api`**: REST API with OpenAPI documentation for managing tunnels and viewing traffic

## Common Commands

### Build
```bash
# Build entire workspace
cargo build

# Build specific crate
cargo build -p localup-client

# Build with release optimizations
cargo build --release
```

### Testing
```bash
# Run all tests in workspace
cargo test

# Run tests for specific crate
cargo test -p localup-proto

# Run specific test
cargo test test_tcp_localup_basic

# Run tests with output
cargo test -- --nocapture
```

### Linting and Formatting
```bash
# Check formatting
cargo fmt --all -- --check

# Format code
cargo fmt --all

# Run clippy (CI configuration - treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings

# Fix clippy warnings automatically
cargo clippy --fix
```

**IMPORTANT**: After modifying any code, you MUST run both linting commands to ensure CI will pass:
```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

These are the exact commands used in the CI workflow (`.github/workflows/ci.yml`). All code must pass both checks before committing.

### Running
```bash
# Run exit node binary (defaults to in-memory SQLite)
cargo run -p localup-exit-node

# Run exit node with persistent SQLite
cargo run -p localup-exit-node -- --database-url "sqlite://./tunnel.db?mode=rwc"

# Run exit node with PostgreSQL
cargo run -p localup-exit-node -- --database-url "postgres://user:pass@localhost/localup_db"

# Run CLI tool
cargo run -p localup-cli -- --help
```

## Architecture Overview

### Protocol Flow

The system uses a three-tier architecture:

1. **Client** (library integrated into user's app)
   - Connects to control plane for tunnel registration
   - Establishes QUIC connection to assigned exit node
   - Multiplexes multiple streams over single connection
   - Forwards requests to local server (localhost:PORT)

2. **Control Plane** (central orchestration)
   - Handles tunnel registration and authentication
   - Selects optimal exit node based on geo-location
   - Manages DNS and subdomain allocation
   - Monitors exit node health

3. **Exit Node** (edge servers)
   - Accepts external connections (TCP/TLS/HTTP/HTTPS)
   - Routes to appropriate tunnel based on port/SNI/host
   - Handles TLS termination (for HTTPS) or passthrough (for TLS)
   - Manages ACME certificates automatically

### Multiplexing Architecture

All communication uses **QUIC multiplexing** where a single QUIC connection carries multiple logical streams:

```
┌─────────────────────────────────────────────────────────────┐
│                    QUIC Connection                           │
├─────────────────────────────────────────────────────────────┤
│  Stream 0: Control    │  Stream 1: TCP     │  Stream 2: HTTP│
│  (Connect, Ping)      │  (TcpConnect,Data) │  (Request,Resp)│
├───────────────────────┼────────────────────┼────────────────┤
│  Stream 3: TCP        │  Stream N...       │                │
│  (TcpConnect,Data)    │  (one per request) │                │
└─────────────────────────────────────────────────────────────┘
```

**Stream 0 (Control)**: ONLY for tunnel registration (Connect/Connected) and heartbeat (Ping/Pong). After registration, this stream is used minimally.

**Data Streams (1...N)**: Each TCP connection or HTTP request gets its **own independent QUIC stream**. This provides:
- ✅ Natural isolation - one slow request doesn't block others
- ✅ No mutexes needed - each stream is independent
- ✅ Better performance - parallel streams utilize available bandwidth
- ✅ Proper flow control - per-stream backpressure

**Critical Architecture Notes**:
- Exit node calls `connection.open_stream()` to create a new stream for each incoming TCP/HTTP connection
- Client calls `connection.accept_stream()` in a loop to receive and handle these streams
- Each stream's lifetime matches the underlying TCP connection or HTTP request/response lifecycle
- **DO NOT** send data messages (TcpConnect, TcpData, HttpRequest, HttpResponse) on the control stream - they belong on dedicated data streams

### Protocol-Specific Flows

**TCP**: Port-based routing, bidirectional proxy
**TLS/SNI**: SNI extraction from ClientHello, TLS passthrough (no termination), end-to-end encryption
**HTTP**: Plain HTTP, host-based routing
**HTTPS**: TLS termination at exit node, HTTP/1.1 and HTTP/2 support, WebSocket upgrade support

## Key Design Decisions

### Why Three Separate Server Crates?
Each protocol (TCP, TLS, HTTPS) has its own crate for:
- Single Responsibility Principle
- Independent development and testing
- Clear protocol-specific optimizations
- Deployment flexibility

### Why QUIC Only?
QUIC provides:
- Built-in multiplexing (no custom mux layer needed)
- 0-RTT connection establishment
- Better mobile/unreliable network performance
- Built-in TLS 1.3 security
- Per-stream flow control

Trade-off: Some corporate firewalls may block UDP/QUIC.

## Code Organization Patterns

### Error Handling
Use `thiserror` for custom error types. Each crate defines its own error types with proper context.

### Async Runtime
Everything uses Tokio. Never use `std::sync` primitives - use `tokio::sync` instead.

### Message Types
Protocol messages are defined in `localup-proto/src/messages.rs` and serialized using `bincode`. All messages implement `Serialize` and `Deserialize`.

### Axum Routing (0.8+)

**Path Parameter Syntax**: Axum 0.8+ requires `{param}` syntax for path parameters, not `:param` from older versions.

```rust
// ✅ Correct (Axum 0.8+)
.route("/api/tunnels/{id}", get(handlers::get_tunnel))
.route("/api/requests/{id}/replay", post(handlers::replay_request))

// ❌ Wrong (will panic with "Path segments must not start with `:`")
.route("/api/tunnels/:id", get(handlers::get_tunnel))
.route("/api/requests/:id/replay", post(handlers::replay_request))
```

**Router Composition**: When integrating multiple routers (e.g., API + Swagger UI), use `.merge()`:

```rust
let api_router = Router::new()
    .route("/api/tunnels", get(handlers::list_tunnels))
    .with_state(state);

let router = Router::new()
    .merge(api_router)
    .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", api_doc));
```

### Code Quality Standards

**Zero Warnings Policy**: All code must compile without warnings. Before committing or completing features:

```bash
# Verify no warnings across all targets
cargo build --all-targets 2>&1 | grep "warning:" && echo "Fix warnings!" || echo "✓ Clean build"
```

When addressing warnings, always understand **why** they exist:
- **Unused code**: Remove if genuinely dead, or prefix with `_` if intentionally unused
- **Placeholder code**: Mark future features with `#[allow(dead_code)]` or `#[allow(unused_imports)]` with comments explaining intent
- **Duplicate code**: Refactor to avoid duplication
- **Incomplete logic**: Fix the underlying issue rather than suppressing the warning

### Testing Strategy

**Test Coverage Requirements**:

- **All crates**: ≥75% test coverage (minimum, non-negotiable)
- **Core libraries**: >90% test coverage (localup-transport, localup-proto, localup-router, localup-auth, localup-relay-db)

#### Test Types

- **Unit tests**: Per-crate in each `src/` file (or `src/tests.rs` module)
  - Test individual functions, methods, and components in isolation
  - Use mocks for dependencies
  - Fast execution, no I/O

- **Integration tests**: In crate `tests/` directory (**MANDATORY for all crates with public APIs**)
  - Test real user workflows from a client perspective
  - Test component interactions end-to-end
  - Simulate actual usage scenarios
  - Must cover: authentication flows, multi-protocol scenarios, error recovery, concurrent operations

- **Protocol-specific tests**: `tcp_server_test.rs`, `tls_server_test.rs`, `https_server_test.rs`

#### Running Tests

```bash
# Run all tests in workspace
cargo test

# Run tests for specific crate
cargo test -p localup-transport
cargo test -p localup-transport-quic

# Run only unit tests
cargo test --lib -p localup-transport

# Run only integration tests
cargo test --test integration -p localup-transport-quic

# Run with output
cargo test -- --nocapture

# Run single-threaded (for QUIC tests)
cargo test -- --test-threads=1
```

#### Coverage Checking

To check test coverage, use `cargo-tarpaulin`:

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Check coverage for a specific crate
cargo tarpaulin -p localup-transport --out Stdout

# Check coverage for multiple crates
cargo tarpaulin -p localup-transport -p localup-transport-quic --out Html
```

#### Test Guidelines

- **All new features must include BOTH unit AND integration tests**
- **Critical paths should have >90% coverage**
- **Mock implementations** should be in `src/tests.rs` or `tests/common/mod.rs`
- **Integration tests are mandatory** for all crates with public APIs
  - Must test from user perspective (how would a developer use this crate?)
  - Must test real component interactions, not just mocks
  - Must cover error scenarios and edge cases
  - Example: For `localup-control`, test actual TCP connections, message serialization, authentication flows
- **Use `#[tokio::test]`** for async tests
- **For QUIC tests**, certificates in workspace root (`cert.pem`, `key.pem`) are used

#### Integration Test Requirements (MANDATORY)

Every crate with a public API **must** have integration tests in `tests/` directory covering:

1. **Happy Path Scenarios**
   - Basic successful operation
   - Multiple protocol/configuration variants
   - Concurrent operations

2. **Authentication & Authorization** (if applicable)
   - Valid credentials → success
   - Invalid credentials → rejection
   - Token expiration → proper error handling

3. **Error Recovery & Edge Cases**
   - Network failures (connection refused, timeout, reset)
   - Invalid input (malformed messages, wrong types)
   - Resource exhaustion (port conflicts, memory limits)
   - Concurrent access (race conditions, deadlocks)

4. **Real Component Integration**
   - Actual TCP/HTTP connections, not mocks
   - Real message serialization/deserialization
   - Real database operations (with in-memory DB)
   - Real async runtime behavior

**Example Structure** (`crates/localup-control/tests/integration.rs`):
```rust
#[tokio::test]
async fn test_basic_http_localup_connection() {
    // Setup: Start real TCP server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Act: Connect as client and send real messages
    let mut client = TcpStream::connect(addr).await.unwrap();
    send_message(&mut client, TunnelMessage::Connect { ... }).await;

    // Assert: Verify real responses
    let response = recv_message(&mut client).await.unwrap();
    assert!(matches!(response, TunnelMessage::Connected { .. }));
}
```

**What NOT to do**:
- ❌ Only mocking all dependencies (that's a unit test)
- ❌ Only testing internal implementation details
- ❌ Ignoring error paths
- ❌ Skipping integration tests because "unit tests pass"

#### Error Recovery Testing (CRITICAL)

**Always test error recovery paths with custom service errors.** This is non-negotiable for production readiness.

Error scenarios to test:

1. **Network Errors**
   - Connection timeout
   - Connection refused
   - Connection lost/reset
   - Partial writes
   - Read errors

2. **Protocol Errors**
   - Invalid message format
   - Message too large
   - Unexpected message type
   - Protocol version mismatch

3. **Resource Errors**
   - Out of memory
   - File descriptor exhaustion
   - Port already in use
   - Certificate not found

4. **Application Errors**
   - Invalid configuration
   - Authentication failure
   - Authorization failure
   - Rate limit exceeded

5. **Recovery Behaviors**
   - Graceful degradation
   - Retry with backoff
   - Circuit breaker patterns
   - Cleanup on failure

Example error recovery test:

```rust
#[tokio::test]
async fn test_connection_recovery_on_timeout() {
    let config = ClientConfig::default()
        .with_timeout(Duration::from_millis(100));

    let client = Client::new(config);

    // Simulate slow server that times out
    let result = client.connect("slow-server:8080").await;

    // Verify timeout error
    assert!(matches!(result, Err(TransportError::Timeout)));

    // Verify client can recover and retry
    let result = client.connect("fast-server:8080").await;
    assert!(result.is_ok());

    // Verify no resource leaks
    assert_eq!(client.active_connections(), 1);
}

#[tokio::test]
async fn test_graceful_degradation_on_cert_error() {
    let config = ServerConfig::default()
        .with_cert_path("nonexistent.pem");

    // Should fail gracefully with clear error
    let result = Server::new(config);
    assert!(matches!(result, Err(ServerError::CertificateNotFound(_))));

    // Error should be descriptive
    let err = result.unwrap_err();
    assert!(err.to_string().contains("nonexistent.pem"));
}
```

#### Custom Error Types

Define domain-specific error types with context:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TunnelError {
    #[error("Connection timeout after {timeout:?}: {context}")]
    Timeout {
        timeout: Duration,
        context: String
    },

    #[error("Certificate error: {0}")]
    Certificate(#[from] CertificateError),

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted {
        attempts: u32,
        last_error: Box<dyn Error + Send + Sync>,
    },
}
```

#### Test Structure Example

```rust
// Unit tests in src/module.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Test here
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Async test here
    }
}

// Integration tests in tests/integration.rs
use localup_transport::*;

#[tokio::test]
async fn test_end_to_end_flow() {
    // Full integration test
}
```

#### Coverage Requirements by Crate Category

Different crate types have different coverage requirements based on their criticality:

**Tier 1: Core Infrastructure (≥80% required)**
- `localup-transport` - Transport abstraction layer
- `localup-proto` - Protocol definitions
- `localup-router` - Routing logic
- `localup-auth` - Authentication/authorization
- `localup-relay-db` - Database layer (CRITICAL: currently 0%)
- `localup-exit-node` - Orchestration (CRITICAL: currently 0%)

**Tier 2: Server Components (≥60% required)**
- `localup-server-tcp` - TCP server
- `localup-server-tls` - TLS/SNI server
- `localup-server-https` - HTTPS server
- `localup-control` - Control plane
- `localup-connection` - Connection management

**Tier 3: Client & Tools (≥50% required)**
- `localup-client` - Client library
- `localup-cli` - CLI tool
- `localup-cert` - Certificate management
- `localup-api` - REST API

**Current Status (as of last check)**:

| Crate | Current | Target | Status |
|-------|---------|--------|--------|
| localup-transport | 95% | 80% | ✅ Exceeds |
| localup-transport-quic | 72% | 75% | ⚠️ Close |
| localup-router | 80% | 80% | ✅ Meets |
| localup-proto | 75% | 80% | ⚠️ Close |
| localup-auth | 80% | 80% | ✅ Meets |
| localup-cli | 85% | 50% | ✅ Exceeds |
| localup-cert | 70% | 50% | ✅ Exceeds |
| **localup-relay-db** | **0%** | **80%** | ❌ **BLOCKER** |
| **localup-exit-node** | **0%** | **80%** | ❌ **BLOCKER** |
| localup-control | 20% | 60% | ❌ Insufficient |
| localup-connection | 15% | 60% | ❌ Insufficient |
| localup-client | 50% | 50% | ✅ Meets |
| localup-api | 25% | 50% | ❌ Insufficient |

**Total workspace tests: 103** (102 passing, 1 failing benchmark)

## Important Constants

- `PROTOCOL_VERSION`: Current protocol version (defined in `localup-proto`)
- `MAX_FRAME_SIZE`: 16MB maximum frame size
- `CONTROL_STREAM_ID`: Stream 0 reserved for control messages

## Dependencies

Key dependencies used across workspace:
- **Async**: `tokio`, `futures`, `async-trait`
- **Networking**: `hyper`, `quinn` (QUIC), `rustls` (TLS)
- **Serialization**: `serde`, `bincode`, `serde_json`
- **ACME**: `instant-acme` (Let's Encrypt integration)
- **Auth**: `jsonwebtoken`, `base64`
- **Database**: `sea-orm` (PostgreSQL, SQLite3), `sea-orm-migration`
- **Web/API**: `axum`, `tower`, `tower-http`, `utoipa`, `utoipa-swagger-ui`
- **Utilities**: `bytes`, `thiserror`, `anyhow`, `tracing`

## Database

The system uses **SeaORM** for database operations, supporting multiple backends:

### Exit Nodes (Production)
- **PostgreSQL with TimescaleDB** (recommended): Optimized for time-series data
  ```bash
  --database-url "postgres://user:pass@localhost/localup_db"
  ```
- **PostgreSQL**: Standard relational database without TimescaleDB
- **SQLite3**: Lightweight option for development or small deployments
  ```bash
  --database-url "sqlite://./tunnel.db?mode=rwc"
  ```

### Exit Nodes (Development/Testing)
- **In-memory SQLite** (default): No persistence, data lost on restart
  ```bash
  # Automatic if --database-url not specified
  cargo run -p localup-exit-node
  ```

### Clients
- **Ephemeral SQLite**: In-memory storage for local request history
  ```
  "sqlite::memory:"
  ```

### Schema

The `localup-relay-db` crate contains:
- **Entities**: SeaORM models (e.g., `CapturedRequest`)
- **Migrations**: Automatic schema setup with `sea-orm-migration`
- **TimescaleDB support**: Automatic hypertable creation for PostgreSQL (if extension available)

Migrations run automatically on startup. The `captured_requests` table stores:
- Full HTTP request/response data (headers, body, status)
- Timestamps for time-series queries
- Latency metrics
- Indexes on `localup_id` and `created_at`

### Reconnection Support

Both port allocations (TCP) and route registrations (HTTP/HTTPS subdomains) use a **reservation system** with TTL:

- **On disconnect**: Resources are marked as "reserved" (default: 5 minutes TTL)
- **On reconnect**: If the same `localup_id` reconnects within the TTL window, it receives the same port/subdomain
- **After TTL expires**: A background cleanup task frees the resources for reuse

This ensures clients can reconnect with the same public URLs after temporary network interruptions.

## Development Workflow

### Adding a New Feature
1. Identify which crate(s) the feature belongs to
2. Update protocol messages if needed (`localup-proto`)
3. Implement in appropriate crate(s)
4. Add unit tests in the same file
5. Add integration tests in `tests/` directory
6. Update documentation

### Adding a New Protocol
1. Define message types in `localup-proto/src/messages.rs`
2. Add routing logic in `localup-router`
3. Create new server crate `localup-server-{protocol}`
4. Integrate with exit node orchestrator
5. Add client-side support in `localup-client`

## Web Applications

The project includes web-based dashboards and management interfaces built with modern web technologies.

### Structure

```
webapps/
├── dashboard/         # Main tunnel management dashboard
└── [future-apps]/     # Additional web applications
```

### Tech Stack Requirements

All web applications must use:

- **Package Manager**: Bun (not npm or yarn)
- **Framework**: React 19+ with TypeScript
- **Build Tool**: Vite 7+
- **Styling**: Tailwind CSS v4 (with `@tailwindcss/vite` plugin)
- **API Client**: `@hey-api/openapi-ts` for type-safe API generation

### Development Workflow

#### 1. Setup
```bash
cd webapps/dashboard
bun install
```

#### 2. Generate API Client
The backend must expose OpenAPI spec at `/api/openapi.json`. Generate the TypeScript client:

```bash
bun run generate:api
```

This creates type-safe API clients in `src/api/generated/`.

#### 3. Development
```bash
bun run dev          # Start dev server (port 3000)
bun run type-check   # Type checking
bun run lint         # Linting
bun run build        # Production build
```

### Backend Integration Requirements

For webapps to work, the Rust backend must:

1. **Use `utoipa` with Axum 0.8+** for OpenAPI documentation
2. **Expose OpenAPI spec** at `/api/openapi.json`
3. **Serve API** on port 8080 (configurable)
4. **CORS configuration** for development (allow localhost:3000)

Example Rust setup:

```rust
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(
    paths(list_tunnels, create_tunnel),
    components(schemas(Tunnel, TunnelConfig))
)]
struct ApiDoc;

let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
    .routes(routes!(list_tunnels, create_tunnel))
    .split_for_parts();

// Serve OpenAPI spec
let app = router.route("/api/openapi.json", get(|| async {
    Json(api)
}));
```

### Web Application Standards

#### File Structure
```
src/
├── api/
│   ├── generated/     # Auto-generated (DO NOT EDIT)
│   └── client.ts      # Client configuration
├── components/        # React components
│   ├── TunnelList.tsx
│   └── TrafficInspector.tsx
├── hooks/             # Custom hooks
│   ├── useTunnels.ts
│   └── useWebSocket.ts
├── types/             # Additional TypeScript types
├── App.tsx            # Root component
├── main.tsx           # Entry point
└── index.css          # Tailwind imports only
```

#### TypeScript Configuration
- **Strict mode**: Enabled
- **No `any` types**: Use proper types or `unknown`
- **Import paths**: Relative or via `@/` alias

#### Styling Guidelines
- **Use Tailwind utilities**: Avoid custom CSS
- **Responsive design**: Use Tailwind breakpoints (`sm:`, `md:`, `lg:`)
- **Dark mode**: Use Tailwind's dark mode classes when needed
- **Components**: Extract repeated patterns into React components

#### API Client Usage
```typescript
// ✅ Good: Use generated types
import { TunnelService, type Tunnel } from './api/generated';

const tunnels: Tunnel[] = await TunnelService.listTunnels();

// ❌ Bad: Manual fetch without types
const response = await fetch('/api/tunnels');
const tunnels = await response.json();
```

#### State Management
- **Local state**: `useState` for component-local state
- **Server state**: React Query or similar (when needed)
- **WebSocket**: Custom hooks for real-time updates

#### Build Output
- Development: Served by Vite dev server (port 3000)
- Production: Built to `dist/`, embedded in Rust binary using `include_bytes!`

### Adding a New Web Application

1. Create directory: `webapps/new-app/`
2. Initialize with Vite:
   ```bash
   cd webapps
   bun create vite new-app --template react-ts
   cd new-app
   bun install
   bun add -d @tailwindcss/vite@next tailwindcss@next @hey-api/openapi-ts
   bun add @hey-api/client-fetch
   ```
3. Configure Tailwind in `vite.config.ts`
4. Create `openapi-ts.config.ts`
5. Update `src/index.css` to `@import "tailwindcss";`
6. Follow the structure and standards above

### Deployment

Web applications can be deployed in multiple ways:

1. **Embedded** (recommended for self-hosted):
   - Build assets are bundled into Rust binary
   - Served directly by Axum from memory
   - Zero external dependencies

2. **Separate hosting**:
   - Build `dist/` and deploy to Vercel/Netlify/Cloudflare Pages
   - Configure API URL via environment variables

3. **Docker**:
   - Multi-stage build: frontend + backend
   - Serve static files from Rust or nginx

## Implementation Status

This project is in active development. The core crates have been scaffolded with basic structure. Refer to SPEC.md for the complete technical specification and implementation phases.

Current milestone: Phase 1-2 (Core protocol and TCP tunnel implementation)

## Security Notes

- All public-facing connections use TLS 1.3
- Tunnel connections use QUIC (built-in TLS 1.3)
- Authentication uses JWT tokens
- ACME integration for automatic certificate management
- Rate limiting and IP allowlisting supported

## Performance Targets

- Additional latency: < 50ms for same-region routing
- Tunnel establishment: < 2 seconds
- Support: 1000+ concurrent connections per tunnel
- Throughput: 10,000+ requests/second per exit node

## localup-lib: Public API Crate

**`localup-lib`** is the high-level public API crate for Rust applications that want to integrate tunnel functionality. It re-exports all the focused crates, providing a unified entry point.

### Purpose

- **For tunnel clients**: Use `TunnelClient` directly from `localup-lib` instead of importing from `localup-client`
- **For custom relays**: Build custom relay servers using the server components (`TunnelHandler`, `HttpsServer`, etc.)
- **Single dependency**: Applications only need to add `localup-lib` instead of multiple crate dependencies

### Maintenance Guidelines

**IMPORTANT**: `localup-lib` must be kept up-to-date whenever you make changes to other crates. This is a **MANDATORY** requirement.

1. **When adding new public types to any crate**, add the re-export to [localup-lib/src/lib.rs](crates/localup-lib/src/lib.rs)
2. **When removing/renaming public types**, update the re-exports accordingly
3. **After any API changes**, run `cargo build -p localup-lib` to ensure it compiles
4. **Only re-export public types** - do not re-export internal/private types

### Structure

```rust
// localup-lib/src/lib.rs
pub use localup_client::{TunnelClient, TunnelConfig, ...};  // Client API
pub use localup_control::{TunnelHandler, ...};               // Relay API
pub use localup_server_https::{HttpsServer, ...};           // Server components
// ... etc
```

### Example Usage

```rust
// Cargo.toml
[dependencies]
localup-lib = { path = "../localup-lib" }

// main.rs
use localup_lib::{TunnelClient, ProtocolConfig, TunnelConfig};

let config = TunnelConfig {
    relay_addr: "localhost:4443".to_string(),
    auth_token: Some("token".to_string()),
    protocol: ProtocolConfig::Http {
        local_port: 3000,
        subdomain: Some("myapp".to_string()),
    },
    ..Default::default()
};

let client = TunnelClient::connect(config).await?;
client.wait().await?;
```

### Verification

Always verify `localup-lib` compiles after making changes:

```bash
cargo build -p localup-lib
cargo build --all-targets  # Ensure entire workspace compiles
```

**Zero warnings policy applies** to `localup-lib` just like all other crates.

## Documentation and File Organization

### Markdown Files

**Guideline**: All markdown files created during development without explicit user request should be placed in the `thoughts/` folder at the repository root.

This keeps the root directory clean while preserving internal documentation and analysis:

```
localup-dev/
├── thoughts/
│   ├── SNI_ANALYSIS.md          # Analysis and research notes
│   ├── ARCHITECTURE_NOTES.md    # Architecture discussions
│   ├── IMPLEMENTATION_PLAN.md   # Implementation planning
│   ├── TEST_SUMMARY.md          # Test documentation
│   └── [other-documentation]/   # Other internal docs
├── docs/                         # User-facing documentation
├── README.md                     # Project readme (root level, explicit)
├── CLAUDE.md                     # This file (root level, explicit)
└── [source files]/
```

**Exception**: User-requested documentation at the repository root (e.g., when user explicitly asks for a README or specific documentation file) may be placed at the root.

**Examples**:
- ✅ Internal SNI analysis → `thoughts/SNI_ANALYSIS.md`
- ✅ Test summaries → `thoughts/TEST_SUMMARY.md`
- ✅ Implementation notes → `thoughts/IMPLEMENTATION_NOTES.md`
- ✅ Exploration findings → `thoughts/CODEBASE_EXPLORATION.md`
- ❌ Root-level documentation without explicit request
