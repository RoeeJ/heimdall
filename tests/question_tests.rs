use bitstream_io::{BigEndian, BitReader, BitWriter};
use heimdall::dns::common::PacketComponent;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::question::DNSQuestion;

#[test]
fn test_question_read_write_roundtrip() {
    let original = DNSQuestion {
        labels: vec!["example".to_string(), "com".to_string(), "".to_string()],
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };

    // Write to buffer
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
        original
            .write(&mut writer)
            .expect("Failed to write question");
    }

    // Read back from buffer
    let mut reader = BitReader::<_, BigEndian>::new(&buffer[..]);
    let mut parsed = DNSQuestion::default();
    parsed.read(&mut reader).expect("Failed to read question");

    // Verify all fields match
    assert_eq!(parsed.labels, original.labels);
    assert_eq!(parsed.qtype, original.qtype);
    assert_eq!(parsed.qclass, original.qclass);
}

#[test]
fn test_question_with_subdomain() {
    let question = DNSQuestion {
        labels: vec![
            "mail".to_string(),
            "subdomain".to_string(),
            "example".to_string(),
            "org".to_string(),
            "".to_string(),
        ],
        qtype: DNSResourceType::MX,
        qclass: DNSResourceClass::IN,
    };

    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
        question
            .write(&mut writer)
            .expect("Failed to write question");
    }

    let mut reader = BitReader::<_, BigEndian>::new(&buffer[..]);
    let mut parsed = DNSQuestion::default();
    parsed.read(&mut reader).expect("Failed to read question");

    assert_eq!(parsed.labels.len(), 5);
    assert_eq!(parsed.labels[0], "mail");
    assert_eq!(parsed.labels[1], "subdomain");
    assert_eq!(parsed.labels[2], "example");
    assert_eq!(parsed.labels[3], "org");
    assert_eq!(parsed.labels[4], "");
    assert_eq!(parsed.qtype, DNSResourceType::MX);
}

#[test]
fn test_question_different_types() {
    let test_cases = vec![
        (DNSResourceType::A, DNSResourceClass::IN),
        (DNSResourceType::AAAA, DNSResourceClass::IN),
        (DNSResourceType::CNAME, DNSResourceClass::IN),
        (DNSResourceType::MX, DNSResourceClass::CS),
        (DNSResourceType::TXT, DNSResourceClass::CH),
        (DNSResourceType::SOA, DNSResourceClass::HS),
    ];

    for (qtype, qclass) in test_cases {
        let question = DNSQuestion {
            labels: vec!["test".to_string(), "".to_string()],
            qtype,
            qclass,
        };

        let mut buffer = Vec::new();
        {
            let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
            question
                .write(&mut writer)
                .expect("Failed to write question");
        }

        let mut reader = BitReader::<_, BigEndian>::new(&buffer[..]);
        let mut parsed = DNSQuestion::default();
        parsed.read(&mut reader).expect("Failed to read question");

        assert_eq!(parsed.qtype, qtype);
        assert_eq!(parsed.qclass, qclass);
    }
}

#[test]
fn test_question_empty_label() {
    // Test root domain query
    let question = DNSQuestion {
        labels: vec!["".to_string()],
        qtype: DNSResourceType::NS,
        qclass: DNSResourceClass::IN,
    };

    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
        question
            .write(&mut writer)
            .expect("Failed to write question");
    }

    // Should just be: 0x00 (root label) + type + class
    assert_eq!(buffer.len(), 5); // 1 + 2 + 2
    assert_eq!(buffer[0], 0x00); // Root label
}
