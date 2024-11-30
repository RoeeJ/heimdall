use std::io::Cursor;

use bitstream_io::{BigEndian, BitReader};


pub trait DnsWireFormat {
    fn to_wire(&self) -> Vec<u8>;
    fn from_wire(reader: &mut BitReader<Cursor<&[u8]>, BigEndian>) -> Result<Self, std::io::Error> where Self: Sized;
}