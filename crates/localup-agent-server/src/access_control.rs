//! Access control for agent-server
//!
//! Validates that requested target addresses are allowed based on:
//! - CIDR ranges (e.g., 10.0.0.0/8, 192.168.0.0/16)
//! - Port ranges (e.g., 22, 80-443, 5432)

use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccessControlError {
    #[error("Address {0} is not in allowed CIDR ranges")]
    CidrNotAllowed(IpAddr),

    #[error("Port {0} is not in allowed port ranges")]
    PortNotAllowed(u16),

    #[error("Invalid address format: {0}")]
    InvalidAddress(String),
}

/// Port range specification (inclusive)
#[derive(Debug, Clone)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    pub fn single(port: u16) -> Self {
        Self {
            start: port,
            end: port,
        }
    }

    pub fn range(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

impl std::str::FromStr for PortRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((start, end)) = s.split_once('-') {
            let start = start
                .trim()
                .parse::<u16>()
                .map_err(|e| format!("Invalid start port: {}", e))?;
            let end = end
                .trim()
                .parse::<u16>()
                .map_err(|e| format!("Invalid end port: {}", e))?;
            if start > end {
                return Err(format!("Start port {} > end port {}", start, end));
            }
            Ok(PortRange::range(start, end))
        } else {
            let port = s
                .trim()
                .parse::<u16>()
                .map_err(|e| format!("Invalid port: {}", e))?;
            Ok(PortRange::single(port))
        }
    }
}

/// Access control configuration
#[derive(Debug, Clone)]
pub struct AccessControl {
    /// Allowed CIDR ranges (empty = allow all)
    pub allowed_cidrs: Vec<IpNet>,
    /// Allowed port ranges (empty = allow all)
    pub allowed_ports: Vec<PortRange>,
}

impl AccessControl {
    /// Create new access control with specified rules
    pub fn new(allowed_cidrs: Vec<IpNet>, allowed_ports: Vec<PortRange>) -> Self {
        Self {
            allowed_cidrs,
            allowed_ports,
        }
    }

    /// Create permissive access control (allow everything)
    pub fn allow_all() -> Self {
        Self {
            allowed_cidrs: vec![],
            allowed_ports: vec![],
        }
    }

    /// Check if an address is allowed
    pub fn is_allowed(&self, addr: &SocketAddr) -> Result<(), AccessControlError> {
        // Check IP address
        if !self.allowed_cidrs.is_empty() {
            let ip = addr.ip();
            let allowed = self.allowed_cidrs.iter().any(|cidr| cidr.contains(&ip));
            if !allowed {
                return Err(AccessControlError::CidrNotAllowed(ip));
            }
        }

        // Check port
        if !self.allowed_ports.is_empty() {
            let port = addr.port();
            let allowed = self.allowed_ports.iter().any(|range| range.contains(port));
            if !allowed {
                return Err(AccessControlError::PortNotAllowed(port));
            }
        }

        Ok(())
    }

    /// Parse target address string and validate
    pub fn validate_target(&self, target: &str) -> Result<SocketAddr, AccessControlError> {
        let addr = target
            .parse::<SocketAddr>()
            .map_err(|_| AccessControlError::InvalidAddress(target.to_string()))?;

        self.is_allowed(&addr)?;
        Ok(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_range_single() {
        let range = "22".parse::<PortRange>().unwrap();
        assert!(range.contains(22));
        assert!(!range.contains(23));
    }

    #[test]
    fn test_port_range_multiple() {
        let range = "80-443".parse::<PortRange>().unwrap();
        assert!(range.contains(80));
        assert!(range.contains(443));
        assert!(range.contains(100));
        assert!(!range.contains(79));
        assert!(!range.contains(444));
    }

    #[test]
    fn test_cidr_validation() {
        let cidrs = vec![
            "10.0.0.0/8".parse().unwrap(),
            "192.168.0.0/16".parse().unwrap(),
        ];
        let ac = AccessControl::new(cidrs, vec![]);

        assert!(ac.is_allowed(&"10.50.1.100:5432".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"192.168.1.1:80".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"8.8.8.8:53".parse().unwrap()).is_err());
    }

    #[test]
    fn test_port_validation() {
        let ports = vec![
            PortRange::single(22),
            PortRange::range(80, 443),
            PortRange::single(5432),
        ];
        let ac = AccessControl::new(vec![], ports);

        assert!(ac.is_allowed(&"10.0.0.1:22".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"10.0.0.1:80".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"10.0.0.1:443".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"10.0.0.1:5432".parse().unwrap()).is_ok());
        assert!(ac.is_allowed(&"10.0.0.1:8080".parse().unwrap()).is_err());
    }

    #[test]
    fn test_combined_validation() {
        let cidrs = vec!["192.168.0.0/16".parse().unwrap()];
        let ports = vec![PortRange::range(22, 22), PortRange::range(80, 443)];
        let ac = AccessControl::new(cidrs, ports);

        assert!(ac.validate_target("192.168.1.10:22").is_ok());
        assert!(ac.validate_target("192.168.1.10:80").is_ok());
        assert!(ac.validate_target("192.168.1.10:8080").is_err()); // Port not allowed
        assert!(ac.validate_target("10.0.0.1:22").is_err()); // CIDR not allowed
    }

    #[test]
    fn test_allow_all() {
        let ac = AccessControl::allow_all();
        assert!(ac.validate_target("8.8.8.8:53").is_ok());
        assert!(ac.validate_target("192.168.1.1:8080").is_ok());
    }
}
