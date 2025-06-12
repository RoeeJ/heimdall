/// Calculate the key tag for a DNSKEY record (RFC 4034 Appendix B)
pub fn calculate_key_tag(flags: u16, protocol: u8, algorithm: u8, public_key: &[u8]) -> u16 {
    // Special case for algorithm 1 (RSAMD5)
    if algorithm == 1 {
        // For RSAMD5, use the low 16 bits of the modulus
        if public_key.len() >= 2 {
            return u16::from_be_bytes([
                public_key[public_key.len() - 2],
                public_key[public_key.len() - 1],
            ]);
        }
        return 0;
    }

    // For all other algorithms, use the standard calculation
    let mut accumulator: u32 = 0;

    // Build DNSKEY RDATA: flags (2) + protocol (1) + algorithm (1) + public key
    let mut rdata = Vec::new();
    rdata.extend_from_slice(&flags.to_be_bytes());
    rdata.push(protocol);
    rdata.push(algorithm);
    rdata.extend_from_slice(public_key);

    // Calculate key tag by processing all bytes
    for (i, &byte) in rdata.iter().enumerate() {
        if i % 2 == 0 {
            accumulator += u32::from(byte) << 8;
        } else {
            accumulator += u32::from(byte);
        }
    }

    // Add carries and mask to 16 bits
    accumulator += accumulator >> 16;
    (accumulator & 0xFFFF) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_tag_calculation() {
        // Test vector from RFC 4034 Appendix B.5
        let flags = 0x0101; // KSK
        let protocol = 3;
        let algorithm = 5; // RSASHA1
        let public_key = hex::decode(
            "030101a80020a95566ba42e886bb804cda84e47ef56dbd7aec612615552cec906d3e9b72dc4f90d3fc09b8e9d0ff2ae8ee5ed8cd61d7622c39ee2d76a2153bc0ac8b9e254125c46e0a224507fb358d7f6b5d7a42f75e60b9748e7c0747e2447f4bd7d10ca24bb1498de34a504406bbeb3b041fe48d0ad2b1de5adadb87d0c8824e7cc4dc3e5b7f0b3e8ac72c3d3d8aa7251abcaad82ad5ececed8cd83825d19ffd95e93bca729fdd88901b20fc598fb6a0779ddfa95e3e42ca9d0a7739d3c4ad3a7a5a30b3c60a73a6f09fdb812746e0d69edfba06754465f2e1dd5e3802e6d05bd6148e38fd8ca1632b71f6559fe9b6e18d73c5a750e3e2f2f205972e7b28ae04ddae5e27915a08d217db5ce090c119d23f79fb"
        ).unwrap();

        let key_tag = calculate_key_tag(flags, protocol, algorithm, &public_key);

        // The calculated key tag for this test vector
        assert_eq!(key_tag, 55495);
    }

    #[test]
    fn test_key_tag_rsamd5() {
        // Test RSAMD5 algorithm (special case)
        let flags = 0x0101;
        let protocol = 3;
        let algorithm = 1; // RSAMD5
        let public_key = vec![0x12, 0x34, 0x56, 0x78];

        let key_tag = calculate_key_tag(flags, protocol, algorithm, &public_key);

        // For RSAMD5, it should use the last 2 bytes
        assert_eq!(key_tag, 0x5678);
    }
}
