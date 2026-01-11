# Quick Start: Testing the Full Auth System

This guide shows you how to test the complete authentication flow like a real user would.

## Step 1: Start the Relay (Exit Node)

```bash
# Build first
cargo build --release

# Start the relay with authentication enabled
./target/release/localup relay \
  --localup-addr 0.0.0.0:4443 \
  --http-addr 0.0.0.0:18080 \
  --https-addr 0.0.0.0:18443 \
  --tls-cert relay-cert.pem \
  --tls-key relay-key.pem \
  --jwt-secret "my-super-secret-key" \
  --database-url "sqlite://./localup.db?mode=rwc" \
  --domain localhost
```

**What this does:**
- Starts QUIC control plane on port 4443 (for tunnel connections)
- Starts HTTP server on port 18080 (for tunneled traffic)
- Starts HTTPS server on port 18443 (for tunneled traffic)
- Enables JWT authentication
- Creates/connects to SQLite database for user accounts and tokens

**You should see:**
```
✅ Database migrations complete
✅ JWT authentication enabled
✅ Control plane (QUIC) listening on 0.0.0.0:4443
✅ HTTP relay server running on 0.0.0.0:18080
✅ HTTPS relay server running on 0.0.0.0:18443
```

Leave this running. **This is your "server" that users connect to.**

---

## Step 2: Access the API (Simulating Web UI)

In a new terminal, let's simulate what a user would do through a web UI.

### 2.1: Register an Account

```bash
curl -X POST http://localhost:18080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@example.com",
    "password": "AdminPass123!",
    "username": "admin"
  }' | jq
```

**Save the token you receive:**
```bash
# Copy the "token" field from the response and save it:
export SESSION_TOKEN="eyJ0eXAiOiJKV1QiLC..."
```

> 💡 **With a Web UI**: You'd visit `http://localhost:18080`, click "Sign Up", fill in the form, and get automatically logged in.

---

### 2.2: Create an API Token for Your Tunnel

```bash
curl -X POST http://localhost:18080/api/auth-tokens \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -d '{
    "name": "My Production Tunnel",
    "description": "Token for my production app"
  }' | jq
```

**Copy the token from the response - THIS IS SHOWN ONLY ONCE!**
```bash
export TUNNEL_TOKEN="eyJ0eXAiOiJKV1QiLC..."
```

> 💡 **With a Web UI**: You'd see a dashboard with a button "Create New Token", fill in the name/description, and get a popup showing the token with a "Copy" button and warning: "⚠️ Save this now - you won't see it again!"

---

### 2.3: View Your Tokens

```bash
curl -X GET http://localhost:18080/api/auth-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN" | jq
```

You should see your token listed with:
- ✅ `is_active: true`
- ✅ `last_used_at: null` (not used yet)

> 💡 **With a Web UI**: You'd see a nice table with token names, creation dates, last used times, and buttons to revoke/delete.

---

## Step 3: Start Your Local Service

Start something to tunnel (any HTTP server):

```bash
# Option 1: Python
python3 -m http.server 3000

# Option 2: Node.js
npx http-server -p 3000

# Option 3: Any app you're developing
# npm run dev (if it runs on port 3000)
```

---

## Step 4: Connect Your Tunnel (The Client Experience)

Now pretend you're a user who just got their token from the web UI. Connect your tunnel:

```bash
./target/release/localup \
  --port 3000 \
  --relay localhost:4443 \
  --protocol http \
  --subdomain myapp \
  --token "$TUNNEL_TOKEN"
```

**You should see:**
```
✅ Tunnel connected successfully

╭────────────────────────────────────────────────────────╮
│           🚀 Tunnel Running Successfully               │
├────────────────────────────────────────────────────────┤
│ HTTP:  http://localhost:18080/myapp                    │
│ HTTPS: https://localhost:18443/myapp                   │
╰────────────────────────────────────────────────────────╯

Press Ctrl+C to stop the tunnel
```

**If authentication fails, you'll see:**
```
❌ Authentication failed: [reason]
```

---

## Step 5: Test Your Tunnel

Open your browser or use curl:

```bash
curl http://localhost:18080/myapp
```

You should see your local service's response!

---

## Step 6: Check Token Usage

Go back to the API and check your token was tracked:

```bash
curl -X GET http://localhost:18080/api/auth-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN" | jq
```

Now you should see:
- ✅ `last_used_at: "2025-01-17T10:45:00Z"` (timestamp when tunnel connected)

> 💡 **With a Web UI**: The dashboard would show "Last used: 2 minutes ago" with a green dot indicating "Active tunnel".

---

## Real User Flow Simulation

Here's what the **complete user experience** looks like:

### For the Relay Admin:
1. ✅ Start relay server with database and JWT secret
2. ✅ Server runs 24/7, handles multiple users

### For Each User:
1. ✅ Visit web portal (future: sign up form)
2. ✅ Create account with email/password
3. ✅ Login and see dashboard
4. ✅ Create API tokens for different tunnels/projects
5. ✅ Copy token and use it in CLI client
6. ✅ Start tunnel with: `localup --port 3000 --relay <server> --token <token>`
7. ✅ Share public URL with others
8. ✅ Revoke/delete tokens when needed

---

## What's Working Now (Phase A-E Complete)

✅ **User Registration & Login** - Full account system
✅ **Session Tokens** - 7-day web UI authentication
✅ **Auth Token Management** - Create/list/update/delete API keys
✅ **Token Type Enforcement** - Session vs auth tokens
✅ **Tunnel Authentication** - Auth tokens required for tunnels
✅ **Database Storage** - All data persisted
✅ **Hash-based Security** - Tokens never stored in plaintext
✅ **Revocation** - Instant token deactivation
✅ **Usage Tracking** - Last used timestamps
✅ **Ownership** - Users can only see/manage their own tokens

---

## What's Missing (Future Phases)

⏳ **Phase D: Web UI Dashboard**
- React dashboard for token management
- Visual token list with search/filter
- One-click token creation with copy button
- Real-time tunnel status indicators
- Usage analytics and charts

⏳ **Phase F: Teams & Multi-tenancy**
- Create teams with multiple members
- Share tokens across team members
- Team-based tunnel ownership
- Role-based permissions (owner/admin/member)

---

## Testing Failure Cases

### Test Invalid Token
```bash
# This will fail - wrong token
./target/release/localup \
  --port 3000 \
  --relay localhost:4443 \
  --protocol http \
  --subdomain test \
  --token "invalid-token-here"
```

**Expected:** `❌ Authentication failed: Invalid JWT token`

### Test Revoked Token

Revoke your token:
```bash
# Get your token ID from the list
TOKEN_ID="<copy-from-list-response>"

curl -X PATCH http://localhost:18080/api/auth-tokens/$TOKEN_ID \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -d '{"is_active": false}'
```

Try using it:
```bash
./target/release/localup \
  --port 3000 \
  --relay localhost:4443 \
  --protocol http \
  --subdomain test \
  --token "$TUNNEL_TOKEN"
```

**Expected:** `❌ Authentication failed: Auth token has been deactivated`

---

## Quick Commands Cheat Sheet

```bash
# 1. Start relay
./target/release/localup relay --localup-addr 0.0.0.0:4443 --http-addr 0.0.0.0:18080 --https-addr 0.0.0.0:18443 --tls-cert relay-cert.pem --tls-key relay-key.pem --jwt-secret "my-secret" --database-url "sqlite://./localup.db?mode=rwc" --domain localhost

# 2. Register
curl -X POST http://localhost:18080/api/auth/register -H "Content-Type: application/json" -d '{"email":"user@example.com","password":"Pass123!","username":"user"}' | jq

# 3. Set session token (copy from response)
export SESSION_TOKEN="<your-session-token>"

# 4. Create auth token
curl -X POST http://localhost:18080/api/auth-tokens -H "Content-Type: application/json" -H "Authorization: Bearer $SESSION_TOKEN" -d '{"name":"My Tunnel"}' | jq

# 5. Set tunnel token (copy from response)
export TUNNEL_TOKEN="<your-auth-token>"

# 6. Start local service
python3 -m http.server 3000

# 7. Connect tunnel
./target/release/localup --port 3000 --relay localhost:4443 --protocol http --subdomain myapp --token "$TUNNEL_TOKEN"

# 8. Test tunnel
curl http://localhost:18080/myapp
```

---

## Troubleshooting

**"Connection refused"**
→ Check relay is running on port 4443

**"Authentication failed: Invalid JWT token"**
→ Check JWT secret matches between relay and token

**"Missing Authorization header"**
→ Check you set `export SESSION_TOKEN="..."`

**"Auth token not found"**
→ Token was deleted/revoked, create a new one

**Can't start relay - "Address already in use"**
→ Kill existing process: `pkill -f localup`

---

## Success Criteria

You've successfully tested the auth system if:

1. ✅ You can register a user account
2. ✅ You can create an auth token via the API
3. ✅ You can connect a tunnel using that token
4. ✅ The tunnel works (you can access your local service)
5. ✅ The `last_used_at` timestamp updates
6. ✅ Invalid/revoked tokens are rejected
7. ✅ You can see all your tokens in the list

**All done? The authentication system is working! 🎉**

Next step: Build a web UI (Phase D) to replace the curl commands with a nice dashboard.
