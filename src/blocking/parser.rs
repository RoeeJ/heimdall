/// Blocklist parser for various formats
use std::net::IpAddr;

/// Supported blocklist formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlocklistFormat {
    /// Simple domain list (one domain per line)
    DomainList,
    /// Hosts file format (IP domain)
    Hosts,
    /// AdBlock Plus format
    AdBlockPlus,
    /// Pi-hole format
    PiHole,
    /// dnsmasq format
    Dnsmasq,
    /// Unbound format
    Unbound,
}

pub struct BlocklistParser {
    format: BlocklistFormat,
}

impl BlocklistParser {
    pub fn new(format: BlocklistFormat) -> Self {
        Self { format }
    }

    /// Parse a single line and extract domain if valid
    pub fn parse_line(&self, line: &str) -> Option<String> {
        // Skip empty lines and comments
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            return None;
        }

        match self.format {
            BlocklistFormat::DomainList => self.parse_domain_list(line),
            BlocklistFormat::Hosts => self.parse_hosts_format(line),
            BlocklistFormat::AdBlockPlus => self.parse_adblock_format(line),
            BlocklistFormat::PiHole => self.parse_pihole_format(line),
            BlocklistFormat::Dnsmasq => self.parse_dnsmasq_format(line),
            BlocklistFormat::Unbound => self.parse_unbound_format(line),
        }
    }

    /// Parse simple domain list format
    fn parse_domain_list(&self, line: &str) -> Option<String> {
        // Just return the domain if it's valid
        let domain = line.trim();
        if self.is_valid_domain(domain) {
            Some(domain.to_string())
        } else {
            None
        }
    }

    /// Parse hosts file format: IP domain [aliases...]
    fn parse_hosts_format(&self, line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        // First part should be an IP address
        if parts[0].parse::<IpAddr>().is_err() {
            return None;
        }

        // Second part is the domain
        let domain = parts[1];
        if self.is_valid_domain(domain) && domain != "localhost" {
            Some(domain.to_string())
        } else {
            None
        }
    }

    /// Parse AdBlock Plus format
    fn parse_adblock_format(&self, line: &str) -> Option<String> {
        // Skip non-domain rules
        if line.starts_with("@@") || line.contains('$') || line.contains('/') {
            return None;
        }

        let mut domain = line;

        // Handle ||domain^ format
        if domain.starts_with("||") {
            domain = &domain[2..];
        }

        // Remove trailing ^ or |
        domain = domain.trim_end_matches('^').trim_end_matches('|');

        // Handle wildcards
        if domain.contains('*') {
            // Convert simple wildcards to our format
            if domain.starts_with("*.") {
                return Some(domain.to_string());
            } else if !domain.contains("**") {
                // Single wildcard - treat as subdomain wildcard
                domain = domain.trim_start_matches('*');
                if domain.starts_with('.') {
                    return Some(format!("*{}", domain));
                }
            }
            return None;
        }

        if self.is_valid_domain(domain) {
            Some(domain.to_string())
        } else {
            None
        }
    }

    /// Parse Pi-hole format (similar to hosts but with some extensions)
    fn parse_pihole_format(&self, line: &str) -> Option<String> {
        // Pi-hole supports multiple formats, try hosts format first
        if let Some(domain) = self.parse_hosts_format(line) {
            return Some(domain);
        }

        // Also supports bare domains
        self.parse_domain_list(line)
    }

    /// Parse dnsmasq format: address=/domain/IP or server=/domain/#
    fn parse_dnsmasq_format(&self, line: &str) -> Option<String> {
        if let Some(stripped) = line.strip_prefix("address=/") {
            let parts: Vec<&str> = stripped.split('/').collect();
            if parts.len() >= 2 {
                let domain = parts[0];
                if self.is_valid_domain(domain) {
                    return Some(domain.to_string());
                }
            }
        } else if let Some(stripped) = line.strip_prefix("server=/") {
            let parts: Vec<&str> = stripped.split('/').collect();
            if parts.len() >= 2 && parts[1] == "#" {
                let domain = parts[0];
                if self.is_valid_domain(domain) {
                    return Some(domain.to_string());
                }
            }
        }
        None
    }

    /// Parse unbound format: local-zone: "domain" refuse
    fn parse_unbound_format(&self, line: &str) -> Option<String> {
        if let Some(stripped) = line.strip_prefix("local-zone:") {
            let rest = stripped.trim();
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest[start + 1..].find('"') {
                    let domain = &rest[start + 1..start + 1 + end];
                    if self.is_valid_domain(domain) {
                        return Some(domain.to_string());
                    }
                }
            }
        }
        None
    }

    /// Check if a domain is valid
    fn is_valid_domain(&self, domain: &str) -> bool {
        if domain.is_empty() || domain.len() > 253 {
            return false;
        }

        // Check for valid characters and structure
        let parts: Vec<&str> = domain.split('.').collect();
        if parts.is_empty() {
            return false;
        }

        for part in parts {
            if part.is_empty() || part.len() > 63 {
                return false;
            }

            // Allow wildcards at the start
            if part == "*" {
                continue;
            }

            // Check for valid label characters
            for (i, ch) in part.chars().enumerate() {
                if i == 0 || i == part.len() - 1 {
                    // First and last char must be alphanumeric
                    if !ch.is_alphanumeric() {
                        return false;
                    }
                } else {
                    // Middle chars can be alphanumeric or hyphen
                    if !ch.is_alphanumeric() && ch != '-' {
                        return false;
                    }
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_list_parser() {
        let parser = BlocklistParser::new(BlocklistFormat::DomainList);

        assert_eq!(
            parser.parse_line("example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("  example.com  "),
            Some("example.com".to_string())
        );
        assert_eq!(parser.parse_line("# comment"), None);
        assert_eq!(parser.parse_line(""), None);
        assert_eq!(
            parser.parse_line("*.example.com"),
            Some("*.example.com".to_string())
        );
    }

    #[test]
    fn test_hosts_parser() {
        let parser = BlocklistParser::new(BlocklistFormat::Hosts);

        assert_eq!(
            parser.parse_line("0.0.0.0 ads.example.com"),
            Some("ads.example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("127.0.0.1 tracker.com"),
            Some("tracker.com".to_string())
        );
        assert_eq!(
            parser.parse_line("::1 ipv6.example.com"),
            Some("ipv6.example.com".to_string())
        );
        assert_eq!(parser.parse_line("0.0.0.0 localhost"), None); // Skip localhost
        assert_eq!(parser.parse_line("not-an-ip example.com"), None);
    }

    #[test]
    fn test_adblock_parser() {
        let parser = BlocklistParser::new(BlocklistFormat::AdBlockPlus);

        assert_eq!(
            parser.parse_line("||ads.example.com^"),
            Some("ads.example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("||example.com^"),
            Some("example.com".to_string())
        );
        assert_eq!(parser.parse_line("@@||example.com^"), None); // Whitelist
        assert_eq!(parser.parse_line("||example.com^$third-party"), None); // Has options
        assert_eq!(
            parser.parse_line("*.doubleclick.net"),
            Some("*.doubleclick.net".to_string())
        );
    }

    #[test]
    fn test_dnsmasq_parser() {
        let parser = BlocklistParser::new(BlocklistFormat::Dnsmasq);

        assert_eq!(
            parser.parse_line("address=/example.com/0.0.0.0"),
            Some("example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("server=/example.com/#"),
            Some("example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("address=/ads.net/::"),
            Some("ads.net".to_string())
        );
        assert_eq!(parser.parse_line("bogus-nxdomain=1.2.3.4"), None);
    }

    #[test]
    fn test_unbound_parser() {
        let parser = BlocklistParser::new(BlocklistFormat::Unbound);

        assert_eq!(
            parser.parse_line("local-zone: \"example.com\" refuse"),
            Some("example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("local-zone: \"ads.example.com\" static"),
            Some("ads.example.com".to_string())
        );
        assert_eq!(
            parser.parse_line("local-data: \"example.com A 0.0.0.0\""),
            None
        );
    }

    #[test]
    fn test_domain_validation() {
        let parser = BlocklistParser::new(BlocklistFormat::DomainList);

        assert!(parser.parse_line("valid-domain.com").is_some());
        assert!(parser.parse_line("sub.domain.example.com").is_some());
        assert!(parser.parse_line("123.456").is_some()); // Numeric is valid
        assert!(parser.parse_line("-invalid.com").is_none()); // Can't start with hyphen
        assert!(parser.parse_line("invalid-.com").is_none()); // Can't end with hyphen
        assert!(
            parser
                .parse_line(&format!("toolong{}.com", "a".repeat(250)))
                .is_none()
        ); // Too long
    }
}
