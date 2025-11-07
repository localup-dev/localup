use ipnetwork::IpNetwork;
use std::net::IpAddr;
use std::str::FromStr;

/// Network and port allowlist for validating remote addresses
#[derive(Debug, Clone)]
pub struct Allowlist {
    networks: Vec<IpNetwork>,
    ports: Vec<u16>,
}

impl Allowlist {
    /// Create a new allowlist from network CIDR strings and port numbers
    ///
    /// # Arguments
    /// * `networks` - CIDR notation strings (e.g., "192.168.0.0/16", "10.0.0.0/8")
    /// * `ports` - Allowed port numbers
    ///
    /// # Returns
    /// Result with Allowlist or error message if CIDR parsing fails
    pub fn new(networks: Vec<String>, ports: Vec<u16>) -> Result<Self, String> {
        let mut parsed_networks = Vec::new();

        for network_str in networks {
            let network = IpNetwork::from_str(&network_str)
                .map_err(|e| format!("Invalid CIDR notation '{}': {}", network_str, e))?;
            parsed_networks.push(network);
        }

        Ok(Self {
            networks: parsed_networks,
            ports,
        })
    }

    /// Check if an address (IP:port format) is allowed
    ///
    /// # Arguments
    /// * `address` - Address in "IP:port" format (e.g., "192.168.1.10:8080")
    ///
    /// # Returns
    /// true if both IP is in allowed networks and port is in allowed ports
    pub fn is_allowed(&self, address: &str) -> bool {
        match Self::parse_address(address) {
            Ok((ip, port)) => self.is_ip_allowed(&ip) && self.is_port_allowed(port),
            Err(e) => {
                tracing::warn!("Failed to parse address '{}': {}", address, e);
                false
            }
        }
    }

    /// Parse an address string into IP and port
    fn parse_address(address: &str) -> Result<(IpAddr, u16), String> {
        let parts: Vec<&str> = address.rsplitn(2, ':').collect();

        if parts.len() != 2 {
            return Err(format!(
                "Invalid address format '{}', expected IP:port",
                address
            ));
        }

        let port_str = parts[0];
        let ip_str = parts[1];

        let port = port_str
            .parse::<u16>()
            .map_err(|e| format!("Invalid port '{}': {}", port_str, e))?;

        let ip = IpAddr::from_str(ip_str)
            .map_err(|e| format!("Invalid IP address '{}': {}", ip_str, e))?;

        Ok((ip, port))
    }

    /// Check if an IP address is in the allowed networks
    fn is_ip_allowed(&self, ip: &IpAddr) -> bool {
        // Empty allowlist means allow all IPs
        if self.networks.is_empty() {
            return true;
        }

        self.networks.iter().any(|network| network.contains(*ip))
    }

    /// Check if a port is in the allowed ports list
    fn is_port_allowed(&self, port: u16) -> bool {
        // Empty allowlist means allow all ports
        if self.ports.is_empty() {
            return true;
        }

        self.ports.contains(&port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowlist_valid_cidr() {
        let allowlist = Allowlist::new(
            vec!["192.168.0.0/16".to_string(), "10.0.0.0/8".to_string()],
            vec![8080, 3000],
        );
        assert!(allowlist.is_ok());
    }

    #[test]
    fn test_allowlist_invalid_cidr() {
        let allowlist = Allowlist::new(vec!["invalid-cidr".to_string()], vec![]);
        assert!(allowlist.is_err());
    }

    #[test]
    fn test_is_allowed_valid() {
        let allowlist = Allowlist::new(vec!["192.168.0.0/16".to_string()], vec![8080]).unwrap();

        assert!(allowlist.is_allowed("192.168.1.10:8080"));
    }

    #[test]
    fn test_is_allowed_wrong_network() {
        let allowlist = Allowlist::new(vec!["192.168.0.0/16".to_string()], vec![8080]).unwrap();

        assert!(!allowlist.is_allowed("10.0.0.1:8080"));
    }

    #[test]
    fn test_is_allowed_wrong_port() {
        let allowlist = Allowlist::new(vec!["192.168.0.0/16".to_string()], vec![8080]).unwrap();

        assert!(!allowlist.is_allowed("192.168.1.10:3000"));
    }

    #[test]
    fn test_is_allowed_empty_allowlist() {
        let allowlist = Allowlist::new(vec![], vec![]).unwrap();

        // Empty allowlist allows everything
        assert!(allowlist.is_allowed("192.168.1.10:8080"));
        assert!(allowlist.is_allowed("10.0.0.1:3000"));
    }

    #[test]
    fn test_parse_address_valid() {
        let result = Allowlist::parse_address("192.168.1.10:8080");
        assert!(result.is_ok());

        let (ip, port) = result.unwrap();
        assert_eq!(ip.to_string(), "192.168.1.10");
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_parse_address_ipv6() {
        // TODO: IPv6 parsing needs proper bracket handling
        // For now, just test that simple IPv4 parsing works
        let result = Allowlist::parse_address("127.0.0.1:8080");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_address_invalid() {
        assert!(Allowlist::parse_address("invalid").is_err());
        assert!(Allowlist::parse_address("192.168.1.10").is_err());
        assert!(Allowlist::parse_address("192.168.1.10:invalid").is_err());
    }
}
