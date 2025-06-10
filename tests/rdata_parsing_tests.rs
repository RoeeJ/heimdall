use heimdall::dns::{
    DNSPacket,
    enums::{DNSResourceClass, DNSResourceType},
    header::DNSHeader,
    question::DNSQuestion,
    resource::DNSResource,
};

fn create_test_packet_with_resource(resource: DNSResource) -> Vec<u8> {
    let packet = DNSPacket {
        header: DNSHeader {
            id: 1234,
            qr: true,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: true,
            z: 0,
            rcode: 0,
            qdcount: 1,
            ancount: 1,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![DNSQuestion {
            labels: resource.labels.clone(),
            qtype: resource.rtype,
            qclass: resource.rclass,
        }],
        answers: vec![resource],
        authorities: vec![],
        resources: vec![],
        edns: None,
    };

    packet.serialize().unwrap()
}

#[test]
fn test_soa_record_parsing() {
    // Build SOA RDATA: ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
    let mut rdata = Vec::new();

    // MNAME: ns1.example.com
    rdata.extend(&[3]); // length of "ns1"
    rdata.extend(b"ns1");
    rdata.extend(&[7]); // length of "example"
    rdata.extend(b"example");
    rdata.extend(&[3]); // length of "com"
    rdata.extend(b"com");
    rdata.extend(&[0]); // null terminator

    // RNAME: admin.example.com
    rdata.extend(&[5]); // length of "admin"
    rdata.extend(b"admin");
    rdata.extend(&[7]); // length of "example"
    rdata.extend(b"example");
    rdata.extend(&[3]); // length of "com"
    rdata.extend(b"com");
    rdata.extend(&[0]); // null terminator

    // Serial: 2024010101
    rdata.extend(&2024010101u32.to_be_bytes());
    // Refresh: 3600
    rdata.extend(&3600u32.to_be_bytes());
    // Retry: 900
    rdata.extend(&900u32.to_be_bytes());
    // Expire: 604800
    rdata.extend(&604800u32.to_be_bytes());
    // Minimum: 86400
    rdata.extend(&86400u32.to_be_bytes());

    // Create a mock SOA record
    let soa_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::SOA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(soa_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed SOA record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_soa = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_soa.parsed_rdata.is_some());
    let parsed = parsed_soa.parsed_rdata.as_ref().unwrap();
    assert_eq!(
        parsed,
        "ns1.example.com admin.example.com 2024010101 3600 900 604800 86400"
    );

    // Test helper methods
    let fields = parsed_soa.get_soa_fields().unwrap();
    assert_eq!(fields.0, "ns1.example.com");
    assert_eq!(fields.1, "admin.example.com");
    assert_eq!(fields.2, 2024010101);
    assert_eq!(fields.3, 3600);
    assert_eq!(fields.4, 900);
    assert_eq!(fields.5, 604800);
    assert_eq!(fields.6, 86400);

    // Test minimum TTL extraction
    assert_eq!(parsed_soa.get_soa_minimum(), Some(86400));
}

#[test]
fn test_srv_record_parsing() {
    // Build SRV RDATA: Priority=10, Weight=60, Port=80, Target=www.example.com
    let mut rdata = Vec::new();

    // Priority: 10
    rdata.extend(&10u16.to_be_bytes());
    // Weight: 60
    rdata.extend(&60u16.to_be_bytes());
    // Port: 80
    rdata.extend(&80u16.to_be_bytes());

    // Target: www.example.com
    rdata.extend(&[3]); // length of "www"
    rdata.extend(b"www");
    rdata.extend(&[7]); // length of "example"
    rdata.extend(b"example");
    rdata.extend(&[3]); // length of "com"
    rdata.extend(b"com");
    rdata.extend(&[0]); // null terminator

    // Create a mock SRV record
    let srv_record = DNSResource {
        labels: vec![
            "_http".to_string(),
            "_tcp".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::SRV,
        rclass: DNSResourceClass::IN,
        ttl: 300,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(srv_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed SRV record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_srv = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_srv.parsed_rdata.is_some());
    let parsed = parsed_srv.parsed_rdata.as_ref().unwrap();
    assert_eq!(parsed, "10 60 80 www.example.com");

    // Test helper method
    let fields = parsed_srv.get_srv_fields().unwrap();
    assert_eq!(fields.0, 10); // priority
    assert_eq!(fields.1, 60); // weight
    assert_eq!(fields.2, 80); // port
    assert_eq!(fields.3, "www.example.com");
}

#[test]
fn test_caa_record_parsing() {
    // Build CAA RDATA: Flags=0, Tag="issue", Value="letsencrypt.org"
    let mut rdata = Vec::new();

    // Flags: 0
    rdata.push(0);

    // Tag length and tag
    let tag = b"issue";
    rdata.push(tag.len() as u8);
    rdata.extend(tag);

    // Value
    rdata.extend(b"letsencrypt.org");

    // Create a mock CAA record
    let caa_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::CAA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(caa_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed CAA record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_caa = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_caa.parsed_rdata.is_some());
    let parsed = parsed_caa.parsed_rdata.as_ref().unwrap();
    assert_eq!(parsed, "0 issue letsencrypt.org");

    // Test helper method
    let fields = parsed_caa.get_caa_fields().unwrap();
    assert_eq!(fields.0, 0); // flags
    assert_eq!(fields.1, "issue"); // tag
    assert_eq!(fields.2, "letsencrypt.org"); // value
}

#[test]
fn test_caa_record_with_critical_flag() {
    // Build CAA RDATA: Flags=128 (critical), Tag="issuewild", Value=";"
    let mut rdata = Vec::new();

    // Flags: 128 (critical flag set)
    rdata.push(128);

    // Tag length and tag
    let tag = b"issuewild";
    rdata.push(tag.len() as u8);
    rdata.extend(tag);

    // Value (semicolon means no wildcard certificates allowed)
    rdata.extend(b";");

    // Create a mock CAA record with critical flag set
    let caa_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::CAA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: rdata.len() as u16,
        rdata,
        parsed_rdata: None,
        raw_class: None,
    };

    // Create a packet with this record and parse it back
    let packet_bytes = create_test_packet_with_resource(caa_record);
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();

    // Get the parsed CAA record
    assert_eq!(parsed_packet.answers.len(), 1);
    let parsed_caa = &parsed_packet.answers[0];

    // Check parsed data
    assert!(parsed_caa.parsed_rdata.is_some());
    let parsed = parsed_caa.parsed_rdata.as_ref().unwrap();
    assert_eq!(parsed, "128 issuewild ;");

    // Test helper method
    let fields = parsed_caa.get_caa_fields().unwrap();
    assert_eq!(fields.0, 128); // flags (critical)
    assert_eq!(fields.1, "issuewild"); // tag
    assert_eq!(fields.2, ";"); // value
}

#[test]
fn test_rebuild_rdata_for_soa() {
    let soa_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::SOA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some(
            "ns1.example.com admin.example.com 2024010101 3600 900 604800 86400".to_string(),
        ),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(soa_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_soa = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_soa.parsed_rdata.as_ref().unwrap(),
        "ns1.example.com admin.example.com 2024010101 3600 900 604800 86400"
    );
}

#[test]
fn test_rebuild_rdata_for_srv() {
    let srv_record = DNSResource {
        labels: vec![
            "_http".to_string(),
            "_tcp".to_string(),
            "example".to_string(),
            "com".to_string(),
        ],
        rtype: DNSResourceType::SRV,
        rclass: DNSResourceClass::IN,
        ttl: 300,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some("10 60 80 www.example.com".to_string()),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(srv_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_srv = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_srv.parsed_rdata.as_ref().unwrap(),
        "10 60 80 www.example.com"
    );
}

#[test]
fn test_rebuild_rdata_for_caa() {
    let caa_record = DNSResource {
        labels: vec!["example".to_string(), "com".to_string()],
        rtype: DNSResourceType::CAA,
        rclass: DNSResourceClass::IN,
        ttl: 3600,
        rdlength: 0,
        rdata: vec![],
        parsed_rdata: Some("0 issue letsencrypt.org".to_string()),
        raw_class: None,
    };

    // Create a packet with this parsed record
    let packet_bytes = create_test_packet_with_resource(caa_record);

    // Parse it back to ensure rebuild_rdata works correctly
    let parsed_packet = DNSPacket::parse(&packet_bytes).unwrap();
    let parsed_caa = &parsed_packet.answers[0];

    // The parsing process should have called rebuild_rdata internally
    assert_eq!(
        parsed_caa.parsed_rdata.as_ref().unwrap(),
        "0 issue letsencrypt.org"
    );
}
