# REST API Reference

The localup relay exposes a REST API for managing tunnels, inspecting traffic, managing domains, and authentication. The API is auto-documented with OpenAPI/Swagger.

---

## Getting Started

### Enable the API

The API server is enabled by default when running a relay. Configure the bind address:

```bash
localup relay http \
  --api-http-addr 0.0.0.0:8080 \
  --jwt-secret "my-secret"
```

For HTTPS:

```bash
localup relay http \
  --api-https-addr 0.0.0.0:8443 \
  --api-tls-cert /path/to/cert.pem \
  --api-tls-key /path/to/key.pem
```

To disable the API:

```bash
localup relay http --no-api
```

### OpenAPI / Swagger UI

- **OpenAPI spec**: `GET /api/openapi.json`
- **Swagger UI**: `GET /swagger-ui`

### Authentication

Most endpoints require a session token obtained via login:

```bash
# Login
curl -X POST http://relay:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@example.com", "password": "mypassword"}'

# Response includes a session token
# {"token": "session-token-here", "expires_at": "..."}

# Use the session token for subsequent requests
curl http://relay:8080/api/tunnels \
  -H "Authorization: Bearer session-token-here"
```

You can also use API keys (auth tokens) for programmatic access:

```bash
# Create an API key
curl -X POST http://relay:8080/api/auth-tokens \
  -H "Authorization: Bearer session-token-here" \
  -H "Content-Type: application/json" \
  -d '{"name": "CI/CD", "description": "Token for CI pipeline"}'

# Use the API key directly
curl http://relay:8080/api/tunnels \
  -H "Authorization: Bearer api-key-here"
```

---

## Endpoints

### Health Check

#### `GET /api/health`

No authentication required.

```bash
curl http://relay:8080/api/health
```

```json
{
  "status": "ok",
  "version": "0.1.9",
  "active_tunnels": 3
}
```

### Protocol Discovery

#### `GET /.well-known/localup-protocols`

No authentication required. Returns available transport protocols.

```bash
curl http://relay:8080/.well-known/localup-protocols
```

---

## Tunnel Management

### List Tunnels

#### `GET /api/tunnels`

```bash
# Active tunnels only
curl http://relay:8080/api/tunnels \
  -H "Authorization: Bearer $TOKEN"

# Include disconnected tunnels
curl "http://relay:8080/api/tunnels?include_inactive=true" \
  -H "Authorization: Bearer $TOKEN"

# Admin: see all users' tunnels
curl "http://relay:8080/api/tunnels?scope=all" \
  -H "Authorization: Bearer $TOKEN"
```

### Get Tunnel

#### `GET /api/tunnels/{id}`

```bash
curl http://relay:8080/api/tunnels/myapp \
  -H "Authorization: Bearer $TOKEN"
```

### Delete Tunnel

#### `DELETE /api/tunnels/{id}`

```bash
curl -X DELETE http://relay:8080/api/tunnels/myapp \
  -H "Authorization: Bearer $TOKEN"
```

### Tunnel Metrics

#### `GET /api/tunnels/{id}/metrics`

```bash
curl http://relay:8080/api/tunnels/myapp/metrics \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "total_requests": 1523,
  "requests_per_minute": 12.5,
  "avg_latency_ms": 45.2,
  "error_rate": 0.02,
  "total_bandwidth_bytes": 5242880
}
```

---

## Traffic Inspection

### List Captured Requests

#### `GET /api/requests`

Query HTTP requests captured by the relay.

```bash
# All requests for a tunnel
curl "http://relay:8080/api/requests?localup_id=myapp" \
  -H "Authorization: Bearer $TOKEN"

# Filter by method and status
curl "http://relay:8080/api/requests?localup_id=myapp&method=POST&status_min=400&status_max=599" \
  -H "Authorization: Bearer $TOKEN"

# Pagination
curl "http://relay:8080/api/requests?localup_id=myapp&offset=0&limit=50" \
  -H "Authorization: Bearer $TOKEN"
```

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `localup_id` | string | Filter by tunnel ID |
| `method` | string | Filter by HTTP method (GET, POST, etc.) |
| `path` | string | Filter by request path (partial match) |
| `status` | u16 | Exact status code |
| `status_min` | u16 | Minimum status code |
| `status_max` | u16 | Maximum status code |
| `offset` | usize | Pagination offset (default: 0) |
| `limit` | usize | Page size (default: 100, max: 1000) |
| `scope` | string | `"mine"` (default) or `"all"` (admin) |

### Get Captured Request

#### `GET /api/requests/{id}`

```bash
curl http://relay:8080/api/requests/550e8400-e29b-41d4-a716-446655440000 \
  -H "Authorization: Bearer $TOKEN"
```

Returns full request and response details including headers and body.

### Replay Request

#### `POST /api/requests/{id}/replay`

Re-send a previously captured request through the tunnel.

```bash
curl -X POST http://relay:8080/api/requests/550e8400-e29b-41d4-a716-446655440000/replay \
  -H "Authorization: Bearer $TOKEN"
```

### List TCP Connections

#### `GET /api/tcp-connections`

For non-HTTP tunnels (TCP, TLS).

```bash
curl "http://relay:8080/api/tcp-connections?localup_id=my-tcp-tunnel" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Custom Domains

### List Domains

#### `GET /api/domains`

```bash
curl http://relay:8080/api/domains \
  -H "Authorization: Bearer $TOKEN"
```

### Upload Certificate

#### `POST /api/domains`

Upload a custom TLS certificate for a domain.

```bash
curl -X POST http://relay:8080/api/domains \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain": "app.example.com",
    "cert_pem": "'$(base64 < cert.pem)'",
    "key_pem": "'$(base64 < key.pem)'",
    "auto_renew": true
  }'
```

### Get Domain Details

#### `GET /api/domains/{domain}`

```bash
curl http://relay:8080/api/domains/app.example.com \
  -H "Authorization: Bearer $TOKEN"
```

### Delete Domain

#### `DELETE /api/domains/{domain}`

```bash
curl -X DELETE http://relay:8080/api/domains/app.example.com \
  -H "Authorization: Bearer $TOKEN"
```

### Certificate Details

#### `GET /api/domains/{domain}/certificate-details`

Get X.509 certificate details (serial, issuer, expiration, SANs).

```bash
curl http://relay:8080/api/domains/app.example.com/certificate-details \
  -H "Authorization: Bearer $TOKEN"
```

---

## ACME / Let's Encrypt

Automated certificate provisioning via ACME. The relay must be started with `--acme-email`.

### Initiate Challenge

#### `POST /api/domains/challenge/initiate`

```bash
curl -X POST http://relay:8080/api/domains/challenge/initiate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"domain": "app.example.com", "challenge_type": "http-01"}'
```

Supported challenge types: `http-01`, `dns-01`.

### Pre-Validate Challenge

#### `POST /api/domains/challenge/pre-validate`

Check if your challenge setup is correct before ACME submission.

### Complete Challenge

#### `POST /api/domains/challenge/complete`

```bash
curl -X POST http://relay:8080/api/domains/challenge/complete \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"domain": "app.example.com", "challenge_id": "challenge-uuid"}'
```

### List Pending Challenges

#### `GET /api/domains/{domain}/challenges`

List pending ACME challenges for a domain.

```bash
curl http://relay:8080/api/domains/app.example.com/challenges \
  -H "Authorization: Bearer $TOKEN"
```

### Cancel Challenge

#### `POST /api/domains/{domain}/challenge/cancel`

Cancel an ongoing ACME challenge.

```bash
curl -X POST http://relay:8080/api/domains/app.example.com/challenge/cancel \
  -H "Authorization: Bearer $TOKEN"
```

### Restart Challenge

#### `POST /api/domains/{domain}/challenge/restart`

Restart a failed ACME challenge.

```bash
curl -X POST http://relay:8080/api/domains/app.example.com/challenge/restart \
  -H "Authorization: Bearer $TOKEN"
```

### Request Certificate

#### `POST /api/domains/{domain}/certificate`

After challenge validation, request the certificate from Let's Encrypt.

```bash
curl -X POST http://relay:8080/api/domains/app.example.com/certificate \
  -H "Authorization: Bearer $TOKEN"
```

### ACME HTTP-01 Challenge

#### `GET /.well-known/acme-challenge/{token}`

Serves ACME HTTP-01 challenge tokens. Called by Let's Encrypt servers during domain validation. No authentication required.

---

## Authentication

### Register

#### `POST /api/auth/register`

Registration must be enabled with `--allow-signup`.

```bash
curl -X POST http://relay:8080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "securepassword",
    "full_name": "Jane Doe"
  }'
```

### Login

#### `POST /api/auth/login`

```bash
curl -X POST http://relay:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "password": "securepassword"}'
```

### Current User

#### `GET /api/auth/me`

```bash
curl http://relay:8080/api/auth/me \
  -H "Authorization: Bearer $TOKEN"
```

### Logout

#### `POST /api/auth/logout`

```bash
curl -X POST http://relay:8080/api/auth/logout \
  -H "Authorization: Bearer $TOKEN"
```

### Auth Configuration

#### `GET /api/auth/config`

No auth required. Returns what auth methods are available.

```bash
curl http://relay:8080/api/auth/config
```

```json
{
  "signup_enabled": true,
  "social_providers": ["google", "github"],
  "magic_link_enabled": true,
  "device_auth_enabled": true
}
```

---

## OAuth (Social Login)

### Get OAuth URL

#### `GET /api/auth/oauth/{provider}/url`

```bash
curl "http://relay:8080/api/auth/oauth/github/url?redirect_uri=http://localhost:3000/callback"
```

Providers: `google`, `github`.

### OAuth Callback

#### `POST /api/auth/oauth/{provider}/callback`

```bash
curl -X POST http://relay:8080/api/auth/oauth/github/callback \
  -H "Content-Type: application/json" \
  -d '{"code": "auth-code", "state": "csrf-state", "redirect_uri": "http://localhost:3000/callback"}'
```

---

## Magic Link (Passwordless Login)

### Send Magic Link

#### `POST /api/auth/magic-link/send`

Send a passwordless login email. Requires SMTP to be configured on the relay.

```bash
curl -X POST http://relay:8080/api/auth/magic-link/send \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com"}'
```

### Verify Magic Link

#### `GET /api/auth/magic-link/verify`

Verify the magic link token from the email and receive a session token.

```bash
curl "http://relay:8080/api/auth/magic-link/verify?token=magic-link-token-here"
```

---

## Device Authorization (RFC 8628)

For CLI and desktop app login without browser redirect.

### Initiate

#### `POST /api/device/authorize`

```bash
curl -X POST http://relay:8080/api/device/authorize \
  -H "Content-Type: application/json" \
  -d '{"client_id": "localup-cli"}'
```

```json
{
  "device_code": "device-code-here",
  "user_code": "ABCD-1234",
  "verification_uri": "https://relay.example.com/device",
  "expires_in": 900,
  "interval": 5
}
```

### Poll for Token

#### `POST /api/device/token`

```bash
curl -X POST http://relay:8080/api/device/token \
  -H "Content-Type: application/json" \
  -d '{
    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    "device_code": "device-code-here",
    "client_id": "localup-cli"
  }'
```

Returns `authorization_pending` until the user approves, then returns the access token.

### Device Info

#### `GET /api/device/info`

Get information about a pending device authorization (used by the verification page).

```bash
curl "http://relay:8080/api/device/info?user_code=ABCD-1234"
```

### Approve (Browser)

#### `POST /api/device/verify`

User approves the device from the browser.

```bash
curl -X POST http://relay:8080/api/device/verify \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_code": "ABCD-1234"}'
```

### Deny (Browser)

#### `POST /api/device/deny`

User denies the device authorization from the browser.

```bash
curl -X POST http://relay:8080/api/device/deny \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"user_code": "ABCD-1234"}'
```

---

## API Keys (Auth Tokens)

Long-lived tokens for programmatic access.

### List API Keys

#### `GET /api/auth-tokens`

```bash
curl http://relay:8080/api/auth-tokens \
  -H "Authorization: Bearer $TOKEN"
```

### Get API Key

#### `GET /api/auth-tokens/{id}`

Get details for a specific API key (token value is not returned).

```bash
curl http://relay:8080/api/auth-tokens/token-uuid \
  -H "Authorization: Bearer $TOKEN"
```

### Create API Key

#### `POST /api/auth-tokens`

```bash
curl -X POST http://relay:8080/api/auth-tokens \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "CI/CD Pipeline", "description": "For automated deployments", "expires_in_days": 90}'
```

The response includes the token value -- **save it immediately**, it's only shown once.

### Update API Key

#### `PATCH /api/auth-tokens/{id}`

```bash
curl -X PATCH http://relay:8080/api/auth-tokens/token-uuid \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Updated Name", "is_active": false}'
```

### Delete API Key

#### `DELETE /api/auth-tokens/{id}`

```bash
curl -X DELETE http://relay:8080/api/auth-tokens/token-uuid \
  -H "Authorization: Bearer $TOKEN"
```

---

## Teams

### List Teams

#### `GET /api/teams`

List teams for the current user.

```bash
curl http://relay:8080/api/teams \
  -H "Authorization: Bearer $TOKEN"
```

---

## Database Configuration

The relay stores captured requests and user data in a database. Configure with `--database-url`:

```bash
# In-memory SQLite (default, data lost on restart)
localup relay http

# File-based SQLite
localup relay http --database-url "sqlite://./tunnel.db?mode=rwc"

# PostgreSQL
localup relay http --database-url "postgres://user:pass@localhost/localup_db"

# PostgreSQL with TimescaleDB (recommended for production)
localup relay http --database-url "postgres://user:pass@localhost/localup_db"
```

Migrations run automatically on startup.

---

## Error Responses

All errors follow RFC 7807 Problem Details format:

```json
{
  "type": "https://httpstatuses.io/404",
  "title": "Not Found",
  "status": 404,
  "detail": "Tunnel 'myapp' not found"
}
```

Common status codes:

| Status | Meaning |
|--------|---------|
| 200 | Success |
| 201 | Created |
| 400 | Bad request (invalid input) |
| 401 | Unauthorized (missing/invalid token) |
| 403 | Forbidden (insufficient permissions) |
| 404 | Not found |
| 409 | Conflict (duplicate resource) |
| 500 | Internal server error |
