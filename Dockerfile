# Multi-stage build for localup tunnel application
# Stage 1: Build stage
FROM rust:1.90-slim AS builder

# Set working directory
WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install Bun for web app builds
RUN curl -fsSL https://bun.sh/install | bash && \
    ln -s /root/.bun/bin/bun /usr/local/bin/bun

# Copy the entire workspace
COPY . .

# Create relays.yaml if it doesn't exist (build requirement)
RUN if [ ! -f relays.yaml ]; then echo "relays: []" > relays.yaml; fi

# Build the release binary (only build the CLI, not the workspace)
RUN cargo build --release --bin localup -p localup-cli

# Stage 2: Runtime stage
# Use Ubuntu 24.04 which has compatible GLIBC version (2.39+)
FROM ubuntu:24.04

# Install runtime dependencies only
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /build/target/release/localup /usr/local/bin/localup

# Make it executable
RUN chmod +x /usr/local/bin/localup

# Verify binary works
RUN /usr/local/bin/localup --version

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD localup --help > /dev/null 2>&1 || exit 1

# Default command
ENTRYPOINT ["localup"]
CMD ["--help"]
