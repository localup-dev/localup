//! IP address filtering with CIDR support
//!
//! This module provides IP-based access control for tunnels.
//! Supports both individual IP addresses and CIDR notation (e.g., "192.168.0.0/16").

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;

/// IP filter for controlling access to tunnels based on source IP address.
///
/// Supports:
/// - Individual IP addresses (e.g., "192.168.1.100")
/// - CIDR notation (e.g., "10.0.0.0/8", "192.168.0.0/16")
/// - IPv4 and IPv6 addresses
///
/// An empty filter allows all connections (default behavior).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IpFilter {
    /// List of allowed IP addresses or CIDR ranges
    /// Empty list means all IPs are allowed
    allowlist: Vec<String>,
    /// Parsed CIDR networks for efficient matching
    #[serde(skip)]
    networks: Vec<IpNetwork>,
}

/// Represents an IP network (CIDR)
#[derive(Debug, Clone, PartialEq)]
struct IpNetwork {
    /// Base IP address
    addr: IpAddr,
    /// Network prefix length (e.g., 24 for /24)
    prefix_len: u8,
}

impl IpNetwork {
    /// Parse a CIDR string like "192.168.0.0/16" or a single IP like "192.168.1.1"
    fn parse(s: &str) -> Result<Self, IpFilterError> {
        if let Some((ip_str, prefix_str)) = s.split_once('/') {
            let addr = IpAddr::from_str(ip_str)
                .map_err(|_| IpFilterError::InvalidIpAddress(s.to_string()))?;
            let prefix_len = prefix_str
                .parse::<u8>()
                .map_err(|_| IpFilterError::InvalidCidr(s.to_string()))?;

            // Validate prefix length
            let max_prefix = match addr {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };

            if prefix_len > max_prefix {
                return Err(IpFilterError::InvalidCidr(s.to_string()));
            }

            Ok(Self { addr, prefix_len })
        } else {
            // Single IP address - treat as /32 or /128
            let addr =
                IpAddr::from_str(s).map_err(|_| IpFilterError::InvalidIpAddress(s.to_string()))?;
            let prefix_len = match addr {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            Ok(Self { addr, prefix_len })
        }
    }

    /// Check if an IP address is contained in this network
    fn contains(&self, ip: &IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net_ip), IpAddr::V4(test_ip)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u32::from(net_ip);
                let test_bits = u32::from(*test_ip);
                let mask = !0u32 << (32 - self.prefix_len);
                (net_bits & mask) == (test_bits & mask)
            }
            (IpAddr::V6(net_ip), IpAddr::V6(test_ip)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u128::from(net_ip);
                let test_bits = u128::from(*test_ip);
                let mask = !0u128 << (128 - self.prefix_len);
                (net_bits & mask) == (test_bits & mask)
            }
            // IPv4 and IPv6 don't match
            _ => false,
        }
    }
}

/// IP filter errors
#[derive(Debug, Clone, PartialEq)]
pub enum IpFilterError {
    /// Invalid IP address format
    InvalidIpAddress(String),
    /// Invalid CIDR notation
    InvalidCidr(String),
}

impl std::fmt::Display for IpFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpFilterError::InvalidIpAddress(s) => write!(f, "Invalid IP address: {}", s),
            IpFilterError::InvalidCidr(s) => write!(f, "Invalid CIDR notation: {}", s),
        }
    }
}

impl std::error::Error for IpFilterError {}

impl IpFilter {
    /// Create a new empty IP filter (allows all connections)
    pub fn new() -> Self {
        Self {
            allowlist: Vec::new(),
            networks: Vec::new(),
        }
    }

    /// Create an IP filter from a list of IP addresses or CIDR ranges
    ///
    /// # Arguments
    /// * `allowlist` - List of IP addresses or CIDR ranges (e.g., ["192.168.0.0/16", "10.0.0.1"])
    ///
    /// # Returns
    /// Result with IpFilter or error if any entry is invalid
    pub fn from_allowlist(allowlist: Vec<String>) -> Result<Self, IpFilterError> {
        let mut networks = Vec::with_capacity(allowlist.len());

        for entry in &allowlist {
            let network = IpNetwork::parse(entry)?;
            networks.push(network);
        }

        Ok(Self {
            allowlist,
            networks,
        })
    }

    /// Check if an IP address is allowed by this filter
    ///
    /// Returns true if:
    /// - The allowlist is empty (no filtering)
    /// - The IP matches any entry in the allowlist
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        // Empty allowlist means allow all
        if self.networks.is_empty() {
            return true;
        }

        self.networks.iter().any(|network| network.contains(ip))
    }

    /// Check if a socket address is allowed by this filter
    ///
    /// Extracts the IP from the socket address and checks it
    pub fn is_socket_allowed(&self, addr: &std::net::SocketAddr) -> bool {
        self.is_allowed(&addr.ip())
    }

    /// Get the allowlist entries
    pub fn allowlist(&self) -> &[String] {
        &self.allowlist
    }

    /// Check if the filter is empty (allows all)
    pub fn is_empty(&self) -> bool {
        self.allowlist.is_empty()
    }

    /// Get the number of entries in the filter
    pub fn len(&self) -> usize {
        self.allowlist.len()
    }

    /// Initialize internal network cache from allowlist
    /// Called after deserialization to rebuild the parsed networks
    pub fn init(&mut self) -> Result<(), IpFilterError> {
        self.networks.clear();
        for entry in &self.allowlist {
            let network = IpNetwork::parse(entry)?;
            self.networks.push(network);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn test_empty_filter_allows_all() {
        let filter = IpFilter::new();
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(filter.is_allowed(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn test_single_ip_filter() {
        let filter = IpFilter::from_allowlist(vec!["192.168.1.100".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn test_cidr_filter_class_c() {
        let filter = IpFilter::from_allowlist(vec!["192.168.1.0/24".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 255))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 2, 1))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn test_cidr_filter_class_b() {
        let filter = IpFilter::from_allowlist(vec!["172.16.0.0/16".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(172, 16, 255, 255))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1))));
    }

    #[test]
    fn test_cidr_filter_class_a() {
        let filter = IpFilter::from_allowlist(vec!["10.0.0.0/8".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(11, 0, 0, 1))));
    }

    #[test]
    fn test_multiple_entries() {
        let filter = IpFilter::from_allowlist(vec![
            "192.168.1.0/24".to_string(),
            "10.0.0.0/8".to_string(),
            "203.0.113.50".to_string(),
        ])
        .unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 50, 100, 200))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(203, 0, 113, 50))));
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn test_ipv6_single_address() {
        let filter = IpFilter::from_allowlist(vec!["::1".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!filter.is_allowed(&IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))));
    }

    #[test]
    fn test_ipv6_cidr() {
        let filter = IpFilter::from_allowlist(vec!["2001:db8::/32".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1))));
        assert!(filter.is_allowed(&IpAddr::V6(Ipv6Addr::new(
            0x2001, 0x0db8, 0xffff, 0xffff, 0, 0, 0, 1
        ))));
        assert!(!filter.is_allowed(&IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db9, 0, 0, 0, 0, 0, 1))));
    }

    #[test]
    fn test_mixed_ipv4_ipv6() {
        let filter = IpFilter::from_allowlist(vec![
            "192.168.1.0/24".to_string(),
            "2001:db8::/32".to_string(),
        ])
        .unwrap();

        // IPv4 should match IPv4 rule
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        // IPv6 should match IPv6 rule
        assert!(filter.is_allowed(&IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1))));
        // Unmatched addresses
        assert!(!filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!filter.is_allowed(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn test_socket_addr_filter() {
        let filter = IpFilter::from_allowlist(vec!["192.168.1.0/24".to_string()]).unwrap();

        let allowed_addr: SocketAddr = "192.168.1.100:8080".parse().unwrap();
        let denied_addr: SocketAddr = "10.0.0.1:8080".parse().unwrap();

        assert!(filter.is_socket_allowed(&allowed_addr));
        assert!(!filter.is_socket_allowed(&denied_addr));
    }

    #[test]
    fn test_invalid_ip_address() {
        let result = IpFilter::from_allowlist(vec!["not-an-ip".to_string()]);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IpFilterError::InvalidIpAddress(_)
        ));
    }

    #[test]
    fn test_invalid_cidr_prefix() {
        let result = IpFilter::from_allowlist(vec!["192.168.1.0/33".to_string()]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), IpFilterError::InvalidCidr(_)));
    }

    #[test]
    fn test_invalid_cidr_format() {
        let result = IpFilter::from_allowlist(vec!["192.168.1.0/abc".to_string()]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), IpFilterError::InvalidCidr(_)));
    }

    #[test]
    fn test_filter_accessors() {
        let filter =
            IpFilter::from_allowlist(vec!["192.168.1.0/24".to_string(), "10.0.0.1".to_string()])
                .unwrap();

        assert_eq!(filter.len(), 2);
        assert!(!filter.is_empty());
        assert_eq!(filter.allowlist().len(), 2);
    }

    #[test]
    fn test_filter_serialization() {
        let filter = IpFilter::from_allowlist(vec!["192.168.1.0/24".to_string()]).unwrap();

        let json = serde_json::to_string(&filter).unwrap();
        let mut deserialized: IpFilter = serde_json::from_str(&json).unwrap();

        // Need to re-initialize after deserialization
        deserialized.init().unwrap();

        assert!(deserialized.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(!deserialized.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn test_zero_prefix() {
        // /0 should match everything of the same IP version
        let filter = IpFilter::from_allowlist(vec!["0.0.0.0/0".to_string()]).unwrap();

        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(filter.is_allowed(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        // But not IPv6
        assert!(!filter.is_allowed(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }
}
