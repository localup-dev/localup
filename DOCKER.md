# LocalUp Docker Guide

This guide covers Docker setup and deployment for the LocalUp tunnel application.

## Available Dockerfiles

### 1. `Dockerfile` - Multi-Stage Build (Recommended)
**Best for**: Production deployments, CI/CD, guaranteed correct Linux binary

**Pros**:
- ✅ Builds from source inside Docker
- ✅ Guaranteed correct binary for Linux
- ✅ Reproducible builds across platforms
- ✅ Single Dockerfile works on macOS, Linux, Windows
- ✅ Multi-stage: small final image (~200MB)

**Cons**:
- ❌ Longer build time (10-15 minutes, includes Rust compilation)
- ❌ Requires internet access for dependencies

**Build**:
```bash
docker build -f Dockerfile -t localup:latest .
```

### 2. `Dockerfile.prebuilt` - Prebuilt Binary (Alternative)
**Best for**: Quick testing, when you already have a compiled Linux binary

**Pros**:
- ✅ Fast builds (< 1 minute)
- ✅ Small context size
- ✅ Simple Dockerfile

**Cons**:
- ❌ Requires pre-compiled Linux binary (`target/release/localup`)
- ❌ Not suitable if building on macOS
- ❌ Extra step to compile binary separately

**Requirements**:
- Linux-compiled binary in `target/release/localup`
- Either compile on Linux or cross-compile: `cargo build --release --target x86_64-unknown-linux-gnu`

**Build**:
```bash
# First compile the binary
cargo build --release --target x86_64-unknown-linux-gnu

# Then build Docker image
docker build -f Dockerfile.prebuilt -t localup:latest .
```

## Quick Start

### Build from Source (Recommended)

```bash
# Build Docker image (compiles inside Docker)
docker build -f Dockerfile -t localup:latest .

# Test the image
docker run --rm localup:latest --version
docker run --rm localup:latest --help
```

### Using Pre-compiled Binary (Alternative)

```bash
# 1. Compile binary for Linux (on Linux or with cross-compilation)
cargo build --release --target x86_64-unknown-linux-gnu

# 2. Build Docker image using prebuilt binary
docker build -f Dockerfile.prebuilt -t localup:latest .

# 3. Test the image
docker run --rm localup:latest --version
docker run --rm localup:latest --help
```

### Using Docker Compose

```bash
# Generate a token
docker-compose run --rm localup generate-token \
  --secret "my-secret" \
  --localup-id "myapp"

# Run as relay server
docker-compose run --rm -p 4443:4443 -p 8080:8080 localup relay \
  --listen 0.0.0.0:4443 \
  --http-port 8080

# Run as agent
docker-compose run --rm localup agent \
  --relay localhost:4443 \
  --token "<TOKEN>" \
  --target-address "localhost:3000"
```

## Docker Testing

### Test 1: Verify Binary Works

```bash
docker run --rm localup:latest --version
docker run --rm localup:latest --help
```

### Test 2: Generate Token

```bash
docker run --rm localup:latest generate-token \
  --secret "test-secret" \
  --localup-id "test-app"
```

### Test 3: List Subcommands

```bash
docker run --rm localup:latest connect --help
docker run --rm localup:latest relay --help
docker run --rm localup:latest agent --help
docker run --rm localup:latest agent-server --help
docker run --rm localup:latest generate-token --help
```

### Test 4: Health Check

```bash
docker run --rm --name localup-health localup:latest --help && \
  echo "✅ Health check passed" || echo "❌ Health check failed"
```

## Running Services

### Run Relay Server

```bash
docker run -d \
  --name localup-relay \
  -p 4443:4443 \
  -p 8080:8080 \
  -e RUST_LOG=info \
  localup:latest \
  relay \
    --listen 0.0.0.0:4443 \
    --http-port 8080 \
    --localup-addr 0.0.0.0:4443
```

### Run Agent

```bash
docker run -d \
  --name localup-agent \
  -e RUST_LOG=info \
  --network host \
  localup:latest \
  agent \
    --relay localhost:4443 \
    --token "<TOKEN>" \
    --target-address "localhost:3000" \
    --insecure
```

### Run Agent Server

```bash
docker run -d \
  --name localup-agent-server \
  -p 4443:4443 \
  -e RUST_LOG=info \
  localup:latest \
  agent-server \
    --listen 0.0.0.0:4443
```

## Build Arguments

You can customize builds with environment variables:

```bash
# Set custom relay config
docker build --build-arg LOCALUP_RELAYS_CONFIG=/path/to/relays.yaml \
  -f Dockerfile.final -t localup:latest .

# Set log level during build
docker build --build-arg RUST_LOG=debug \
  -f Dockerfile.final -t localup:latest .
```

## Networking

### Port Mappings

| Port | Service | Purpose |
|------|---------|---------|
| 4443 | QUIC | Control plane (tunnel registration) |
| 8080 | HTTP | Relay HTTP server |
| 9090 | Metrics | Metrics dashboard |

### Network Modes

```bash
# Host network (for local testing)
docker run --network host localup:latest ...

# Bridge network (for container communication)
docker run --network my-network localup:latest ...

# Custom bridge with named containers
docker network create localup-net
docker run --network localup-net --name relay localup:latest relay ...
docker run --network localup-net --name agent localup:latest agent \
  --relay relay:4443 ...
```

## Troubleshooting

### Build Fails: "Bun is not installed"

If building with `Dockerfile.final`, ensure Bun is available:

```bash
# Install Bun in the Docker image or skip web apps
RUN curl -fsSL https://bun.sh/install | bash
```

### Binary: "exec format error"

This means you're trying to run a macOS binary in a Linux container:

**Solution**: Compile for Linux:
```bash
# Cross-compile on macOS
cargo build --release --target x86_64-unknown-linux-gnu

# Or build in a Linux environment
docker run -v $(pwd):/workspace -w /workspace rust:latest \
  cargo build --release --target x86_64-unknown-linux-gnu
```

### Network timeout pulling base images

If Docker Hub is slow:

1. Try again later
2. Use a docker mirror
3. Build locally without pulling:
   ```bash
   docker build --offline -f Dockerfile.ubuntu ...
   ```

## Production Deployment

### Best Practices

1. **Use specific version tags**:
   ```bash
   docker build -t localup:v0.1.0 .
   docker tag localup:v0.1.0 localup:latest
   ```

2. **Push to registry**:
   ```bash
   docker tag localup:latest myregistry.com/localup:latest
   docker push myregistry.com/localup:latest
   ```

3. **Use multi-stage build** for smaller final images

4. **Set resource limits**:
   ```bash
   docker run -m 512m --cpus 2 localup:latest ...
   ```

5. **Use secrets for sensitive data**:
   ```bash
   docker run --secret relay_token localup:latest ...
   ```

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Build Docker Image
  run: docker build -f Dockerfile.ubuntu -t localup:${{ github.sha }} .

- name: Test Docker Image
  run: |
    docker run --rm localup:${{ github.sha }} --version
    docker run --rm localup:${{ github.sha }} --help

- name: Push to Registry
  run: |
    docker tag localup:${{ github.sha }} myregistry.com/localup:latest
    docker push myregistry.com/localup:latest
```

## Size Optimization

Current sizes:
- `Dockerfile.ubuntu` (prebuilt): ~2.25GB
- `Dockerfile.final` (multi-stage): ~2.5GB

To reduce size:
1. Use Alpine Linux instead of Ubuntu
2. Strip symbols from binary: `strip target/release/localup`
3. Use distroless images
4. Remove build dependencies in final stage

Example Alpine-based Dockerfile:

```dockerfile
FROM alpine:latest
RUN apk add --no-cache ca-certificates libssl3
COPY target/release/localup /usr/local/bin/
ENTRYPOINT ["localup"]
```

## Support

For Docker-specific issues:
- Check logs: `docker logs <container-name>`
- View image details: `docker inspect localup:latest`
- Debug interactive: `docker run -it localup:latest /bin/bash`
