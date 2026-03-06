# Contributing to localup

Thank you for your interest in contributing to localup! This guide will help you get started.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Code Style](#code-style)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Issue Guidelines](#issue-guidelines)

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally
3. **Create a branch** for your changes
4. **Make your changes** with tests
5. **Submit a pull request**

## Development Setup

### Prerequisites

- **Rust** 1.90.0 or later (install via [rustup](https://rustup.rs/))
- **OpenSSL** (for TLS certificate generation in tests)
- **Bun** (for web applications in `webapps/`)

### Building

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/localup.git
cd localup

# Build the workspace
cargo build

# Build a specific crate
cargo build -p localup-client
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p localup-proto

# Run tests with output
cargo test -- --nocapture
```

## Making Changes

### Branch Naming

Use descriptive branch names with prefixes:

- `feat/` - New features (e.g., `feat/websocket-compression`)
- `fix/` - Bug fixes (e.g., `fix/reconnection-timeout`)
- `docs/` - Documentation changes (e.g., `docs/api-reference`)
- `chore/` - Maintenance tasks (e.g., `chore/update-dependencies`)
- `refactor/` - Code refactoring (e.g., `refactor/transport-layer`)

### Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/). Each commit message should be structured as:

```
type(scope): description

[optional body]

[optional footer(s)]
```

**Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`, `ci`

**Examples**:
```
feat(transport): add HTTP/2 transport support
fix(client): handle reconnection on certificate rotation
docs(readme): add WebSocket transport example
test(router): add SNI routing edge case tests
```

A pre-commit hook validates commit message format automatically.

## Code Style

### Rust

- Format with `cargo fmt` before every commit
- No clippy warnings allowed (`cargo clippy -- -D warnings`)
- Use `thiserror` for custom error types
- Use `tokio::sync` primitives (never `std::sync`)
- Follow existing patterns in the codebase

### Mandatory Checks

Before submitting a PR, run these commands (they match our CI pipeline):

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

All three must pass. PRs with failing CI will not be reviewed.

### Documentation

- Add rustdoc comments (`///`) to all public types and functions
- Include code examples in doc comments where helpful
- Update README.md if your change affects user-facing behavior

## Testing

### Requirements

- All new features must include unit tests
- All new public APIs must include integration tests
- Critical paths should have >90% coverage
- Use `#[tokio::test]` for async tests

### Test Organization

```
crates/localup-foo/
  src/
    lib.rs        # Unit tests in #[cfg(test)] mod tests {}
  tests/
    integration.rs  # Integration tests
```

### What to Test

- **Happy path**: Basic successful operation
- **Error cases**: Invalid input, network failures, timeouts
- **Edge cases**: Empty input, boundary values, concurrent access
- **Recovery**: Graceful handling after errors

## Submitting Changes

### Pull Request Process

1. Update documentation for any user-facing changes
2. Add or update tests as needed
3. Ensure all CI checks pass
4. Fill out the PR template completely
5. Request review from maintainers

### PR Guidelines

- Keep PRs focused -- one feature or fix per PR
- Provide context in the description (what, why, how)
- Link related issues (e.g., "Closes #42")
- Include screenshots or terminal output for UI/behavior changes
- Be responsive to review feedback

### Review Process

- At least one maintainer approval is required
- CI must pass before merging
- Reviewers may request changes -- this is normal and constructive
- We aim to review PRs within 1 week

## Issue Guidelines

### Bug Reports

Use the bug report template and include:
- localup version (`localup --version`)
- Operating system and architecture
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs (with `--log-level debug`)

### Feature Requests

Use the feature request template and include:
- Problem description (what pain point does this solve?)
- Proposed solution
- Alternative approaches you considered
- Impact on existing functionality

### Good First Issues

Look for issues labeled [`good first issue`](https://github.com/localup-dev/localup/labels/good%20first%20issue) -- these are ideal starting points for new contributors.

## Architecture

The project is organized as a Rust workspace with focused crates. See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation including:

- Workspace structure and crate responsibilities
- Protocol flow and multiplexing architecture
- Key design decisions
- Performance targets

## License

By contributing to localup, you agree that your contributions will be licensed under the project's dual MIT/Apache-2.0 license.
