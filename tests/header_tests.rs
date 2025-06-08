use bitstream_io::{BigEndian, BitReader, BitWriter};
use heimdall::dns::common::PacketComponent;
use heimdall::dns::header::DNSHeader;

#[test]
fn test_header_read_write_roundtrip() {
    let original = DNSHeader {
        id: 0xABCD,
        qr: true,
        opcode: 2,
        aa: true,
        tc: false,
        rd: true,
        ra: false,
        z: 0,
        rcode: 3,
        qdcount: 5,
        ancount: 2,
        nscount: 1,
        arcount: 0,
    };

    // Write to buffer
    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
        original.write(&mut writer).expect("Failed to write header");
    }

    // Read back from buffer
    let mut reader = BitReader::<_, BigEndian>::new(&buffer[..]);
    let mut parsed = DNSHeader::default();
    parsed.read(&mut reader).expect("Failed to read header");

    // Verify all fields match
    assert_eq!(parsed.id, original.id);
    assert_eq!(parsed.qr, original.qr);
    assert_eq!(parsed.opcode, original.opcode);
    assert_eq!(parsed.aa, original.aa);
    assert_eq!(parsed.tc, original.tc);
    assert_eq!(parsed.rd, original.rd);
    assert_eq!(parsed.ra, original.ra);
    assert_eq!(parsed.z, original.z);
    assert_eq!(parsed.rcode, original.rcode);
    assert_eq!(parsed.qdcount, original.qdcount);
    assert_eq!(parsed.ancount, original.ancount);
    assert_eq!(parsed.nscount, original.nscount);
    assert_eq!(parsed.arcount, original.arcount);
}

#[test]
fn test_header_flags_packing() {
    let header = DNSHeader {
        id: 0x1234,
        qr: true,    // bit 15
        opcode: 0xA, // bits 14-11 (1010)
        aa: true,    // bit 10
        tc: false,   // bit 9
        rd: true,    // bit 8
        ra: false,   // bit 7
        z: 0x5,      // bits 6-4 (101)
        rcode: 0xF,  // bits 3-0 (1111)
        ..Default::default()
    };

    let mut buffer = Vec::new();
    {
        let mut writer = BitWriter::<_, BigEndian>::new(&mut buffer);
        header.write(&mut writer).expect("Failed to write header");
    }

    // Check the flags byte packing
    assert_eq!(buffer[0], 0x12); // ID high byte
    assert_eq!(buffer[1], 0x34); // ID low byte
    assert_eq!(buffer[2], 0xD5); // QR=1, Opcode=1010, AA=1, TC=0, RD=1
    assert_eq!(buffer[3], 0x5F); // RA=0, Z=101, RCODE=1111
}

#[test]
fn test_header_default_values() {
    let header = DNSHeader::default();

    assert_eq!(header.id, 0);
    assert!(!header.qr);
    assert_eq!(header.opcode, 0);
    assert!(!header.aa);
    assert!(!header.tc);
    assert!(!header.rd);
    assert!(!header.ra);
    assert_eq!(header.z, 0);
    assert_eq!(header.rcode, 0);
    assert_eq!(header.qdcount, 0);
    assert_eq!(header.ancount, 0);
    assert_eq!(header.nscount, 0);
    assert_eq!(header.arcount, 0);
}
