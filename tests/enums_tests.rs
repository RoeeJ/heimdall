use heimdall::dns::enums::{DNSResourceType, DNSResourceClass};

#[test]
fn test_resource_type_conversion() {
    // Test From<u16> for DNSResourceType
    assert_eq!(DNSResourceType::from(1), DNSResourceType::A);
    assert_eq!(DNSResourceType::from(2), DNSResourceType::NS);
    assert_eq!(DNSResourceType::from(5), DNSResourceType::CNAME);
    assert_eq!(DNSResourceType::from(6), DNSResourceType::SOA);
    assert_eq!(DNSResourceType::from(15), DNSResourceType::MX);
    assert_eq!(DNSResourceType::from(16), DNSResourceType::TXT);
    assert_eq!(DNSResourceType::from(28), DNSResourceType::AAAA);
    assert_eq!(DNSResourceType::from(999), DNSResourceType::Unknown);
}

#[test]
fn test_resource_type_into_u16() {
    // Test Into<u16> for DNSResourceType
    assert_eq!(u16::from(DNSResourceType::A), 1);
    assert_eq!(u16::from(DNSResourceType::NS), 2);
    assert_eq!(u16::from(DNSResourceType::CNAME), 5);
    assert_eq!(u16::from(DNSResourceType::SOA), 6);
    assert_eq!(u16::from(DNSResourceType::MX), 15);
    assert_eq!(u16::from(DNSResourceType::TXT), 16);
    assert_eq!(u16::from(DNSResourceType::AAAA), 28);
    assert_eq!(u16::from(DNSResourceType::Unknown), 0);
}

#[test]
fn test_resource_class_conversion() {
    // Test From<u16> for DNSResourceClass
    assert_eq!(DNSResourceClass::from(1), DNSResourceClass::IN);
    assert_eq!(DNSResourceClass::from(2), DNSResourceClass::CS);
    assert_eq!(DNSResourceClass::from(3), DNSResourceClass::CH);
    assert_eq!(DNSResourceClass::from(4), DNSResourceClass::HS);
    assert_eq!(DNSResourceClass::from(999), DNSResourceClass::Unknown);
}

#[test]
fn test_resource_class_into_u16() {
    // Test Into<u16> for DNSResourceClass
    assert_eq!(u16::from(DNSResourceClass::IN), 1);
    assert_eq!(u16::from(DNSResourceClass::CS), 2);
    assert_eq!(u16::from(DNSResourceClass::CH), 3);
    assert_eq!(u16::from(DNSResourceClass::HS), 4);
    assert_eq!(u16::from(DNSResourceClass::Unknown), 0);
}

#[test]
fn test_resource_type_default() {
    assert_eq!(DNSResourceType::default(), DNSResourceType::Unknown);
}

#[test]
fn test_resource_class_default() {
    assert_eq!(DNSResourceClass::default(), DNSResourceClass::Unknown);
}

#[test]
fn test_resource_type_roundtrip() {
    // Test that converting to u16 and back preserves the value
    let types = vec![
        DNSResourceType::A,
        DNSResourceType::NS,
        DNSResourceType::CNAME,
        DNSResourceType::SOA,
        DNSResourceType::MX,
        DNSResourceType::TXT,
        DNSResourceType::AAAA,
    ];
    
    for rt in types {
        let value: u16 = rt.into();
        let converted: DNSResourceType = value.into();
        assert_eq!(rt, converted);
    }
}

#[test]
fn test_resource_class_roundtrip() {
    // Test that converting to u16 and back preserves the value
    let classes = vec![
        DNSResourceClass::IN,
        DNSResourceClass::CS,
        DNSResourceClass::CH,
        DNSResourceClass::HS,
    ];
    
    for rc in classes {
        let value: u16 = rc.into();
        let converted: DNSResourceClass = value.into();
        assert_eq!(rc, converted);
    }
}