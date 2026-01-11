# Tunnel API

REST API server for managing and monitoring tunnel connections with OpenAPI documentation.

## Features

- ✅ **OpenAPI 3.0** specification auto-generated from code
- ✅ **Type-safe** endpoints with utoipa annotations
- ✅ **CORS** support for development
- ✅ **RESTful** design following best practices
- ✅ **JSON** request/response format

## API Endpoints

### Tunnels

- `GET /api/tunnels` - List all active tunnels
- `GET /api/tunnels/{id}` - Get tunnel details
- `DELETE /api/tunnels/{id}` - Delete/disconnect a tunnel
- `GET /api/tunnels/{id}/metrics` - Get tunnel metrics

### Traffic Inspector

- `GET /api/requests` - List captured requests
- `GET /api/requests/{id}` - Get request details
- `POST /api/requests/{id}/replay` - Replay a request

### System

- `GET /api/health` - Health check endpoint
- `GET /api/openapi.json` - OpenAPI specification

## Usage

### Starting the API Server

```rust
use localup_api::{ApiServer, ApiServerConfig};
use localup_control::TunnelConnectionManager;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create tunnel manager
    let localup_manager = Arc::new(TunnelConnectionManager::new());

    // Configure API server
    let config = ApiServerConfig {
        bind_addr: "0.0.0.0:8080".parse()?,
        enable_cors: true,
        cors_origins: Some(vec!["http://localhost:3000".to_string()]),
    };

    // Start server
    let server = ApiServer::new(config, localup_manager);
    server.start().await?;

    Ok(())
}
```

### Convenience Function

```rust
use localup_api::run_api_server;

// Simple way to start API server
run_api_server("0.0.0.0:8080".parse()?, localup_manager).await?;
```

## OpenAPI Documentation

The API automatically generates an OpenAPI 3.0 specification available at `/api/openapi.json`.

### Viewing the Spec

```bash
curl http://localhost:8080/api/openapi.json
```

### Using with Swagger UI

```bash
docker run -p 8081:8080 -e SWAGGER_JSON_URL=http://localhost:8080/api/openapi.json \
    swaggerapi/swagger-ui
```

Then visit: http://localhost:8081

### Generating Client Code

The OpenAPI spec can be used to generate type-safe clients:

```bash
# TypeScript client (for dashboard)
cd webapps/dashboard
bun run generate:api

# Python client
openapi-generator-cli generate -i http://localhost:8080/api/openapi.json \
    -g python -o ./python-client

# Go client
openapi-generator-cli generate -i http://localhost:8080/api/openapi.json \
    -g go -o ./go-client
```

## Development

### Adding a New Endpoint

1. **Define the model** in `src/models.rs`:
```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MyResource {
    pub id: String,
    pub name: String,
}
```

2. **Implement the handler** in `src/handlers.rs`:
```rust
#[utoipa::path(
    get,
    path = "/api/resources/{id}",
    responses(
        (status = 200, description = "Resource found", body = MyResource),
        (status = 404, description = "Not found", body = ErrorResponse)
    ),
    tag = "resources"
)]
pub async fn get_resource(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MyResource>, (StatusCode, Json<ErrorResponse>)> {
    // Implementation
}
```

3. **Register in OpenAPI** in `src/lib.rs`:
```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // ... existing paths
        handlers::get_resource,
    ),
    components(
        schemas(
            // ... existing schemas
            models::MyResource,
        )
    )
)]
struct ApiDoc;
```

4. **Add to router** in `src/lib.rs`:
```rust
.routes(utoipa_axum::routes!(
    // ... existing routes
    handlers::get_resource,
))
```

## Testing

```bash
# Build
cargo build -p localup-api

# Test
cargo test -p localup-api

# Check OpenAPI generation
cargo test -p localup-api test_openapi_generation
```

## CORS Configuration

For development, CORS is enabled by default and allows `http://localhost:3000`.

For production, configure allowed origins:

```rust
let config = ApiServerConfig {
    enable_cors: true,
    cors_origins: Some(vec![
        "https://dashboard.yourtunnel.com".to_string(),
    ]),
    ..Default::default()
};
```

## Error Handling

All errors are returned in a consistent format:

```json
{
  "error": "Human-readable error message",
  "code": "ERROR_CODE"
}
```

Status codes:
- `200` - Success
- `204` - Success (no content)
- `400` - Bad request
- `404` - Not found
- `500` - Internal server error
- `501` - Not implemented

## Future Enhancements

- [ ] WebSocket support for real-time updates
- [ ] Request/response capture and replay
- [ ] Authentication middleware
- [ ] Rate limiting
- [ ] Metrics export (Prometheus)
- [ ] GraphQL endpoint (alternative to REST)

## Dependencies

- **axum 0.8** - Web framework
- **utoipa 5.0** - OpenAPI code generation
- **utoipa-axum 0.2** - Axum integration for utoipa
- **tower-http** - Middleware (CORS, tracing)
- **serde** - JSON serialization

## See Also

- [Web Dashboard](../../webapps/dashboard/) - Frontend that consumes this API
- [CLAUDE.md](../../CLAUDE.md#web-applications) - Development guidelines
- [utoipa documentation](https://docs.rs/utoipa/)
- [Axum documentation](https://docs.rs/axum/)
