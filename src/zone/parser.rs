use super::{Result, Zone, ZoneError, ZoneRecord, constants};
use crate::dns::enums::{DNSResourceClass, DNSResourceType};
use std::fs;
use std::path::Path;
use tracing::{debug, trace};

/// RFC 1035 zone file parser
pub struct ZoneParser {
    /// Current origin for relative names
    current_origin: String,
    /// Current default TTL
    current_ttl: Option<u32>,
    /// Current class
    current_class: DNSResourceClass,
    /// Line number for error reporting
    line_number: usize,
}

impl ZoneParser {
    /// Create a new zone parser
    pub fn new() -> Self {
        Self {
            current_origin: String::new(),
            current_ttl: None,
            current_class: DNSResourceClass::IN,
            line_number: 0,
        }
    }

    /// Parse a zone file from path
    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<Zone> {
        let path = path.as_ref();

        // Read file contents
        let contents = fs::read_to_string(path).map_err(|e| ZoneError::IoError(e.to_string()))?;

        // Check file size
        if contents.len() > constants::MAX_ZONE_FILE_SIZE {
            return Err(ZoneError::FileTooLarge);
        }

        // Parse contents
        let mut zone = self.parse(&contents)?;
        zone.file_path = Some(path.to_string_lossy().to_string());

        Ok(zone)
    }

    /// Parse zone file contents
    pub fn parse(&mut self, contents: &str) -> Result<Zone> {
        self.line_number = 0;

        // Create zone (origin will be set by $ORIGIN or first SOA)
        let mut zone = Zone::new(String::new(), constants::DEFAULT_TTL);
        let mut _found_origin = false;
        let mut pending_records = Vec::new();

        // Process lines
        for line in contents.lines() {
            self.line_number += 1;

            // Skip empty lines and comments
            let line = self.strip_comments(line);
            if line.trim().is_empty() {
                continue;
            }

            trace!("Parsing line {}: {}", self.line_number, line);

            // Handle directives
            if line.trim_start().starts_with('$') {
                self.parse_directive(line, &mut zone)?;
                if line.trim_start().starts_with("$ORIGIN") {
                    _found_origin = true;
                }
                continue;
            }

            // Handle line continuations
            let line = if line.ends_with('\\') {
                // TODO: Handle multi-line records
                line.trim_end_matches('\\')
            } else {
                line
            };

            // Parse resource record
            match self.parse_record(line) {
                Ok(record) => {
                    pending_records.push(record);
                }
                Err(e) => {
                    return Err(ZoneError::ParseError(format!(
                        "Line {}: {}",
                        self.line_number, e
                    )));
                }
            }
        }

        // Validate zone has origin
        if zone.origin.is_empty() {
            return Err(ZoneError::ParseError(
                "Zone file missing $ORIGIN directive or SOA record".to_string(),
            ));
        }

        // Apply default TTL if set
        if let Some(ttl) = self.current_ttl {
            zone.default_ttl = ttl;
        }

        // Now add all pending records
        for record in pending_records {
            zone.add_record(record)?;
        }

        // Validate zone
        zone.validate()?;

        debug!(
            "Parsed zone {} with {} records",
            zone.origin,
            zone.stats().total_records
        );

        Ok(zone)
    }

    /// Strip comments from a line
    fn strip_comments<'a>(&self, line: &'a str) -> &'a str {
        if let Some(pos) = line.find(';') {
            &line[..pos]
        } else {
            line
        }
    }

    /// Parse a directive line
    fn parse_directive(&mut self, line: &str, zone: &mut Zone) -> Result<()> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0].to_uppercase().as_str() {
            "$ORIGIN" => {
                if parts.len() < 2 {
                    return Err(ZoneError::ParseError(
                        "$ORIGIN requires domain name".to_string(),
                    ));
                }
                let origin = parts[1].trim_end_matches('.').to_lowercase();
                self.current_origin = origin.clone();
                zone.origin = origin.clone();
                debug!("Set origin to: {}", origin);
            }
            "$TTL" => {
                if parts.len() < 2 {
                    return Err(ZoneError::ParseError("$TTL requires value".to_string()));
                }
                let ttl = self.parse_ttl(parts[1])?;
                self.current_ttl = Some(ttl);
                zone.default_ttl = ttl;
                debug!("Set default TTL to: {}", ttl);
            }
            "$INCLUDE" => {
                // TODO: Handle $INCLUDE directive
                debug!("$INCLUDE not yet supported");
            }
            "$GENERATE" => {
                // TODO: Handle $GENERATE directive
                debug!("$GENERATE not yet supported");
            }
            _ => {
                debug!("Unknown directive: {}", parts[0]);
            }
        }

        Ok(())
    }

    /// Parse a resource record line
    fn parse_record(&self, line: &str) -> Result<ZoneRecord> {
        let mut parts = Vec::new();
        let mut in_quotes = false;
        let mut current_part = String::new();

        // Parse line respecting quoted strings
        for ch in line.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    current_part.push(ch);
                }
                ' ' | '\t' => {
                    if in_quotes {
                        current_part.push(ch);
                    } else if !current_part.is_empty() {
                        parts.push(current_part.clone());
                        current_part.clear();
                    }
                }
                _ => {
                    current_part.push(ch);
                }
            }
        }

        if !current_part.is_empty() {
            parts.push(current_part);
        }

        if parts.is_empty() {
            return Err(ZoneError::ParseError("Empty record line".to_string()));
        }

        // Parse record components
        let mut idx = 0;
        let name;
        let mut ttl = self.current_ttl;
        let mut class = self.current_class;
        let mut rtype = None;

        // First field could be name, TTL, class, or type
        // If it starts with whitespace, name is inherited from previous record
        if line.starts_with(' ') || line.starts_with('\t') {
            // Name is empty (inherit from previous)
            name = String::new();
        } else {
            // First field is the name
            name = parts[idx].clone();
            idx += 1;
        }

        // Parse optional TTL, class, and type
        while idx < parts.len() && rtype.is_none() {
            let field = &parts[idx];

            // Try to parse as TTL
            if let Ok(ttl_value) = self.parse_ttl(field) {
                ttl = Some(ttl_value);
                idx += 1;
                continue;
            }

            // Try to parse as class
            if let Ok(parsed_class) = self.parse_class(field) {
                class = parsed_class;
                idx += 1;
                continue;
            }

            // Try to parse as type
            if let Ok(parsed_type) = self.parse_type(field) {
                rtype = Some(parsed_type);
                idx += 1;
                break;
            }

            return Err(ZoneError::ParseError(format!("Invalid field: {}", field)));
        }

        // Type is required
        let rtype =
            rtype.ok_or_else(|| ZoneError::ParseError("Missing record type".to_string()))?;

        // Rest is RDATA
        if idx >= parts.len() {
            return Err(ZoneError::ParseError("Missing RDATA".to_string()));
        }

        let rdata = parts[idx..].join(" ").trim_matches('"').to_string();

        Ok(ZoneRecord::new(name, ttl, class, rtype, rdata))
    }

    /// Parse TTL value (supports suffixes like 1h, 30m, etc.)
    fn parse_ttl(&self, s: &str) -> Result<u32> {
        let s = s.to_lowercase();

        // Check for time suffixes
        if let Some(num_str) = s.strip_suffix('s') {
            // Seconds
            num_str
                .parse()
                .map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        } else if let Some(num_str) = s.strip_suffix('m') {
            // Minutes
            num_str
                .parse::<u32>()
                .map(|n| n * 60)
                .map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        } else if let Some(num_str) = s.strip_suffix('h') {
            // Hours
            num_str
                .parse::<u32>()
                .map(|n| n * 3600)
                .map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        } else if let Some(num_str) = s.strip_suffix('d') {
            // Days
            num_str
                .parse::<u32>()
                .map(|n| n * 86400)
                .map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        } else if let Some(num_str) = s.strip_suffix('w') {
            // Weeks
            num_str
                .parse::<u32>()
                .map(|n| n * 604800)
                .map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        } else {
            // No suffix, assume seconds
            s.parse().map_err(|_| ZoneError::InvalidTTL(s.to_string()))
        }
    }

    /// Parse class
    fn parse_class(&self, s: &str) -> Result<DNSResourceClass> {
        match s.to_uppercase().as_str() {
            "IN" => Ok(DNSResourceClass::IN),
            "CS" => Ok(DNSResourceClass::CS),
            "CH" => Ok(DNSResourceClass::CH),
            "HS" => Ok(DNSResourceClass::HS),
            _ => Err(ZoneError::ParseError(format!("Unknown class: {}", s))),
        }
    }

    /// Parse record type
    fn parse_type(&self, s: &str) -> Result<DNSResourceType> {
        match s.to_uppercase().as_str() {
            "A" => Ok(DNSResourceType::A),
            "NS" => Ok(DNSResourceType::NS),
            "CNAME" => Ok(DNSResourceType::CNAME),
            "SOA" => Ok(DNSResourceType::SOA),
            "PTR" => Ok(DNSResourceType::PTR),
            "MX" => Ok(DNSResourceType::MX),
            "TXT" => Ok(DNSResourceType::TXT),
            "AAAA" => Ok(DNSResourceType::AAAA),
            "SRV" => Ok(DNSResourceType::SRV),
            "CAA" => Ok(DNSResourceType::CAA),
            // Add more types as needed
            _ => {
                // Try to parse as numeric type
                if let Ok(num) = s.parse::<u16>() {
                    DNSResourceType::from_u16(num)
                        .ok_or_else(|| ZoneError::InvalidRRType(s.to_string()))
                } else {
                    Err(ZoneError::InvalidRRType(s.to_string()))
                }
            }
        }
    }
}

impl Default for ZoneParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ttl() {
        let parser = ZoneParser::new();

        assert_eq!(parser.parse_ttl("300").unwrap(), 300);
        assert_eq!(parser.parse_ttl("5m").unwrap(), 300);
        assert_eq!(parser.parse_ttl("1h").unwrap(), 3600);
        assert_eq!(parser.parse_ttl("1d").unwrap(), 86400);
        assert_eq!(parser.parse_ttl("1w").unwrap(), 604800);
    }

    #[test]
    fn test_simple_zone_file() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400

@       IN  NS  ns1.example.com.
@       IN  NS  ns2.example.com.

@       IN  A   192.0.2.1
www     IN  A   192.0.2.2
mail    IN  A   192.0.2.3

@       IN  MX  10 mail.example.com.
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");
        assert_eq!(zone.default_ttl, 3600);
        assert!(zone.get_soa().is_some());

        let stats = zone.stats();
        assert_eq!(stats.soa_records, 1);
        assert_eq!(stats.ns_records, 2);
        assert_eq!(stats.a_records, 3);
        assert_eq!(stats.mx_records, 1);
    }
}
