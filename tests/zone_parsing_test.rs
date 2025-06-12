use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::zone::{QueryResult, Zone, ZoneParser, ZoneRecord, ZoneStore};
use std::fs;

#[test]
fn test_simple_zone_parsing() {
    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400

@   IN  NS  ns1.example.com.
@   IN  NS  ns2.example.com.

@       IN  A   192.0.2.1
www     IN  A   192.0.2.2
mail    IN  A   192.0.2.3

@       IN  MX  10 mail.example.com.
"#;

    let mut parser = ZoneParser::new();
    let zone = parser.parse(zone_content).unwrap();

    assert_eq!(zone.origin, "example.com");
    assert_eq!(zone.default_ttl, 3600);

    // Check SOA record
    assert!(zone.get_soa().is_some());

    // Check record counts
    let stats = zone.stats();
    assert_eq!(stats.soa_records, 1);
    assert_eq!(stats.ns_records, 2);
    assert_eq!(stats.a_records, 3);
    assert_eq!(stats.mx_records, 1);
}

#[test]
fn test_zone_store_queries() {
    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400

@   IN  NS  ns1.example.com.
@   IN  NS  ns2.example.com.

@       IN  A   192.0.2.1
www     IN  A   192.0.2.2
mail    IN  A   192.0.2.3
ftp     IN  CNAME www.example.com.

@       IN  MX  10 mail.example.com.

; Delegation
sub     IN  NS  ns1.sub.example.com.
sub     IN  NS  ns2.sub.example.com.
"#;

    let mut parser = ZoneParser::new();
    let zone = parser.parse(zone_content).unwrap();

    let store = ZoneStore::new();
    store.add_zone(zone).unwrap();

    // Test successful A record query
    match store.query("www.example.com", DNSResourceType::A) {
        QueryResult::Success { records, .. } => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].parsed_rdata.as_ref().unwrap(), "192.0.2.2");
        }
        _ => panic!("Expected successful A record query"),
    }

    // Test NXDOMAIN
    match store.query("nonexistent.example.com", DNSResourceType::A) {
        QueryResult::NXDomain { .. } => {}
        _ => panic!("Expected NXDOMAIN"),
    }

    // Test NoData (existing name but no AAAA records)
    match store.query("www.example.com", DNSResourceType::AAAA) {
        QueryResult::NoData { .. } => {}
        _ => panic!("Expected NoData"),
    }

    // Test CNAME query
    match store.query("ftp.example.com", DNSResourceType::CNAME) {
        QueryResult::Success { records, .. } => {
            assert_eq!(records.len(), 1);
            assert_eq!(
                records[0].parsed_rdata.as_ref().unwrap(),
                "www.example.com."
            );
        }
        _ => panic!("Expected successful CNAME query"),
    }

    // Test delegation
    match store.query("www.sub.example.com", DNSResourceType::A) {
        QueryResult::Delegation { ns_records, .. } => {
            assert_eq!(ns_records.len(), 2);
        }
        _ => panic!("Expected delegation"),
    }

    // Test not authoritative
    match store.query("example.org", DNSResourceType::A) {
        QueryResult::NotAuthoritative => {}
        _ => panic!("Expected NotAuthoritative"),
    }
}

#[test]
fn test_zone_ttl_parsing() {
    let zone_content = r#"
$ORIGIN example.com.
$TTL 1h

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 86400 7200 604800 86400

@   IN  NS  ns1.example.com.
www     300 IN  A   192.0.2.1       ; explicit TTL in seconds
ftp     300 IN  A   192.0.2.2       ; TTL in minutes
"#;

    let mut parser = ZoneParser::new();
    let zone = parser.parse(zone_content).unwrap();

    assert_eq!(zone.default_ttl, 3600); // 1 hour

    // Check that records have appropriate TTLs
    let www_records = zone.get_records("www.example.com", Some(DNSResourceType::A));
    assert_eq!(www_records.len(), 1);
    assert_eq!(www_records[0].ttl, Some(300));

    let ftp_records = zone.get_records("ftp.example.com", Some(DNSResourceType::A));
    assert_eq!(ftp_records.len(), 1);
    assert_eq!(ftp_records[0].ttl, Some(300)); // 5 minutes = 300 seconds
}

#[test]
fn test_zone_file_loading() {
    // Create a temporary zone file
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test.example.com.zone");

    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
@   IN  A   192.0.2.1
"#;

    fs::write(&zone_file_path, zone_content).unwrap();

    let store = ZoneStore::new();
    let origin = store.load_zone_file(&zone_file_path).unwrap();

    assert_eq!(origin, "example.com");
    assert_eq!(store.zone_count(), 1);

    // Clean up
    fs::remove_file(zone_file_path).ok();
}

#[test]
fn test_txt_record_parsing() {
    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

@   IN  TXT "v=spf1 include:_spf.example.com ~all"
_dmarc  IN  TXT "v=DMARC1; p=reject; rua=mailto:dmarc@example.com"
"#;

    let mut parser = ZoneParser::new();
    let zone = parser.parse(zone_content).unwrap();

    let txt_records = zone.get_records("example.com", Some(DNSResourceType::TXT));
    assert_eq!(txt_records.len(), 1);

    let dmarc_records = zone.get_records("_dmarc.example.com", Some(DNSResourceType::TXT));
    assert_eq!(dmarc_records.len(), 1);
}

#[test]
fn test_srv_record_parsing() {
    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.

_http._tcp  IN  SRV 10 5 80 www.example.com.
_https._tcp IN  SRV 10 5 443 www.example.com.
"#;

    let mut parser = ZoneParser::new();
    let zone = parser.parse(zone_content).unwrap();

    let srv_records = zone.get_records("_http._tcp.example.com", Some(DNSResourceType::SRV));
    assert_eq!(srv_records.len(), 1);
}

#[test]
fn test_zone_validation() {
    // Zone without SOA should fail validation
    let mut zone = Zone::new("example.com".to_string(), 3600);
    assert!(zone.validate().is_err());

    // Add SOA record
    let soa = ZoneRecord::new(
        "@".to_string(),
        Some(3600),
        DNSResourceClass::IN,
        DNSResourceType::SOA,
        "ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400".to_string(),
    );
    zone.add_record(soa).unwrap();

    // Still missing NS record
    assert!(zone.validate().is_err());

    // Add NS record
    let ns = ZoneRecord::new(
        "@".to_string(),
        Some(3600),
        DNSResourceClass::IN,
        DNSResourceType::NS,
        "ns1.example.com.".to_string(),
    );
    zone.add_record(ns).unwrap();

    // Now should validate
    assert!(zone.validate().is_ok());
}
