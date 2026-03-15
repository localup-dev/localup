# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.9] - 2026-03-07

### Fixed

- Force `Connection: close` on proxied HTTP/1.1 requests to prevent connection reuse issues
- Keep QUIC send half alive during H2 passthrough streaming

## [0.1.8] - 2026-03-07

### Fixed

- Rewrite H2 proxy as true bidirectional passthrough for reliable HTTP/2 streaming

## [0.1.7] - 2026-03-06

### Added

- Device authentication and OAuth support
- Exit node portal improvements

### Fixed

- Chunked HTTP/2 proxy handling

## [0.1.6] - 2026-02-21

### Fixed

- Rewrite Host and Origin headers in raw HTTP requests
- Rewrite Host header in transparent streaming

### Changed

- Replace inline test server with dedicated script

## [0.1.5] - 2026-02-16

### Fixed

- Use transparent byte streaming for all HTTP/1.1 requests

## [0.1.4] - 2026-02-08

### Added

- HTTP/2 support with ALPN negotiation

## [0.1.3] - 2026-02-08

### Fixed

- Use HttpProxy for concurrent HTTP request handling

## [0.1.2] - 2026-02-08

### Fixed

- Skip body reading for HTTP 1xx/204/304 and HEAD responses

## [0.1.1] - 2026-02-08

### Added

- Wildcard domain tunnels
- Subdomain access restrictions in JWT claims
- Tauri desktop application (macOS, Linux)
- Tunnel editing in dashboard UI
- HTTP/2 support in HTTPS relay
- TCP connection management

### Fixed

- 5-second latency on chunked HTTP responses
- Generate unique tunnel IDs for each protocol configuration

## [0.0.1-beta] - 2025-10-26 to 2026-02-07

Initial beta period with 70 beta releases. Key milestones:

- Core QUIC-based tunnel protocol
- TCP, TLS/SNI, HTTP, and HTTPS tunnel support
- JWT authentication
- ACME/Let's Encrypt certificate management
- Geo-distributed exit node architecture
- CLI tool and client library
- SeaORM database layer (SQLite, PostgreSQL)
- REST API with OpenAPI/Swagger documentation
- Dashboard web application
- Node.js SDK
- Multi-platform release builds (Linux, macOS)
- Docker support

[Unreleased]: https://github.com/localup-dev/localup/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/localup-dev/localup/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/localup-dev/localup/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/localup-dev/localup/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/localup-dev/localup/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/localup-dev/localup/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/localup-dev/localup/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/localup-dev/localup/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/localup-dev/localup/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/localup-dev/localup/compare/v0.0.1-beta70...v0.1.1
[0.0.1-beta]: https://github.com/localup-dev/localup/releases/tag/v0.0.1-beta1
