use std::io::BufWriter;

use rust_bitwriter::BitWriter;

use crate::model::{Answer, Packet, QueryType, Question, Resource};

fn write_qname(bw: &mut BitWriter, qname: &Vec<String>) {
    for part in qname {
        bw.write_u8(part.len() as u8, 8)
            .expect("Failed to part len");
        bw.write(&part.as_bytes().to_vec())
            .expect("Failed to write name part");
    }
    bw.write_u8(0 as u8, 8).expect("Failed to write end byte");
}

pub trait Writable {
    fn to_bytes(&self) -> Vec<u8>;
}

impl Writable for Packet {
    fn to_bytes(&self) -> Vec<u8> {
        let mut writer = BitWriter::new();

        writer
            .write_unsigned_bits(self.id.into(), 16, 16)
            .expect("Failed to write ID");
        writer
            .write_unsigned_bits(self.qr.into(), 1, 1)
            .expect("Failed to write QR");
        writer
            .write_u8(self.op as u8, 4)
            .expect("Failed to write OP");
        writer
            .write_unsigned_bits(self.aa.into(), 1, 1)
            .expect("Failed to write AA");
        writer
            .write_unsigned_bits(self.tc.into(), 1, 1)
            .expect("Failed to write TC");
        writer
            .write_unsigned_bits(self.rd.into(), 1, 1)
            .expect("Failed to write RD");
        writer
            .write_unsigned_bits(self.ra.into(), 1, 1)
            .expect("Failed to write RA");
        writer
            .write_unsigned_bits(self.z.into(), 3, 3)
            .expect("Failed to write Z");
        writer
            .write_unsigned_bits((self.rcode as u8).into(), 4, 4)
            .expect("Failed to write rcode");
        writer
            .write_u16(self.questions.len() as u16, 16)
            .expect("Failed to write qdcount");
        writer
            .write_u16(self.answers.len() as u16, 16)
            .expect("Failed to write ancount");
        writer
            .write_u16(self.name_servers.len() as u16, 16)
            .expect("Failed to write nscount");
        writer
            .write_u16(self.resources.len() as u16, 16)
            .expect("Failed to write arcount");

        self.questions.iter().for_each(|q| {
            writer
                .write(&q.to_bytes())
                .expect("Failed to write question");
        });

        self.answers.iter().for_each(|a| {
            writer.write(&a.to_bytes()).expect("Failed to write answer");
        });

        self.name_servers.iter().for_each(|ns| {
            writer
                .write(&ns.to_bytes())
                .expect("Failed to write name_server");
        });

        self.resources.iter().for_each(|r| {
            writer
                .write(&r.to_bytes())
                .expect("Failed to write resource");
        });

        return writer.data().to_owned();
    }
}

impl Writable for Question {
    /*
    QNAME: [][]u8,
    QTYPE: enums.DNSQueryType,
    QCLASS: enums.DNSClassType,
    */

    fn to_bytes(&self) -> Vec<u8> {
        let mut writer = BitWriter::new();
        write_qname(&mut writer, &self.qname);
        writer
            .write_u16(self.qtype as u16, 16)
            .expect("Failed to write qtype");
        writer
            .write_u16(self.qclass as u16, 16)
            .expect("Failed to write class");
        return writer.data().to_owned();
    }
}

impl Writable for Resource {
    /*
    DOMAIN_NAME: [][]u8,
    QTYPE: enums.DNSQueryType,
    QCLASS: enums.DNSClassType,
    TTL: u32,
    DATA_LENGTH: u16,
    DATA: []u8,
    */

    fn to_bytes(&self) -> Vec<u8> {
        let mut writer = BitWriter::new();
        match &self.name {
            crate::model::Name::Pointer(p) => {
                writer
                    .write_u8(0xc0 as u8, 8)
                    .expect("Failed to write pointer offset");
                writer
                    .write_u8(*p, 8)
                    .expect("Failed to write pointer offset");
            }
            crate::model::Name::String(qname) => {
                write_qname(&mut writer, qname);
            }
            crate::model::Name::Root => {
                writer
                    .write_u8(0x00, 8)
                    .expect("Failed to write root pointer");
            }
            crate::model::Name::Empty => unimplemented!(),
        }

        writer
            .write_u16(self.qtype as u16, 16)
            .expect("Failed to write qtype");
        writer
            .write_u16(self.qclass as u16, 16)
            .expect("Failed to write class");
        writer.write_u32(self.ttl, 32).expect("Failed to write TTL");
        writer
            .write_u16(self.data.len() as u16, 16)
            .expect("Failed to write Data Length");
        writer.write(&self.data).expect("Failed to write data");
        return writer.data().to_owned();
    }
}

// impl Writable for Answer {
//     /*
//     DOMAIN_NAME: [][]u8,
//     QTYPE: enums.DNSQueryType,
//     QCLASS: enums.DNSClassType,
//     TTL: u32,
//     DATA_LENGTH: u16,
//     DATA: []u8,
//     */
//
//     fn to_bytes(&self) -> Vec<u8> {
//         todo!()
//     }
// }
