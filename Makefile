# Makefile for localup development

.PHONY: build build-release relay relay-https relay-http tunnel tunnel-https test clean gen-cert gen-cert-if-needed gen-token help

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
	@echo ""
	@echo "Client targets:"
	@echo "  make tunnel         - Start HTTPS tunnel client (LOCAL_PORT=8080, SUBDOMAIN=myapp)"
	@echo "  make tunnel-https   - Same as tunnel"
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
	@echo "Data is persisted in: localup-dev.db (SQLite)"

# Configuration
JWT_SECRET ?= dev-secret-key-change-in-production
LOCALUP_ADDR ?= 0.0.0.0:4443
HTTP_ADDR ?= 0.0.0.0:28080
HTTPS_ADDR ?= 0.0.0.0:28443
API_ADDR ?= 0.0.0.0:3080
DOMAIN ?= localho.st
LOG_LEVEL ?= info
ADMIN_EMAIL ?= admin@localho.st
ADMIN_PASSWORD ?= admin123
DATABASE_URL ?= sqlite://./localup-dev.db?mode=rwc

# Client configuration
LOCAL_PORT ?= 8080
SUBDOMAIN ?= myapp
RELAY_ADDR ?= localhost:4443
USER_ID ?= 1

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
