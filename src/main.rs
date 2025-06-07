use dns::DNSPacket;
use tokio::net::UdpSocket;

pub mod dns;

#[tokio::main]
async fn main() {
    let port = 1053;
    let mut sock = UdpSocket::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("Listening on 0.0.0.0:{}", port);

    let mut buf = vec![0; 4096];
    loop {
        let (read_bytes, addr) = sock.recv_from(&mut buf).await.unwrap();
        std::fs::write("packet.bin", &buf[..read_bytes]).unwrap();
        let packet = DNSPacket::parse(&buf[..read_bytes]);
        match packet {
            Ok(packet) => {
                match packet.generate_response().serialize() {
                    Ok(serialized) => {
                        sock.send_to(&serialized, addr).await.unwrap();
                    }
                    Err(e) => println!("Error serializing packet: {:?}", e),
                };
            }
            Err(e) => println!("Error parsing packet: {:?}", e),
        }
    }
}
