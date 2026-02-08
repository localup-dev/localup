# Pull Request

## Title

fix(client): Generate unique tunnel IDs for each protocol configuration

## Description

### Problem

When creating multiple TLS tunnels with the same JWT token, only one tunnel would appear in the UI. This was because the `localup_id` was generated solely from the auth token, causing all tunnels with the same token to get the same ID. Since tunnels are stored in a HashMap keyed by `localup_id`, subsequent tunnels would overwrite previous ones.

### Root Cause

The function `generate_localup_id_from_token()` only hashed the auth token:

```rust
// Before: Same token → Same localup_id (overwrites other tunnels)
fn generate_localup_id_from_token(token: &str) -> String {
    token.hash(&mut hasher);
    // ...
}
```

### Solution

Renamed and updated the function to `generate_localup_id_from_token_and_protocols()` which now includes ALL protocol parameters in the hash:

| Protocol | Parameters included in hash |
|----------|----------------------------|
| **HTTP** | `local_port`, `subdomain`, `custom_domain` |
| **HTTPS** | `local_port`, `subdomain`, `custom_domain` |
| **TCP** | `local_port`, `remote_port` |
| **TLS** | `local_port`, `sni_hostnames[]`, `http_port` |

This ensures:
- ✅ Different TLS tunnels with different SNI hostnames get unique IDs
- ✅ Different HTTP tunnels with different subdomains get unique IDs
- ✅ Different TCP tunnels with different ports get unique IDs
- ✅ Same token + same protocol config = same ID (reconnection support preserved)

### Changes

- **`crates/localup-client/src/localup.rs`**:
  - Renamed `generate_localup_id_from_token()` → `generate_localup_id_from_token_and_protocols()`
  - Added hashing of all protocol parameters (local_port, subdomain, sni_hostnames, etc.)
  - Added 11 unit tests to verify unique ID generation for all parameter combinations

- **`crates/localup-proto/src/messages.rs`**:
  - Fixed clippy warning: replaced manual `impl Default` with `#[derive(Default)]` for `HttpAuthConfig`

- **`crates/localup-cert/src/acme.rs`**:
  - Removed unused `error` import

### Testing

Added 11 unit tests covering:
- Same token + same protocol → same ID
- Same token + different subdomain → different IDs
- Same token + different SNI hostnames → different IDs
- Same token + different local_port → different IDs
- Same token + different remote_port → different IDs
- Same token + different http_port → different IDs
- Different tokens + same protocol → different IDs
- UUID format validation
- Multiple SNI patterns (order matters)

All tests pass:
```
running 11 tests
test localup::tests::test_generate_localup_id_* ... ok
test result: ok. 11 passed; 0 failed
```

### Breaking Changes

None. The change is backward compatible:
- Existing tunnels will get new IDs on next connection (expected behavior)
- Reconnection with same token + same config still gets the same ID
