use bitstream_io::{BigEndian, BitRead, BitReader};
use std::io::Cursor;

// Helper function to encode domain names in DNS wire format
pub fn encode_domain_name(name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    // Remove trailing dot if present
    let name = name.trim_end_matches('.');
    for label in name.split('.') {
        let len = label.len() as u8;
        bytes.push(len);
        bytes.extend(label.as_bytes());
    }
    bytes.push(0); // Root label
    bytes
}

// Helper function to decode domain names from DNS wire format
pub fn decode_domain_name(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<String, std::io::Error> {
    let mut name = Vec::new();
    
    // Read first byte to check for root domain
    let first = reader.read::<u8>(8)?;
    if first == 0 {
        return Ok(".".to_string());
    }
    
    // Process first label
    if (first & 0xC0) == 0xC0 {
        reader.read::<u8>(8)?; // Skip second byte of pointer
        return Ok(".".to_string()); // Return root for compressed root
    }
    
    if first > 63 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Label too long"
        ));
    }
    
    // Read first label
    for _ in 0..first {
        name.push(reader.read::<u8>(8)?);
    }
    
    // Read remaining labels
    loop {
        let len = reader.read::<u8>(8)?;
        if len == 0 {
            break;
        }

        if (len & 0xC0) == 0xC0 {
            reader.read::<u8>(8)?; // Skip second byte of pointer
            break;
        }

        if len > 63 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Label too long"
            ));
        }

        name.push(b'.');
        for _ in 0..len {
            name.push(reader.read::<u8>(8)?);
        }
    }

    String::from_utf8(name).map_err(|_| std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Invalid UTF-8 in domain name"
    ))
}