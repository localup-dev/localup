//! TLS SNI-based routing

use crate::{RouteKey, RouteRegistry, RouteTarget};
use localup_proto::IpFilter;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, trace};

/// SNI routing errors
#[derive(Debug, Error)]
pub enum SniRouterError {
    #[error("Route error: {0}")]
    RouteError(#[from] crate::registry::RouteError),

    #[error("Invalid SNI hostname: {0}")]
    InvalidSni(String),

    #[error("SNI extraction failed")]
    SniExtractionFailed,
}

/// SNI route information
#[derive(Debug, Clone)]
pub struct SniRoute {
    pub sni_hostname: String,
    pub localup_id: String,
    pub target_addr: String,
    /// IP filter for access control (empty allows all)
    pub ip_filter: IpFilter,
}

/// SNI router for TLS connections
pub struct SniRouter {
    registry: Arc<RouteRegistry>,
}

impl SniRouter {
    pub fn new(registry: Arc<RouteRegistry>) -> Self {
        Self { registry }
    }

    /// Register an SNI route
    pub fn register_route(&self, route: SniRoute) -> Result<(), SniRouterError> {
        debug!(
            "Registering SNI route: {} -> {}",
            route.sni_hostname, route.target_addr
        );

        let key = RouteKey::TlsSni(route.sni_hostname.clone());
        let target = RouteTarget {
            localup_id: route.localup_id,
            target_addr: route.target_addr,
            metadata: None,
            ip_filter: route.ip_filter,
        };

        self.registry.register(key, target)?;
        Ok(())
    }

    /// Lookup route by SNI hostname
    pub fn lookup(&self, sni_hostname: &str) -> Result<RouteTarget, SniRouterError> {
        trace!("Looking up SNI route for hostname: {}", sni_hostname);

        let key = RouteKey::TlsSni(sni_hostname.to_string());
        let target = self.registry.lookup(&key)?;

        Ok(target)
    }

    /// Unregister an SNI route
    pub fn unregister(&self, sni_hostname: &str) -> Result<(), SniRouterError> {
        debug!("Unregistering SNI route for hostname: {}", sni_hostname);

        let key = RouteKey::TlsSni(sni_hostname.to_string());
        self.registry.unregister(&key)?;

        Ok(())
    }

    /// Check if SNI has a route
    pub fn has_route(&self, sni_hostname: &str) -> bool {
        let key = RouteKey::TlsSni(sni_hostname.to_string());
        self.registry.exists(&key)
    }

    /// Extract SNI from TLS ClientHello
    /// Parses the TLS handshake to extract the Server Name Indication (SNI) extension
    pub fn extract_sni(client_hello: &[u8]) -> Result<String, SniRouterError> {
        // Skip TLS record header (5 bytes) and handshake header (4 bytes)
        if client_hello.len() < 43 {
            return Err(SniRouterError::SniExtractionFailed);
        }

        let mut offset = 9; // Skip record header (5) + handshake header (4)

        // Skip ClientHello version (2 bytes)
        offset += 2;

        // Skip random (32 bytes)
        offset += 32;

        // Skip session ID
        if offset >= client_hello.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }
        let session_id_len = client_hello[offset] as usize;
        offset += 1 + session_id_len;

        // Skip cipher suites
        if offset + 2 > client_hello.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }
        let cipher_suites_len =
            u16::from_be_bytes([client_hello[offset], client_hello[offset + 1]]) as usize;
        offset += 2 + cipher_suites_len;

        // Skip compression methods
        if offset >= client_hello.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }
        let compression_methods_len = client_hello[offset] as usize;
        offset += 1 + compression_methods_len;

        // Parse extensions
        if offset + 2 > client_hello.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }
        let extensions_len =
            u16::from_be_bytes([client_hello[offset], client_hello[offset + 1]]) as usize;
        offset += 2;

        let extensions_end = offset + extensions_len;
        if extensions_end > client_hello.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }

        // Search for server_name extension (type 0x0000)
        while offset + 4 <= extensions_end {
            let ext_type = u16::from_be_bytes([client_hello[offset], client_hello[offset + 1]]);
            let ext_len =
                u16::from_be_bytes([client_hello[offset + 2], client_hello[offset + 3]]) as usize;
            offset += 4;

            if ext_type == 0x0000 {
                // Found server_name extension
                return Self::parse_sni_extension(&client_hello[offset..offset + ext_len]);
            }

            offset += ext_len;
        }

        Err(SniRouterError::SniExtractionFailed)
    }

    /// Parse the server_name extension data
    fn parse_sni_extension(data: &[u8]) -> Result<String, SniRouterError> {
        if data.len() < 5 {
            return Err(SniRouterError::SniExtractionFailed);
        }

        // Skip server_name_list length (2 bytes)
        let mut offset = 2;

        // Skip name_type (1 byte, should be 0 for host_name)
        if data[offset] != 0 {
            return Err(SniRouterError::InvalidSni("Invalid name type".to_string()));
        }
        offset += 1;

        // Get host_name length (2 bytes)
        let name_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        if offset + name_len > data.len() {
            return Err(SniRouterError::SniExtractionFailed);
        }

        let hostname = String::from_utf8(data[offset..offset + name_len].to_vec())
            .map_err(|_| SniRouterError::InvalidSni("Invalid UTF-8 in hostname".to_string()))?;

        if hostname.is_empty() {
            return Err(SniRouterError::InvalidSni("Empty hostname".to_string()));
        }

        trace!("Extracted SNI hostname: {}", hostname);
        Ok(hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sni_router() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let route = SniRoute {
            sni_hostname: "db.example.com".to_string(),
            localup_id: "localup-db".to_string(),
            target_addr: "localhost:5432".to_string(),
            ip_filter: IpFilter::new(),
        };

        router.register_route(route).unwrap();

        assert!(router.has_route("db.example.com"));

        let target = router.lookup("db.example.com").unwrap();
        assert_eq!(target.localup_id, "localup-db");

        router.unregister("db.example.com").unwrap();
        assert!(!router.has_route("db.example.com"));
    }

    #[test]
    fn test_sni_router_not_found() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let result = router.lookup("unknown.example.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_wildcard_sni() {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry);

        let route = SniRoute {
            sni_hostname: "*.example.com".to_string(),
            localup_id: "localup-wildcard".to_string(),
            target_addr: "localhost:3000".to_string(),
            ip_filter: IpFilter::new(),
        };

        router.register_route(route).unwrap();

        // Exact match works
        assert!(router.has_route("*.example.com"));

        // Note: Wildcard matching would require additional logic
        // This test just verifies exact registration/lookup works
    }

    #[test]
    fn test_sni_extraction() {
        // Valid TLS ClientHello with SNI extension
        let mut client_hello = Vec::new();

        // TLS Record Header (5 bytes)
        client_hello.push(0x16); // Content type: Handshake
        client_hello.push(0x03); // Version TLS 1.2 (major)
        client_hello.push(0x03); // Version TLS 1.2 (minor)

        // Total length will be calculated below
        let length_index = client_hello.len();
        client_hello.push(0x00); // Placeholder for length high byte
        client_hello.push(0x00); // Placeholder for length low byte

        // Handshake Header (4 bytes)
        client_hello.push(0x01); // Msg type: ClientHello
        let handshake_length_index = client_hello.len();
        client_hello.push(0x00); // Placeholder for length
        client_hello.push(0x00); // Placeholder for length
        client_hello.push(0x00); // Placeholder for length

        // ClientHello Protocol Version (2 bytes)
        client_hello.push(0x03); // TLS 1.2
        client_hello.push(0x03);

        // Random (32 bytes)
        client_hello.extend_from_slice(&[0x00; 32]);

        // Session ID length (1 byte)
        client_hello.push(0x00);

        // Cipher suites length (2 bytes) - 2 suites
        client_hello.push(0x00);
        client_hello.push(0x04);

        // Cipher suites (2 x 2 = 4 bytes)
        client_hello.push(0x00);
        client_hello.push(0x2f); // TLS_RSA_WITH_AES_128_CBC_SHA
        client_hello.push(0x00);
        client_hello.push(0x35); // TLS_RSA_WITH_AES_256_CBC_SHA

        // Compression methods length (1 byte)
        client_hello.push(0x01);

        // Compression methods
        client_hello.push(0x00); // null compression

        // Extensions length (2 bytes)
        let extensions_length_index = client_hello.len();
        client_hello.push(0x00); // Placeholder
        client_hello.push(0x00); // Placeholder

        // SNI Extension
        let extension_start = client_hello.len();
        client_hello.push(0x00); // Type: server_name
        client_hello.push(0x00);
        client_hello.push(0x00); // Length (will update)
        client_hello.push(0x00);

        // Server name list
        let sni_list_length_index = client_hello.len();
        client_hello.push(0x00); // Length (will update)
        client_hello.push(0x00);

        // Server name
        client_hello.push(0x00); // Type: host_name
        client_hello.push(0x00); // Name length
        let hostname = b"example.test";
        client_hello.push(hostname.len() as u8);
        client_hello.extend_from_slice(hostname);

        // Update SNI list length
        let sni_list_len = client_hello.len() - sni_list_length_index - 2;
        client_hello[sni_list_length_index] = (sni_list_len >> 8) as u8;
        client_hello[sni_list_length_index + 1] = sni_list_len as u8;

        // Update extension length
        let extension_len = client_hello.len() - extension_start - 4;
        client_hello[extension_start + 2] = (extension_len >> 8) as u8;
        client_hello[extension_start + 3] = extension_len as u8;

        // Update extensions length
        let extensions_len = client_hello.len() - extensions_length_index - 2;
        client_hello[extensions_length_index] = (extensions_len >> 8) as u8;
        client_hello[extensions_length_index + 1] = extensions_len as u8;

        // Update handshake length
        let handshake_len = client_hello.len() - handshake_length_index - 3;
        client_hello[handshake_length_index] = ((handshake_len >> 16) & 0xFF) as u8;
        client_hello[handshake_length_index + 1] = ((handshake_len >> 8) & 0xFF) as u8;
        client_hello[handshake_length_index + 2] = (handshake_len & 0xFF) as u8;

        // Update record length
        let record_len = client_hello.len() - length_index - 2;
        client_hello[length_index] = (record_len >> 8) as u8;
        client_hello[length_index + 1] = record_len as u8;

        let result = SniRouter::extract_sni(&client_hello);
        assert!(result.is_ok(), "SNI extraction failed: {:?}", result);
        assert_eq!(result.unwrap(), "example.test");
    }

    #[test]
    fn test_sni_extraction_not_found() {
        // ClientHello without SNI extension
        let client_hello = vec![
            // TLS Record Header
            0x16, 0x03, 0x01, 0x00, 0x4A, // type=22, version=TLS1.0, length=74
            // Handshake Header
            0x01, 0x00, 0x00, 0x46, // msg_type=1, length=70
            // ClientHello Body
            0x03, 0x03, // version TLS 1.2
            // 32 random bytes
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f, // Session ID length
            0x00, // Cipher suites length
            0x00, 0x02, // Cipher suite
            0x00, 0x2f, // Compression methods length
            0x01, // Compression method
            0x00, // Extensions length (no extensions)
            0x00, 0x00,
        ];

        let result = SniRouter::extract_sni(&client_hello);
        assert!(result.is_err());
    }

    #[test]
    fn test_sni_extraction_malformed() {
        // Malformed ClientHello (too short)
        let client_hello = vec![0x16, 0x03, 0x01];
        let result = SniRouter::extract_sni(&client_hello);
        assert!(result.is_err());
    }
}
