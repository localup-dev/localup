# Makefile for localup development

.PHONY: build build-release relay relay-https relay-http relay-tls tunnel tunnel-https tunnel-custom-domain tunnel-tls test test-server tls-server https-server test-daemon clean gen-cert gen-cert-if-needed gen-cert-custom-domain gen-cert-tls gen-token register-custom-domain list-custom-domains daemon-config daemon-start daemon-stop daemon-status daemon-tunnel-start daemon-tunnel-stop daemon-reload daemon-quick-test help

# Default target
help:
	@echo "localup Development Makefile"
	@echo ""
	@echo "Build targets:"
	@echo "  make build          - Build debug version"
	@echo "  make build-release  - Build release version"
	@echo ""
	@echo "Relay targets:"
	@echo "  make relay          - Start HTTPS relay with localho.st domain (default)"
	@echo "  make relay-https    - Start HTTPS relay with localho.st domain"
	@echo "  make relay-http     - Start HTTP-only relay with localho.st domain"
	@echo "  make relay-tls      - Start TLS/SNI relay (passthrough, no termination)"
	@echo ""
	@echo "Client targets:"
	@echo "  make tunnel         - Start HTTPS tunnel client (LOCAL_PORT=8080, SUBDOMAIN=myapp)"
	@echo "  make tunnel-https   - Same as tunnel"
	@echo "  make tunnel-custom-domain CUSTOM_DOMAIN=api.example.com - Start tunnel with custom domain"
	@echo "  make tunnel-tls SNI_DOMAINS=test1.example.com,test2.example.com - Start TLS tunnel with SNI patterns"
	@echo ""
	@echo "Custom Domain targets:"
	@echo "  make gen-cert-custom-domain CUSTOM_DOMAIN=api.example.com - Generate cert for custom domain"
	@echo "  make register-custom-domain CUSTOM_DOMAIN=api.example.com - Register custom domain via API"
	@echo "  make list-custom-domains    - List all registered custom domains"
	@echo ""
	@echo "Daemon + IPC targets:"
	@echo "  make daemon-start           - Start daemon with test .localup.yml"
	@echo "  make daemon-stop            - Stop daemon"
	@echo "  make daemon-status          - Get status via IPC"
	@echo "  make daemon-tunnel-start TUNNEL_NAME=api - Start tunnel via IPC"
	@echo "  make daemon-tunnel-stop TUNNEL_NAME=api  - Stop tunnel via IPC"
	@echo "  make daemon-reload          - Reload config via IPC"
	@echo "  make test-daemon            - Show full daemon test instructions"
	@echo ""
	@echo "TLS/SNI Testing targets:"
	@echo "  make relay-tls      - Start TLS/SNI passthrough relay"
	@echo "  make tls-server     - Start TLS echo test server on port 9443"
	@echo "  make https-server   - Start HTTPS test server on port 9443 (HTTP over TLS)"
	@echo "  make tunnel-tls     - Start TLS tunnel with SNI patterns"
	@echo "  make gen-cert-tls   - Generate TLS certificates for backend server"
	@echo "  make test-tls       - Show full TLS testing instructions"
	@echo ""
	@echo "Utility targets:"
	@echo "  make gen-cert       - Generate self-signed certificates for localho.st"
	@echo "  make gen-token      - Generate a JWT token for testing"
	@echo "  make test           - Run all tests"
	@echo "  make clean          - Clean build artifacts"
	@echo ""
	@echo "Access URLs after starting relay:"
	@echo "  HTTP:  http://myapp.localho.st:28080"
	@echo "  HTTPS: https://myapp.localho.st:28443"
	@echo "  API:   http://localhost:3080/swagger-ui"
	@echo ""
	@echo "Custom Domain Example:"
	@echo "  1. make relay                                           # Start relay"
	@echo "  2. make gen-cert-custom-domain CUSTOM_DOMAIN=api.test   # Generate cert"
	@echo "  3. make register-custom-domain CUSTOM_DOMAIN=api.test   # Register domain"
	@echo "  4. make tunnel-custom-domain CUSTOM_DOMAIN=api.test LOCAL_PORT=3000"
	@echo ""
	@echo "Data is persisted in: localup-dev.db (SQLite)"

# Configuration
JWT_SECRET ?= dev-secret-key-change-in-production
LOCALUP_ADDR ?= 0.0.0.0:4443
HTTP_ADDR ?= 0.0.0.0:28080
HTTPS_ADDR ?= 0.0.0.0:28443
API_ADDR ?= 0.0.0.0:3080
DOMAIN ?= localho.st
# Log level: info, debug, trace (use LOG_LEVEL=debug make <target> for verbose output)
LOG_LEVEL ?= info
ADMIN_EMAIL ?= admin@localho.st
ADMIN_PASSWORD ?= admin123
DATABASE_URL ?= sqlite://./localup-dev.db?mode=rwc

# Client configuration
LOCAL_PORT ?= 8080
SUBDOMAIN ?= myapp
RELAY_ADDR ?= localhost:4443
USER_ID ?= 1

# Custom domain configuration
CUSTOM_DOMAIN ?= api.example.com
CUSTOM_DOMAIN_CERT_DIR ?= ./certs

# TLS/SNI configuration
TLS_ADDR ?= 0.0.0.0:28443
TLS_BACKEND_PORT ?= 9443
TLS_CERT_DIR ?= ./certs/tls
SNI_DOMAINS ?= test1.example.com,test2.example.com,*.example.com

# Certificate paths
CERT_FILE ?= localhost-cert.pem
KEY_FILE ?= localhost-key.pem

# Build debug version
build:
	cargo build  -p localup-cli --bin=localup

# Build release version
build-release:
	cargo build --release

# Generate self-signed certificates for localho.st
gen-cert:
	@echo "Generating self-signed certificate for localho.st..."
	openssl req -x509 -newkey rsa:2048 \
		-keyout $(KEY_FILE) \
		-out $(CERT_FILE) \
		-days 365 -nodes \
		-subj "/CN=localho.st" \
		-addext "subjectAltName=DNS:localho.st,DNS:*.localho.st,DNS:localhost,DNS:*.localhost,IP:127.0.0.1"
	@echo "Certificate generated: $(CERT_FILE)"
	@echo "Key generated: $(KEY_FILE)"

# Generate a JWT token for testing
gen-token: build
	@./target/debug/localup generate-token \
		--secret "$(JWT_SECRET)" \
		--sub "test-tunnel" \
		--user-id "$(USER_ID)" \
		--hours 24

# Start HTTPS relay (default)
relay: relay-https

# Generate certificates if they don't exist
gen-cert-if-needed:
	@if [ ! -f "$(CERT_FILE)" ] || [ ! -f "$(KEY_FILE)" ]; then \
		echo "Certificates not found, generating..."; \
		$(MAKE) gen-cert; \
	fi

# Start HTTPS relay with localho.st domain
relay-https: build gen-cert-if-needed
	@echo ""
	@echo "Starting HTTPS relay with localho.st domain..."
	@echo "================================================"
	@echo "  Domain:     $(DOMAIN)"
	@echo "  QUIC:       $(LOCALUP_ADDR)"
	@echo "  HTTP:       $(HTTP_ADDR)"
	@echo "  HTTPS:      $(HTTPS_ADDR)"
	@echo "  API:        $(API_ADDR)"
	@echo "  JWT Secret: $(JWT_SECRET)"
	@echo ""
	@echo "Access URLs:"
	@echo "  HTTP:    http://<subdomain>.$(DOMAIN):28080"
	@echo "  HTTPS:   https://<subdomain>.$(DOMAIN):28443"
	@echo "  API:     http://localhost:3080/swagger-ui"
	@echo ""
	@echo "Generate a token with: make gen-token"
	@echo "================================================"
	@echo ""
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup relay http \
		--localup-addr $(LOCALUP_ADDR) \
		--http-addr $(HTTP_ADDR) \
		--https-addr $(HTTPS_ADDR) \
		--tls-cert $(CERT_FILE) \
		--tls-key $(KEY_FILE) \
		--domain $(DOMAIN) \
		--jwt-secret "$(JWT_SECRET)" \
		--api-http-addr $(API_ADDR) \
		--admin-email "$(ADMIN_EMAIL)" \
		--admin-password "$(ADMIN_PASSWORD)" \
		--database-url "$(DATABASE_URL)" \
		--log-level $(LOG_LEVEL)

# Start HTTP-only relay with localho.st domain
relay-http: build
	@echo ""
	@echo "Starting HTTP relay with localho.st domain..."
	@echo "=============================================="
	@echo "  Domain:     $(DOMAIN)"
	@echo "  QUIC:       $(LOCALUP_ADDR)"
	@echo "  HTTP:       $(HTTP_ADDR)"
	@echo "  API:        $(API_ADDR)"
	@echo "  JWT Secret: $(JWT_SECRET)"
	@echo ""
	@echo "Access URLs:"
	@echo "  HTTP:    http://<subdomain>.$(DOMAIN):28080"
	@echo "  API:     http://localhost:3080/swagger-ui"
	@echo ""
	@echo "Generate a token with: make gen-token"
	@echo "=============================================="
	@echo ""
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup relay http \
		--localup-addr $(LOCALUP_ADDR) \
		--http-addr $(HTTP_ADDR) \
		--domain $(DOMAIN) \
		--jwt-secret "$(JWT_SECRET)" \
		--api-http-addr $(API_ADDR) \
		--admin-email "$(ADMIN_EMAIL)" \
		--admin-password "$(ADMIN_PASSWORD)" \
		--database-url "$(DATABASE_URL)" \
		--log-level $(LOG_LEVEL)

# Start tunnel client (HTTPS protocol)
tunnel: tunnel-https

# Start HTTPS tunnel client
tunnel-https: build
	@echo ""
	@echo "Starting HTTPS tunnel client..."
	@echo "================================"
	@echo "  Local port:  $(LOCAL_PORT)"
	@echo "  Subdomain:   $(SUBDOMAIN)"
	@echo "  Relay:       $(RELAY_ADDR)"
	@echo "  Protocol:    https"
	@echo ""
	@echo "Your service will be accessible at:"
	@echo "  HTTP:  http://$(SUBDOMAIN).$(DOMAIN):28080"
	@echo "  HTTPS: https://$(SUBDOMAIN).$(DOMAIN):28443"
	@echo "================================"
	@echo ""
	@TOKEN=$$(./target/debug/localup generate-token --secret "$(JWT_SECRET)" --sub "$(SUBDOMAIN)" --user-id "$(USER_ID)" --hours 24 --token-only); \
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup \
		--port $(LOCAL_PORT) \
		--relay $(RELAY_ADDR) \
		--protocol https \
		--subdomain $(SUBDOMAIN) \
		--token "$$TOKEN"

# Run all tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean

# ==========================================
# Custom Domain Targets
# ==========================================

# Generate self-signed certificate for a custom domain
gen-cert-custom-domain:
	@mkdir -p $(CUSTOM_DOMAIN_CERT_DIR)
	@echo "Generating self-signed certificate for $(CUSTOM_DOMAIN)..."
	openssl req -x509 -newkey rsa:2048 \
		-keyout $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-key.pem \
		-out $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-cert.pem \
		-days 365 -nodes \
		-subj "/CN=$(CUSTOM_DOMAIN)" \
		-addext "subjectAltName=DNS:$(CUSTOM_DOMAIN)"
	@echo ""
	@echo "Certificate generated:"
	@echo "  Cert: $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-cert.pem"
	@echo "  Key:  $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-key.pem"

# Register a custom domain with the relay API (uploads certificate)
register-custom-domain:
	@echo "Registering custom domain: $(CUSTOM_DOMAIN)"
	@echo ""
	@if [ ! -f "$(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-cert.pem" ]; then \
		echo "Error: Certificate not found. Run 'make gen-cert-custom-domain CUSTOM_DOMAIN=$(CUSTOM_DOMAIN)' first."; \
		exit 1; \
	fi
	@CERT_CONTENT=$$(cat $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-cert.pem | sed 's/"/\\"/g' | tr '\n' '|' | sed 's/|/\\n/g'); \
	KEY_CONTENT=$$(cat $(CUSTOM_DOMAIN_CERT_DIR)/$(CUSTOM_DOMAIN)-key.pem | sed 's/"/\\"/g' | tr '\n' '|' | sed 's/|/\\n/g'); \
	curl -s -X POST "http://localhost:$(subst 0.0.0.0:,,$(API_ADDR))/api/custom-domains" \
		-H "Content-Type: application/json" \
		-d "{\"domain\": \"$(CUSTOM_DOMAIN)\", \"cert_pem\": \"$$CERT_CONTENT\", \"key_pem\": \"$$KEY_CONTENT\"}" | jq .
	@echo ""
	@echo "Custom domain $(CUSTOM_DOMAIN) registered!"
	@echo "You can now use: make tunnel-custom-domain CUSTOM_DOMAIN=$(CUSTOM_DOMAIN) LOCAL_PORT=<port>"

# List all registered custom domains
list-custom-domains:
	@echo "Listing custom domains..."
	@curl -s "http://localhost:$(subst 0.0.0.0:,,$(API_ADDR))/api/custom-domains" | jq .

# Start tunnel with custom domain
tunnel-custom-domain: build
	@echo ""
	@echo "Starting tunnel with custom domain..."
	@echo "======================================"
	@echo "  Custom Domain: $(CUSTOM_DOMAIN)"
	@echo "  Local port:    $(LOCAL_PORT)"
	@echo "  Relay:         $(RELAY_ADDR)"
	@echo "  Protocol:      https"
	@echo ""
	@echo "Your service will be accessible at:"
	@echo "  HTTPS: https://$(CUSTOM_DOMAIN):28443"
	@echo "======================================"
	@echo ""
	@echo "Note: Ensure DNS for $(CUSTOM_DOMAIN) points to localhost/127.0.0.1"
	@echo "      For testing, add to /etc/hosts: 127.0.0.1 $(CUSTOM_DOMAIN)"
	@echo ""
	@TOKEN=$$(./target/debug/localup generate-token --secret "$(JWT_SECRET)" --sub "$(CUSTOM_DOMAIN)" --user-id "$(USER_ID)" --hours 24 --token-only); \
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup \
		--port $(LOCAL_PORT) \
		--relay $(RELAY_ADDR) \
		--protocol https \
		--custom-domain $(CUSTOM_DOMAIN) \
		--token "$$TOKEN"

# Quick test: Start a simple HTTP server for testing tunnels
test-server:
	@echo "Starting test HTTP server on port $(LOCAL_PORT)..."
	@echo "This server returns 'Hello from localup test server!'"
	@python3 -c "from http.server import HTTPServer, BaseHTTPRequestHandler; \
		class H(BaseHTTPRequestHandler): \
			def do_GET(self): \
				self.send_response(200); \
				self.send_header('Content-Type', 'text/plain'); \
				self.end_headers(); \
				self.wfile.write(b'Hello from localup test server!\\n'); \
		print('Test server running on http://localhost:$(LOCAL_PORT)'); \
		HTTPServer(('', $(LOCAL_PORT)), H).serve_forever()"

# ==========================================
# TLS/SNI Passthrough Testing Targets
# ==========================================

# Generate TLS certificates for backend server
gen-cert-tls:
	@mkdir -p $(TLS_CERT_DIR)
	@echo "Generating TLS backend certificates..."
	openssl genpkey -algorithm RSA -out $(TLS_CERT_DIR)/backend.key -pkeyopt rsa_keygen_bits:2048
	openssl req -new -key $(TLS_CERT_DIR)/backend.key -out $(TLS_CERT_DIR)/backend.csr \
		-subj "/CN=localhost"
	@echo "basicConstraints=CA:FALSE" > $(TLS_CERT_DIR)/backend.ext
	@echo "keyUsage=digitalSignature,keyEncipherment" >> $(TLS_CERT_DIR)/backend.ext
	@echo "subjectAltName=DNS:localhost,DNS:test1.example.com,DNS:test2.example.com,DNS:*.example.com,IP:127.0.0.1" >> $(TLS_CERT_DIR)/backend.ext
	openssl x509 -req -in $(TLS_CERT_DIR)/backend.csr -signkey $(TLS_CERT_DIR)/backend.key \
		-out $(TLS_CERT_DIR)/backend.crt -days 365 \
		-extfile $(TLS_CERT_DIR)/backend.ext
	@rm -f $(TLS_CERT_DIR)/backend.csr $(TLS_CERT_DIR)/backend.ext
	@echo ""
	@echo "TLS backend certificates generated:"
	@echo "  Cert: $(TLS_CERT_DIR)/backend.crt"
	@echo "  Key:  $(TLS_CERT_DIR)/backend.key"

# Start TLS/SNI passthrough relay
relay-tls: build gen-cert-if-needed
	@echo ""
	@echo "Starting TLS/SNI passthrough relay..."
	@echo "======================================"
	@echo "  QUIC Control: $(LOCALUP_ADDR)"
	@echo "  TLS Server:   $(TLS_ADDR)"
	@echo "  API HTTP:     $(API_ADDR)"
	@echo "  JWT Secret:   $(JWT_SECRET)"
	@echo ""
	@echo "TLS traffic is passed through without termination."
	@echo "SNI is extracted from ClientHello for routing."
	@echo ""
	@echo "Generate a token with: make gen-token"
	@echo "======================================"
	@echo ""
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup relay tls \
		--localup-addr $(LOCALUP_ADDR) \
		--tls-addr $(TLS_ADDR) \
		--jwt-secret "$(JWT_SECRET)" \
		--api-http-addr $(API_ADDR) \
		--log-level $(LOG_LEVEL)

# Start TLS echo test server (Python)
tls-server: gen-cert-tls
	@echo ""
	@echo "Starting TLS echo server on port $(TLS_BACKEND_PORT)..."
	@echo "======================================================="
	@echo "  Cert: $(TLS_CERT_DIR)/backend.crt"
	@echo "  Key:  $(TLS_CERT_DIR)/backend.key"
	@echo ""
	@echo "This server echoes back received data with 'TLS BACKEND RESPONSE:' prefix"
	@echo "======================================================="
	@echo ""
	@python3 -c "$$TLS_SERVER_SCRIPT"

define TLS_SERVER_SCRIPT
import ssl, socket, sys, datetime
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain('$(TLS_CERT_DIR)/backend.crt', '$(TLS_CERT_DIR)/backend.key')
context.check_hostname = False
context.verify_mode = ssl.CERT_NONE
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(('127.0.0.1', $(TLS_BACKEND_PORT)))
sock.listen(5)
print('TLS Echo Server running on port $(TLS_BACKEND_PORT)', file=sys.stderr, flush=True)
ssock = context.wrap_socket(sock, server_side=True)
request_count = 0
while True:
    try:
        conn, addr = ssock.accept()
        request_count += 1
        ts = datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        print(f'[{ts}] #{request_count} Connection from {addr}', file=sys.stderr, flush=True)
        data = conn.recv(4096)
        print(f'[{ts}] #{request_count} Received {len(data)} bytes: {data[:100]}...', file=sys.stderr, flush=True)
        conn.sendall(b'TLS BACKEND RESPONSE: ' + data)
        conn.close()
        print(f'[{ts}] #{request_count} Response sent, connection closed', file=sys.stderr, flush=True)
    except Exception as e:
        print(f'[ERROR] {e}', file=sys.stderr, flush=True)
endef
export TLS_SERVER_SCRIPT

# Start HTTPS test server (Python) - serves HTTP over TLS
https-server: gen-cert-tls
	@echo ""
	@echo "Starting HTTPS test server on port $(TLS_BACKEND_PORT)..."
	@echo "=========================================================="
	@echo "  Cert: $(TLS_CERT_DIR)/backend.crt"
	@echo "  Key:  $(TLS_CERT_DIR)/backend.key"
	@echo ""
	@echo "This serves HTTP responses over TLS (proper HTTPS)"
	@echo "=========================================================="
	@echo ""
	@python3 -c "$$HTTPS_SERVER_SCRIPT"

define HTTPS_SERVER_SCRIPT
import ssl, http.server, datetime

class LoggingHandler(http.server.BaseHTTPRequestHandler):
    request_count = 0

    def log_message(self, format, *args):
        ts = datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        print(f'[{ts}] {self.address_string()} - {format % args}', flush=True)

    def do_GET(self):
        LoggingHandler.request_count += 1
        ts = datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        print(f'[{ts}] #{LoggingHandler.request_count} GET {self.path} from {self.address_string()}', flush=True)
        print(f'[{ts}] #{LoggingHandler.request_count} Headers: {dict(self.headers)}', flush=True)
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        response = f'HTTPS Backend Response\nRequest #{LoggingHandler.request_count}\nPath: {self.path}\nTime: {ts}\n'
        self.wfile.write(response.encode())
        print(f'[{ts}] #{LoggingHandler.request_count} Response sent: 200 OK', flush=True)

    def do_POST(self):
        LoggingHandler.request_count += 1
        ts = datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        print(f'[{ts}] #{LoggingHandler.request_count} POST {self.path} from {self.address_string()}', flush=True)
        print(f'[{ts}] #{LoggingHandler.request_count} Headers: {dict(self.headers)}', flush=True)
        print(f'[{ts}] #{LoggingHandler.request_count} Body ({content_length} bytes): {body[:200]}', flush=True)
        self.send_response(200)
        self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        response = f'HTTPS Backend Response\nRequest #{LoggingHandler.request_count}\nPath: {self.path}\nBody received: {content_length} bytes\n'
        self.wfile.write(response.encode())
        print(f'[{ts}] #{LoggingHandler.request_count} Response sent: 200 OK', flush=True)

context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain('$(TLS_CERT_DIR)/backend.crt', '$(TLS_CERT_DIR)/backend.key')
server = http.server.HTTPServer(('127.0.0.1', $(TLS_BACKEND_PORT)), LoggingHandler)
server.socket = context.wrap_socket(server.socket, server_side=True)
print(f'HTTPS Server running on https://127.0.0.1:$(TLS_BACKEND_PORT)', flush=True)
print(f'Test with: curl -k https://localhost:$(TLS_BACKEND_PORT)/', flush=True)
server.serve_forever()
endef
export HTTPS_SERVER_SCRIPT

# Start TLS tunnel with SNI patterns
tunnel-tls: build
	@echo ""
	@echo "Starting TLS tunnel with SNI patterns..."
	@echo "========================================="
	@echo "  Backend:     localhost:$(TLS_BACKEND_PORT)"
	@echo "  Relay:       $(RELAY_ADDR)"
	@echo "  Protocol:    tls"
	@echo "  SNI Domains: $(SNI_DOMAINS)"
	@echo ""
	@echo "Incoming TLS connections matching these SNI patterns"
	@echo "will be routed to localhost:$(TLS_BACKEND_PORT)"
	@echo "========================================="
	@echo ""
	@TOKEN=$$(./target/debug/localup generate-token --secret "$(JWT_SECRET)" --sub "tls-tunnel" --user-id "$(USER_ID)" --hours 24 --token-only); \
	CUSTOM_DOMAIN_ARGS=$$(echo "$(SNI_DOMAINS)" | tr ',' '\n' | sed 's/^/--custom-domain /' | tr '\n' ' '); \
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup \
		--port $(TLS_BACKEND_PORT) \
		--relay $(RELAY_ADDR) \
		--protocol tls \
		$$CUSTOM_DOMAIN_ARGS \
		--token "$$TOKEN"

# Full TLS testing instructions
test-tls:
	@echo ""
	@echo "=========================================="
	@echo "TLS/SNI Passthrough Testing Guide"
	@echo "=========================================="
	@echo ""
	@echo "This tests TLS passthrough routing based on SNI (Server Name Indication)."
	@echo "The relay extracts SNI from ClientHello without decrypting TLS traffic."
	@echo ""
	@echo "Prerequisites:"
	@echo "  - Add to /etc/hosts: 127.0.0.1 test1.example.com test2.example.com"
	@echo ""
	@echo "Test Steps:"
	@echo ""
	@echo "  # Terminal 1: Start TLS relay"
	@echo "  make relay-tls"
	@echo ""
	@echo "  # Terminal 2: Start TLS backend server"
	@echo "  make tls-server"
	@echo ""
	@echo "  # Terminal 3: Start TLS tunnel"
	@echo "  make tunnel-tls"
	@echo ""
	@echo "  # Terminal 4: Test TLS connection"
	@echo "  echo 'Hello TLS' | openssl s_client -connect localhost:28443 \\"
	@echo "    -servername test1.example.com -quiet 2>/dev/null"
	@echo ""
	@echo "  # Expected output: 'TLS BACKEND RESPONSE: Hello TLS'"
	@echo ""
	@echo "Custom SNI domains (comma-separated):"
	@echo "  make tunnel-tls SNI_DOMAINS=app1.test,app2.test,*.test"
	@echo ""
	@echo "Verify SNI extraction in relay logs:"
	@echo "  Look for: 'Extracted SNI: test1.example.com'"
	@echo "  Look for: 'Routing SNI test1.example.com to backend'"
	@echo ""
	@echo "=========================================="

# ==========================================
# Daemon + IPC Testing Targets
# ==========================================

# Create a test project config with custom domain (uses LOCALUP_TOKEN env var)
daemon-config:
	@echo "Creating test .localup.yml with custom domain..."
	@echo "# Test configuration for daemon with custom domain" > .localup.yml
	@echo "defaults:" >> .localup.yml
	@echo "  relay: \"$(RELAY_ADDR)\"" >> .localup.yml
	@echo "  token: \"\$${LOCALUP_TOKEN}\"" >> .localup.yml
	@echo "  local_host: \"localhost\"" >> .localup.yml
	@echo "  timeout_seconds: 30" >> .localup.yml
	@echo "" >> .localup.yml
	@echo "tunnels:" >> .localup.yml
	@echo "  # Standard subdomain tunnel" >> .localup.yml
	@echo "  - name: api" >> .localup.yml
	@echo "    port: 3020" >> .localup.yml
	@echo "    protocol: https" >> .localup.yml
	@echo "    subdomain: myapp" >> .localup.yml
	@echo "" >> .localup.yml
	@echo "  # Custom domain tunnel" >> .localup.yml
	@echo "  - name: production" >> .localup.yml
	@echo "    port: 8080" >> .localup.yml
	@echo "    protocol: https" >> .localup.yml
	@echo "    custom_domain: $(CUSTOM_DOMAIN)" >> .localup.yml
	@echo ""
	@echo "Created .localup.yml with:"
	@echo "  - api tunnel (subdomain: myapp, port 3000)"
	@echo "  - production tunnel (custom_domain: $(CUSTOM_DOMAIN), port 8080)"
	@echo ""
	@cat .localup.yml

# Create a simple subdomain-only config (no token - uses config set-token)
daemon-config-simple:
	@echo "Creating simple .localup.yml (uses stored token from 'config set-token')..."
	@echo "# Simple subdomain tunnel configuration" > .localup.yml
	@echo "defaults:" >> .localup.yml
	@echo "  relay: \"$(RELAY_ADDR)\"" >> .localup.yml
	@echo "  local_host: \"localhost\"" >> .localup.yml
	@echo "  timeout_seconds: 30" >> .localup.yml
	@echo "" >> .localup.yml
	@echo "tunnels:" >> .localup.yml
	@echo "  - name: api" >> .localup.yml
	@echo "    port: $(LOCAL_PORT)" >> .localup.yml
	@echo "    protocol: https" >> .localup.yml
	@echo "    subdomain: myapp" >> .localup.yml
	@echo ""
	@echo "Created .localup.yml with:"
	@echo "  - api tunnel (subdomain: myapp, port $(LOCAL_PORT))"
	@echo "  - Token: uses stored token from 'localup config set-token'"
	@echo ""
	@cat .localup.yml

# Start daemon with test config (generates and sets LOCALUP_TOKEN)
daemon-start: build daemon-config
	@echo ""
	@echo "Starting localup daemon..."
	@echo "=========================="
	@TOKEN=$$(./target/debug/localup generate-token --secret "$(JWT_SECRET)" --sub "daemon-test" --user-id "$(USER_ID)" --hours 24 --token-only); \
	LOCALUP_TOKEN="$$TOKEN" RUST_LOG=$(LOG_LEVEL) ./target/debug/localup daemon start

# Start daemon with simple config (uses stored token from 'config set-token')
daemon-start-simple: build daemon-config-simple
	@echo ""
	@echo "Starting localup daemon with stored token..."
	@echo "==========================================="
	@echo "Note: Token is read from ~/.localup/config.json (set via 'localup config set-token')"
	@echo ""
	RUST_LOG=$(LOG_LEVEL) ./target/debug/localup daemon start

# Stop daemon (shows instructions - daemon runs in foreground)
daemon-stop: build
	@echo "Stopping localup daemon..."
	@./target/debug/localup daemon stop

# Get daemon status (IPC test)
daemon-status: build
	@echo "Getting daemon status via IPC..."
	@./target/debug/localup daemon status

# Start a specific tunnel via IPC
daemon-tunnel-start: build
	@echo "Starting tunnel '$(TUNNEL_NAME)' via IPC..."
	@./target/debug/localup daemon tunnel-start $(TUNNEL_NAME)

# Stop a specific tunnel via IPC
daemon-tunnel-stop: build
	@echo "Stopping tunnel '$(TUNNEL_NAME)' via IPC..."
	@./target/debug/localup daemon tunnel-stop $(TUNNEL_NAME)

# Reload daemon config via IPC
daemon-reload: build
	@echo "Reloading daemon configuration via IPC..."
	@./target/debug/localup daemon reload

# Full daemon test with custom domain
test-daemon: build
	@echo ""
	@echo "=========================================="
	@echo "Daemon + IPC Custom Domain Test"
	@echo "=========================================="
	@echo ""
	@echo "Prerequisites:"
	@echo "  1. Relay running: make relay-https"
	@echo "  2. Custom domain registered: make register-custom-domain CUSTOM_DOMAIN=$(CUSTOM_DOMAIN)"
	@echo "  3. /etc/hosts entry: 127.0.0.1 $(CUSTOM_DOMAIN)"
	@echo ""
	@echo "Test Steps:"
	@echo ""
	@echo "  # Terminal 1: Start relay"
	@echo "  make relay-https"
	@echo ""
	@echo "  # Terminal 2: Register custom domain and start test servers"
	@echo "  make gen-cert-custom-domain CUSTOM_DOMAIN=$(CUSTOM_DOMAIN)"
	@echo "  make register-custom-domain CUSTOM_DOMAIN=$(CUSTOM_DOMAIN)"
	@echo "  make test-server LOCAL_PORT=3000 &"
	@echo "  make test-server LOCAL_PORT=8080 &"
	@echo ""
	@echo "  # Terminal 3: Start daemon and test IPC"
	@echo "  make daemon-start"
	@echo ""
	@echo "  # Terminal 4: Test IPC commands"
	@echo "  make daemon-status                          # View all tunnels"
	@echo "  make daemon-tunnel-stop TUNNEL_NAME=api     # Stop api tunnel"
	@echo "  make daemon-tunnel-start TUNNEL_NAME=api    # Restart api tunnel"
	@echo "  make daemon-reload                          # Reload config"
	@echo "  make daemon-stop                            # Stop daemon"
	@echo ""
	@echo "  # Test access"
	@echo "  curl -k https://myapp.localho.st:28443/     # Subdomain tunnel"
	@echo "  curl -k https://$(CUSTOM_DOMAIN):28443/     # Custom domain tunnel"
	@echo ""
	@echo "=========================================="

# Quick daemon test (assumes relay is already running)
daemon-quick-test: daemon-stop daemon-config
	@echo ""
	@echo "Quick daemon test..."
	@echo ""
	@TOKEN=$$(./target/debug/localup generate-token --secret "$(JWT_SECRET)" --sub "daemon-test" --user-id "$(USER_ID)" --hours 24 --token-only); \
	LOCALUP_TOKEN="$$TOKEN" RUST_LOG=$(LOG_LEVEL) ./target/debug/localup daemon start &
	@sleep 2
	@echo ""
	@echo "Daemon started. Checking status..."
	@./target/debug/localup daemon status
	@echo ""
	@echo "Stopping daemon..."
	@./target/debug/localup daemon stop

# Variables for daemon testing
TUNNEL_NAME ?= api
