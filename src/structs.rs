
use crate::enums::{DnsOpcode, DnsQClass, DnsQType, DnsQr, DnsResponseCode};

#[derive(Debug)]
pub struct DnsPacket {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsAnswer>,
    pub authorities: Vec<DnsAuthority>,
    pub additional: Vec<DnsAdditional>,
}

#[derive(Debug)]
pub struct DnsQuestion {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
}

#[derive(Debug)]
pub struct DnsAnswer {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
    pub ttl: u32,
    pub length: u16,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct DnsAuthority {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
    pub ttl: u32,
    pub length: u16,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct DnsAdditional {
    pub name: String,
    pub qtype: DnsQType,
    pub qclass: DnsQClass,
    pub ttl: u32,
    pub length: u16,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct DnsHeader {
    pub id: u16,
    pub qr: DnsQr,
    pub opcode: DnsOpcode,
    pub aa: u8,
    pub tc: u8,
    pub rd: u8,
    pub ra: u8,
    pub z: u8,
    pub rcode: DnsResponseCode,
}