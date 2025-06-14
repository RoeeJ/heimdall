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

        // Buffer for multi-line records
        let mut multi_line_buffer = String::new();
        let mut in_parentheses = false;
        let mut paren_start_line = 0;

        // Process lines
        for line in contents.lines() {
            self.line_number += 1;

            // Skip empty lines and comments (unless in multi-line)
            let line = self.strip_comments(line);
            if line.trim().is_empty() && !in_parentheses {
                continue;
            }

            trace!("Parsing line {}: {}", self.line_number, line);

            // Handle multi-line records with parentheses
            if in_parentheses {
                multi_line_buffer.push(' ');
                multi_line_buffer.push_str(line.trim());

                // Check if we're closing the parentheses
                if line.contains(')') && !line.contains('(') {
                    in_parentheses = false;
                    let complete_line = multi_line_buffer.clone();
                    multi_line_buffer.clear();

                    // Process the complete multi-line record
                    match self.parse_record(&complete_line) {
                        Ok(record) => {
                            pending_records.push(record);
                        }
                        Err(e) => {
                            return Err(ZoneError::ParseError(format!(
                                "Lines {}-{}: {}",
                                paren_start_line, self.line_number, e
                            )));
                        }
                    }
                }
                continue;
            }

            // Check if we're starting a multi-line record
            if line.contains('(') && !line.contains(')') {
                in_parentheses = true;
                paren_start_line = self.line_number;
                multi_line_buffer = line.to_string();
                continue;
            }

            // Handle directives
            if line.trim_start().starts_with('$') {
                self.parse_directive(line, &mut zone, &mut pending_records)?;
                if line.trim_start().starts_with("$ORIGIN") {
                    _found_origin = true;
                }
                continue;
            }

            // Parse single-line resource record
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

        // Check for unclosed parentheses
        if in_parentheses {
            return Err(ZoneError::ParseError(format!(
                "Unclosed parentheses starting at line {}",
                paren_start_line
            )));
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
    fn parse_directive(
        &mut self,
        line: &str,
        zone: &mut Zone,
        pending_records: &mut Vec<ZoneRecord>,
    ) -> Result<()> {
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
                if parts.len() < 2 {
                    return Err(ZoneError::ParseError(
                        "$INCLUDE requires file path".to_string(),
                    ));
                }

                let include_path = parts[1];
                let domain = if parts.len() > 2 {
                    Some(parts[2].trim_end_matches('.').to_lowercase())
                } else {
                    None
                };

                debug!("Processing $INCLUDE {} {:?}", include_path, domain);

                // Save current state
                let saved_origin = self.current_origin.clone();
                let saved_line = self.line_number;

                // Set origin for included file if specified
                if let Some(ref domain) = domain {
                    self.current_origin = domain.clone();
                }

                // Read and parse the included file's content
                let include_contents = fs::read_to_string(include_path).map_err(|e| {
                    ZoneError::ParseError(format!(
                        "Failed to read include file {}: {}",
                        include_path, e
                    ))
                })?;

                // Parse the included content into records
                // We need to parse line by line to avoid zone validation issues
                let included_lines = include_contents.lines();
                let saved_line_number = self.line_number;
                self.line_number = 0;

                for line in included_lines {
                    self.line_number += 1;

                    // Skip empty lines and comments
                    let line = self.strip_comments(line);
                    if line.trim().is_empty() {
                        continue;
                    }

                    // Skip directives in included files (except nested $INCLUDE)
                    if line.trim_start().starts_with('$') {
                        if line.trim_start().starts_with("$INCLUDE") {
                            // Handle nested includes
                            self.parse_directive(line, zone, pending_records)?;
                        }
                        // Skip other directives like $ORIGIN, $TTL
                        continue;
                    }

                    // Parse the record
                    match self.parse_record(line) {
                        Ok(mut record) => {
                            // Adjust record name if a different origin was specified
                            if domain.is_some() {
                                if record.name == "@" || record.name.is_empty() {
                                    // @ or empty name becomes the include origin
                                    record.name = self.current_origin.clone();
                                } else if !record.name.contains('.') {
                                    // Relative names are prefixed with the include origin
                                    record.name =
                                        format!("{}.{}", record.name, self.current_origin);
                                }
                                // Fully qualified names remain unchanged
                            }
                            pending_records.push(record);
                        }
                        Err(e) => {
                            return Err(ZoneError::ParseError(format!(
                                "Error in included file {} line {}: {}",
                                include_path, self.line_number, e
                            )));
                        }
                    }
                }

                self.line_number = saved_line_number;
                debug!("Successfully processed $INCLUDE {}", include_path);

                // Restore state
                self.current_origin = saved_origin;
                self.line_number = saved_line;
            }
            "$GENERATE" => {
                if parts.len() < 4 {
                    return Err(ZoneError::ParseError(
                        "$GENERATE requires range, lhs, type, and rhs".to_string(),
                    ));
                }

                let range_str = parts[1];
                let lhs = parts[2];
                let rtype_str = parts[3];
                let rhs = parts[4..].join(" ");

                // Parse range (e.g., "1-10", "20-30/2" for step)
                let (start, stop, step) = self.parse_generate_range(range_str)?;

                // Parse record type
                let rtype = self.parse_type(rtype_str)?;

                debug!(
                    "Processing $GENERATE {}-{}/{} {} {} {}",
                    start, stop, step, lhs, rtype_str, rhs
                );

                // Generate records
                let mut i = start;
                while i <= stop {
                    // First expand format specifiers, then replace simple $
                    let name = self.expand_generate_format(lhs, i)?;
                    let rdata = self.expand_generate_format(&rhs, i)?;

                    // Replace remaining $ with the current value
                    let name = name.replace('$', &i.to_string());
                    let rdata = rdata.replace('$', &i.to_string());

                    // Create the record
                    let record =
                        ZoneRecord::new(name, self.current_ttl, self.current_class, rtype, rdata);

                    pending_records.push(record);
                    i += step;
                }

                debug!(
                    "Generated {} records from $GENERATE",
                    (stop - start) / step + 1
                );
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
        let mut in_parens = false;
        let mut current_part = String::new();

        // Parse line respecting quoted strings and parentheses
        for ch in line.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    current_part.push(ch);
                }
                '(' => {
                    in_parens = true;
                    // Don't include the parenthesis in the part
                }
                ')' => {
                    in_parens = false;
                    // Don't include the parenthesis in the part
                }
                ' ' | '\t' => {
                    if in_quotes || in_parens {
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

    /// Parse $GENERATE range specification
    fn parse_generate_range(&self, range_str: &str) -> Result<(u32, u32, u32)> {
        // Format: start-stop[/step]
        let parts: Vec<&str> = range_str.split('/').collect();

        let (start, stop) = if let Some(dash_pos) = parts[0].find('-') {
            let start_str = &parts[0][..dash_pos];
            let stop_str = &parts[0][dash_pos + 1..];

            let start = start_str.parse::<u32>().map_err(|_| {
                ZoneError::ParseError(format!("Invalid $GENERATE start: {}", start_str))
            })?;
            let stop = stop_str.parse::<u32>().map_err(|_| {
                ZoneError::ParseError(format!("Invalid $GENERATE stop: {}", stop_str))
            })?;

            (start, stop)
        } else {
            return Err(ZoneError::ParseError(
                "$GENERATE range must contain '-'".to_string(),
            ));
        };

        let step = if parts.len() > 1 {
            parts[1].parse::<u32>().map_err(|_| {
                ZoneError::ParseError(format!("Invalid $GENERATE step: {}", parts[1]))
            })?
        } else {
            1
        };

        if start > stop {
            return Err(ZoneError::ParseError(
                "$GENERATE start must be <= stop".to_string(),
            ));
        }

        if step == 0 {
            return Err(ZoneError::ParseError(
                "$GENERATE step must be > 0".to_string(),
            ));
        }

        Ok((start, stop, step))
    }

    /// Expand $GENERATE format specifiers
    fn expand_generate_format(&self, template: &str, value: u32) -> Result<String> {
        let mut result = String::new();
        let mut chars = template.chars();

        while let Some(ch) = chars.next() {
            if ch == '$' && chars.as_str().starts_with('{') {
                // Skip the '{'
                chars.next();

                // Find the closing '}'
                let mut spec = String::new();
                let mut found_close = false;

                for ch in chars.by_ref() {
                    if ch == '}' {
                        found_close = true;
                        break;
                    }
                    spec.push(ch);
                }

                if !found_close {
                    return Err(ZoneError::ParseError(
                        "Unclosed ${} in $GENERATE".to_string(),
                    ));
                }

                // Parse the format spec: offset,width,base
                let parts: Vec<&str> = spec.split(',').collect();
                if parts.len() != 3 {
                    return Err(ZoneError::ParseError(
                        "Invalid $GENERATE format, expected ${offset,width,base}".to_string(),
                    ));
                }

                let offset = parts[0]
                    .parse::<u32>()
                    .map_err(|_| ZoneError::ParseError(format!("Invalid offset: {}", parts[0])))?;
                let width = parts[1]
                    .parse::<usize>()
                    .map_err(|_| ZoneError::ParseError(format!("Invalid width: {}", parts[1])))?;
                let base = parts[2];

                let adjusted_value = value + offset;
                let formatted = match base {
                    "d" => format!("{:0width$}", adjusted_value, width = width),
                    "o" => format!("{:0width$o}", adjusted_value, width = width),
                    "x" => format!("{:0width$x}", adjusted_value, width = width),
                    "X" => format!("{:0width$X}", adjusted_value, width = width),
                    _ => {
                        return Err(ZoneError::ParseError(format!(
                            "Invalid base '{}', expected d, o, x, or X",
                            base
                        )));
                    }
                };

                result.push_str(&formatted);
            } else {
                result.push(ch);
            }
        }

        Ok(result)
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

    #[test]
    fn test_multi_line_soa_record() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA (
    ns1.example.com.    ; Primary nameserver
    admin.example.com.  ; Admin email
    2024010101          ; Serial
    3600                ; Refresh
    900                 ; Retry
    604800              ; Expire
    86400               ; Minimum TTL
)

@       IN  NS  ns1.example.com.
@       IN  A   192.0.2.1
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");
        assert!(zone.get_soa().is_some());

        let stats = zone.stats();
        assert_eq!(stats.soa_records, 1);
        assert_eq!(stats.ns_records, 1);
        assert_eq!(stats.a_records, 1);
    }

    #[test]
    fn test_multi_line_txt_record() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

@   IN  TXT (
    "v=spf1 "
    "ip4:192.0.2.0/24 "
    "ip4:203.0.113.0/24 "
    "include:_spf.example.com "
    "-all"
)

long IN TXT ( "This is a very long TXT record that spans "
              "multiple lines in the zone file but will be "
              "concatenated into a single string" )
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");

        let stats = zone.stats();
        assert_eq!(stats.txt_records, 2);
    }

    #[test]
    fn test_unclosed_parentheses_error() {
        let zone_content = r#"
$ORIGIN example.com.

@   IN  SOA (
    ns1.example.com.
    admin.example.com.
    2024010101
    ; Missing closing parenthesis
        "#;

        let mut parser = ZoneParser::new();
        let result = parser.parse(zone_content);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Unclosed parentheses"));
    }

    #[test]
    fn test_include_directive() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();

        // Create the included file
        let include_path = temp_dir.path().join("included.zone");
        let included_content = r#"
; This is the included zone file
www     IN  A   192.0.2.100
ftp     IN  A   192.0.2.101
        "#;
        fs::write(&include_path, included_content).unwrap();

        // Create the main zone file content
        let zone_content = format!(
            r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

@       IN  A   192.0.2.1
mail    IN  A   192.0.2.2

$INCLUDE {}
        "#,
            include_path.display()
        );

        let mut parser = ZoneParser::new();
        let zone = parser.parse(&zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");

        let stats = zone.stats();
        assert_eq!(stats.a_records, 4); // 2 from main + 2 from include

        // Check that included records exist
        let records: Vec<_> = zone.records().collect();
        assert!(
            records
                .iter()
                .any(|r| r.name == "www" && r.rdata == "192.0.2.100")
        );
        assert!(
            records
                .iter()
                .any(|r| r.name == "ftp" && r.rdata == "192.0.2.101")
        );
    }

    #[test]
    fn test_include_with_origin() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();

        // Create the included file (for subdomain)
        let include_path = temp_dir.path().join("subdomain.zone");
        let included_content = r#"
@       IN  A   192.0.2.200
www     IN  A   192.0.2.201
        "#;
        fs::write(&include_path, included_content).unwrap();

        // Create the main zone file content
        let zone_content = format!(
            r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

@       IN  A   192.0.2.1

; Include subdomain records
$INCLUDE {} sub.example.com.
        "#,
            include_path.display()
        );

        let mut parser = ZoneParser::new();
        let zone = parser.parse(&zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");

        let stats = zone.stats();
        assert_eq!(stats.a_records, 3); // 1 from main + 2 from include

        // Check that included records have proper names
        let records: Vec<_> = zone.records().collect();
        // The @ record from the included file becomes "sub.example.com" in the main zone
        assert!(records.iter().any(|r| r.name == "sub.example.com" || r.name == "sub" && r.rdata == "192.0.2.200"),
                "Did not find sub.example.com A record");
        assert!(
            records
                .iter()
                .any(|r| r.name == "www.sub.example.com" && r.rdata == "192.0.2.201"),
            "Did not find www.sub.example.com A record"
        );
    }

    #[test]
    fn test_include_file_not_found() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

$INCLUDE /nonexistent/file.zone
        "#;

        let mut parser = ZoneParser::new();
        let result = parser.parse(zone_content);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(
            error_str.contains("Failed to read include file")
                || error_str.contains("Failed to include"),
            "Unexpected error: {}",
            error_str
        );
    }

    #[test]
    fn test_generate_simple() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Generate host1 through host5
$GENERATE 1-5 host$ A 192.0.2.$
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        assert_eq!(zone.origin, "example.com");

        let stats = zone.stats();
        assert_eq!(stats.a_records, 5); // Generated 5 A records

        // Check that generated records exist
        let records: Vec<_> = zone.records().collect();
        for i in 1..=5 {
            assert!(
                records
                    .iter()
                    .any(|r| r.name == format!("host{}", i) && r.rdata == format!("192.0.2.{}", i)),
                "Missing host{} record",
                i
            );
        }
    }

    #[test]
    fn test_generate_with_step() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Generate only even numbered hosts
$GENERATE 2-10/2 host$ A 192.0.2.$
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        let stats = zone.stats();
        assert_eq!(stats.a_records, 5); // Should generate 2,4,6,8,10

        let records: Vec<_> = zone.records().collect();
        for i in (2..=10).step_by(2) {
            assert!(
                records
                    .iter()
                    .any(|r| r.name == format!("host{}", i) && r.rdata == format!("192.0.2.{}", i)),
                "Missing host{} record",
                i
            );
        }
    }

    #[test]
    fn test_generate_with_format() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Generate with zero-padded numbers
$GENERATE 1-3 host${0,3,d} A 192.0.2.${0,1,d}
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        let records: Vec<_> = zone.records().collect();

        // Should generate host001, host002, host003
        assert!(
            records
                .iter()
                .any(|r| r.name == "host001" && r.rdata == "192.0.2.1")
        );
        assert!(
            records
                .iter()
                .any(|r| r.name == "host002" && r.rdata == "192.0.2.2")
        );
        assert!(
            records
                .iter()
                .any(|r| r.name == "host003" && r.rdata == "192.0.2.3")
        );
    }

    #[test]
    fn test_generate_ptr_records() {
        let zone_content = r#"
$ORIGIN 2.0.192.in-addr.arpa.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Generate reverse PTR records
$GENERATE 1-5 $ PTR host$.example.com.
        "#;

        let mut parser = ZoneParser::new();
        let zone = parser.parse(zone_content).unwrap();

        let stats = zone.stats();
        assert_eq!(stats.other_records, 5); // PTR records counted as "other"

        let records: Vec<_> = zone.records().collect();
        for i in 1..=5 {
            assert!(
                records.iter().any(|r| r.name == i.to_string()
                    && r.rtype == DNSResourceType::PTR
                    && r.rdata == format!("host{}.example.com.", i)),
                "Missing PTR record for {}",
                i
            );
        }
    }

    #[test]
    fn test_generate_range_validation() {
        let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

; Invalid range - start > stop
$GENERATE 10-5 host$ A 192.0.2.$
        "#;

        let mut parser = ZoneParser::new();
        let result = parser.parse(zone_content);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("start must be <= stop"));
    }
}
