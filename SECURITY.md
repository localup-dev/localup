# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

If you discover a security vulnerability in localup, please report it responsibly:

1. **Email**: Send details to [security@localup.dev](mailto:security@localup.dev)
2. **GitHub Security Advisories**: Use [GitHub's private vulnerability reporting](https://github.com/localup-dev/localup/security/advisories/new)

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial assessment**: Within 1 week
- **Fix timeline**: Depends on severity, typically within 30 days for critical issues

### Severity Classification

| Severity | Description | Response |
|----------|-------------|----------|
| **Critical** | Remote code execution, authentication bypass | Patch within 72 hours |
| **High** | Data exposure, privilege escalation | Patch within 1 week |
| **Medium** | Denial of service, information disclosure | Patch within 30 days |
| **Low** | Minor issues, hardening opportunities | Next regular release |

## Security Considerations

localup handles network traffic and tunneling, which makes security critical:

- **TLS 1.3**: All public-facing connections use TLS 1.3
- **QUIC Transport**: Built-in TLS 1.3 for tunnel connections
- **JWT Authentication**: Token-based authorization with HMAC-SHA256 signing
- **No Plaintext Secrets**: Secrets are never logged or stored in plaintext

### Best Practices for Operators

- Use strong, unique JWT secrets (32+ characters)
- Rotate JWT secrets periodically
- Use proper TLS certificates (not self-signed) in production
- Restrict TCP port ranges to minimize exposure
- Keep localup updated to the latest version
- Monitor relay logs for unusual authentication patterns

## Disclosure Policy

We follow [coordinated disclosure](https://en.wikipedia.org/wiki/Coordinated_vulnerability_disclosure). We ask that you:

- Allow us reasonable time to fix the issue before public disclosure
- Do not exploit the vulnerability beyond what is necessary to demonstrate it
- Do not access or modify data belonging to others

We will credit reporters in security advisories (unless you prefer to remain anonymous).
