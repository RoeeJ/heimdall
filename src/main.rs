extern crate num;
#[macro_use]
extern crate num_derive;

mod model;
mod traits;
use std::net::UdpSocket;

use bitreader::BitReader;
use model::*;

use crate::traits::Writable;

fn main() -> ! {
    let sock = UdpSocket::bind(":::1053").expect("Failed to bind :1053");
    loop {
        let mut buf: [u8; 512] = [0; 512];
        if let Ok((bread, from)) = sock.recv_from(&mut buf) {
            println!("Read {} bytes from {}", bread, from);
            let packet = read_packet(&buf);
            let orig = &buf[0..bread];
            let resp = generate_response(&packet);
            std::fs::write("out/orig.bin", &orig)
                .expect("Failed to serialize orig.bin");
            std::fs::write("out/recr.bin", &packet.to_bytes())
                .expect("Failed to serialize recr.bin");
            let porig = read_packet(&orig);
            dbg!(&porig,&resp);
            if let Err(e) = sock.send_to(&resp.to_bytes(), from) {
                eprintln!("ERR:{}", e.to_string());
            }
        }
    }
}

fn generate_response(packet: &Packet) -> Packet {
    let mut resp = packet.clone();
    resp.qr = true;
    resp.ra = resp.rd;
    resp.questions.iter().for_each(|q| {
        resp.answers.push(Resource {
            name: Name::String(q.qname.clone()),
            qtype: q.qtype,
            qclass: q.qclass,
            ttl: 1,
            data: vec![1, 1, 1, 1],
        });
    });
    return resp;
}

fn read_packet(buf: &[u8]) -> Packet {
    let mut reader = BitReader::new(buf);
    let mut packet = Packet::default();
    packet.id = reader.read_u16(16).unwrap();
    packet.qr = reader.read_u8(1).unwrap() == 1;
    packet.op = num::FromPrimitive::from_u8(reader.read_u8(4).expect("Failed to read packet OP"))
        .unwrap_or(Opcode::Other);
    packet.aa = reader.read_u8(1).unwrap() == 1;
    packet.tc = reader.read_u8(1).unwrap() == 1;
    packet.rd = reader.read_u8(1).unwrap() == 1;
    packet.ra = reader.read_u8(1).unwrap() == 1;
    packet.z = ux::u3::new(reader.read_u8(3).unwrap() as u8);
    packet.rcode =
        num::FromPrimitive::from_u8(reader.read_u8(4).expect("Failed to read packet OP"))
            .unwrap_or(RCode::UNK);
    packet.qdcount = reader.read_u16(16).unwrap();
    packet.ancount = reader.read_u16(16).unwrap();
    packet.nscount = reader.read_u16(16).unwrap();
    packet.arcount = reader.read_u16(16).unwrap();

    for _ in 0..packet.qdcount {
        packet.questions.push(read_question(&mut reader));
    }

    for _ in 0..packet.ancount {
        println!("Read Answer");
        // packet.questions.push(read_answer(&mut reader));
    }

    for _ in 0..packet.nscount {
        println!("Read NS");
        // packet.questions.push(read_ns(&mut reader));
    }

    for _ in 0..packet.arcount {
        println!("Read Additional Resource",);
        if let Some(r) = read_resource(&mut reader) {
            if r.qtype == QueryType::OPT {
                continue;
            }
            packet.resources.push(r);
        }
    }

    packet.qdcount = packet.questions.len() as u16;
    packet.ancount = packet.answers.len() as u16;
    packet.nscount = packet.name_servers.len() as u16;
    packet.arcount = packet.resources.len() as u16;

    println!("QD:{}/AN:{}/NS:{}/AR:{}",packet.qdcount,packet.ancount,packet.nscount,packet.arcount);
    packet
}

fn read_resource(reader: &mut BitReader) -> Option<Resource> {
    let mut r = Resource::default();
    let mut parts = Vec::new();
    let name: Name;
    loop {
        let partlen = reader.read_u8(8).unwrap_or_default() as usize;
        if partlen == 0 && parts.len() == 0 {
            name = Name::Root;
            break;
        } else if partlen == 0xc0 {
            let p = reader.read_u8(8).unwrap_or_default();
            name = Name::Pointer(p);
            break;
        }
        let mut part = vec![0; partlen];
        let _ = reader.read_u8_slice(part.as_mut_slice());

        if let Ok(s) = String::from_utf8(part) {
            parts.push(s);
        }
    }

    r.qtype = num::FromPrimitive::from_u16(reader.read_u16(16).expect("Failed to read QueryType"))
        .unwrap_or(QueryType::UNK);
    r.qclass =
        num::FromPrimitive::from_u16(reader.read_u16(16).expect("Failed to read QueryClass"))
            .unwrap_or(QueryClass::ANY);

    match name {
        Name::Empty => {
            r.name = Name::String(parts);
        }
        n => {
            r.name = n;
        }
    }
    return Some(r);
}

fn read_question(reader: &mut BitReader) -> Question {
    let mut q = Question::default();
    loop {
        let partlen = reader.read_u8(8).expect("Failed to read name part") as usize;
        if partlen == 0 {
            break;
        }
        let mut part: Vec<u8> = vec![0; partlen];
        let _ = reader.read_u8_slice(part.as_mut_slice());
        if let Ok(s) = String::from_utf8(part) {
            q.qname.push(s);
        }
    }
    q.qtype = num::FromPrimitive::from_u16(reader.read_u16(16).expect("Failed to read QueryType"))
        .unwrap_or(QueryType::UNK);
    q.qclass =
        num::FromPrimitive::from_u16(reader.read_u16(16).expect("Failed to read QueryClass"))
            .unwrap_or(QueryClass::ANY);
    return q;
}
