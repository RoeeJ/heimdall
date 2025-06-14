#[cfg(test)]
mod tests {
    use crate::dns::DNSPacket;
    use crate::dns::enums::DNSResourceType;

    #[test]
    fn test_cname_compression_pointer_parsing() {
        // This is the actual DNS response for www.ynet.co.il from 8.8.8.8
        // It contains multiple CNAME records with compression pointers
        let packet_data = vec![
            // Header (12 bytes)
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
            // Question: www.ynet.co.il A IN
            0x03, 0x77, 0x77, 0x77, 0x04, 0x79, 0x6e, 0x65, 0x74, 0x02, 0x63, 0x6f, 0x02, 0x69,
            0x6c, 0x00, 0x00, 0x01, 0x00, 0x01,
            // Answer 1: www.ynet.co.il CNAME www.ynet.co.il-v1.edgekey.net
            0xc0, 0x0c, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x8a, 0x00, 0x1f, 0x03, 0x77,
            0x77, 0x77, 0x04, 0x79, 0x6e, 0x65, 0x74, 0x02, 0x63, 0x6f, 0x05, 0x69, 0x6c, 0x2d,
            0x76, 0x31, 0x07, 0x65, 0x64, 0x67, 0x65, 0x6b, 0x65, 0x79, 0x03, 0x6e, 0x65, 0x74,
            0x00,
            // Answer 2: www.ynet.co.il-v1.edgekey.net CNAME e12476.dscb.akamaiedge.net
            0xc0, 0x2c, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x34, 0x37, 0x00, 0x19, 0x06, 0x65,
            0x31, 0x32, 0x34, 0x37, 0x36, 0x04, 0x64, 0x73, 0x63, 0x62, 0x0a, 0x61, 0x6b, 0x61,
            0x6d, 0x61, 0x69, 0x65, 0x64, 0x67, 0x65, 0xc0, 0x46,
            // Answer 3: e12476.dscb.akamaiedge.net A 104.79.201.182
            0xc0, 0x57, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04, 0x68, 0x4f,
            0xc9, 0xb6,
        ];

        // Parse the packet
        let packet = DNSPacket::parse(&packet_data).expect("Failed to parse packet");

        // Verify we have 3 answers
        assert_eq!(packet.answers.len(), 3);

        // Check first CNAME
        let answer1 = &packet.answers[0];
        assert_eq!(answer1.labels.join("."), "www.ynet.co.il");
        assert_eq!(answer1.rtype, DNSResourceType::CNAME);
        assert_eq!(
            answer1.parsed_rdata,
            Some("www.ynet.co.il-v1.edgekey.net".to_string())
        );

        // Check second CNAME - this is where the bug was occurring
        let answer2 = &packet.answers[1];
        assert_eq!(answer2.labels.join("."), "www.ynet.co.il-v1.edgekey.net");
        assert_eq!(answer2.rtype, DNSResourceType::CNAME);
        // The bug caused this to be "e12476.dscb.akamaiedge.il-v1.edgekey.net"
        // instead of the correct "e12476.dscb.akamaiedge.net"
        assert_eq!(
            answer2.parsed_rdata,
            Some("e12476.dscb.akamaiedge.net".to_string()),
            "CNAME target should be e12476.dscb.akamaiedge.net, not corrupted"
        );

        // Check A record
        let answer3 = &packet.answers[2];
        assert_eq!(answer3.labels.join("."), "e12476.dscb.akamaiedge.net");
        assert_eq!(answer3.rtype, DNSResourceType::A);
        assert_eq!(answer3.parsed_rdata, Some("104.79.201.182".to_string()));
    }

    #[test]
    fn test_compression_pointer_middle_of_domain() {
        // Test case where compression pointer points to the middle of another domain
        // This tests the specific bug where we read too much when following pointers
        let packet_data = vec![
            // Header (12 bytes)
            0x00, 0x00, 0x81, 0x80, // ID and flags
            0x00, 0x00, // QDCOUNT = 0
            0x00, 0x02, // ANCOUNT = 2
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
            // First record: example.com A record
            0x07, 0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, // "example"
            0x03, 0x63, 0x6f, 0x6d, 0x00, // "com" + null
            0x00, 0x01, // A
            0x00, 0x01, // IN
            0x00, 0x00, 0x00, 0x3c, // TTL = 60
            0x00, 0x04, // RDLENGTH = 4
            0x01, 0x02, 0x03, 0x04, // IP 1.2.3.4
            // Second record: test.com CNAME target.com (using compression)
            0x04, 0x74, 0x65, 0x73, 0x74, // "test"
            0xc0, 0x14, // Compression pointer to offset 20 (".com" part)
            0x00, 0x05, // CNAME
            0x00, 0x01, // IN
            0x00, 0x00, 0x00, 0x3c, // TTL = 60
            0x00, 0x09, // RDLENGTH = 9
            0x06, 0x74, 0x61, 0x72, 0x67, 0x65, 0x74, // "target"
            0xc0, 0x14, // Compression pointer to ".com"
        ];

        let packet = DNSPacket::parse(&packet_data).expect("Failed to parse packet");

        assert_eq!(packet.answers.len(), 2);

        // Check the CNAME record
        let cname = &packet.answers[1];
        assert_eq!(cname.labels.join("."), "test.com");
        assert_eq!(cname.rtype, DNSResourceType::CNAME);
        assert_eq!(
            cname.parsed_rdata,
            Some("target.com".to_string()),
            "CNAME should point to target.com, not include extra labels"
        );
    }
}
