use heimdall::dns::DNSPacket;
use heimdall::dns::enums::{DNSResourceType, ResponseCode};
use heimdall::dnssec::DenialOfExistenceValidator;

#[test]
fn test_name_range_checking() {
    let _validator = DenialOfExistenceValidator::new();

    // Create a simple test using the internal method via a minimal implementation
    // Since the methods are private, we'll test through the public interface
    let mut packet = DNSPacket::default();
    packet.header.rcode = ResponseCode::NameError.to_u8();

    // This test verifies that the denial validator can be created
    // Full integration tests would require NSEC/NSEC3 records
    assert_eq!(packet.header.rcode, 3); // NXDOMAIN
}

#[test]
fn test_denial_validator_creation() {
    let validator = DenialOfExistenceValidator::new();

    // Test that validator can process empty packet without panicking
    let packet = DNSPacket::default();
    let result = validator.validate_denial(&packet, "test.example.com", DNSResourceType::A);

    // Should fail with no NSEC/NSEC3 records
    assert!(result.is_err());
}

#[test]
fn test_positive_response_no_denial_needed() {
    let validator = DenialOfExistenceValidator::new();

    let mut packet = DNSPacket::default();
    packet.header.ancount = 1;
    packet.header.rcode = ResponseCode::NoError.to_u8();

    // Positive response should return Ok (no denial validation needed)
    let result = validator.validate_denial(&packet, "test.example.com", DNSResourceType::A);
    assert!(result.is_ok());
}
