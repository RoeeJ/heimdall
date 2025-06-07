use dns::DNSPacket;
use tokio::net::UdpSocket;

pub mod dns;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = 1053;
    let sock = UdpSocket::bind(format!("127.0.0.1:{}", port)).await?;
    println!("Listening on 127.0.0.1:{}", port);

    // Pre-allocate buffer outside loop for efficiency
    let mut buf = vec![0; 4096];
    
    loop {
        let (read_bytes, src_addr) = sock.recv_from(&mut buf).await?;
        
        // Parse the DNS packet
        match DNSPacket::parse(&buf[..read_bytes]) {
            Ok(packet) => {
                // TODO: Implement proper DNS resolution
                eprintln!("Received DNS query from {}: {:?}", src_addr, packet.header);
                
                // For now, generate a basic response
                let response = packet.generate_response();
                match response.serialize() {
                    Ok(serialized) => {
                        sock.send_to(&serialized, src_addr).await?;
                    }
                    Err(e) => eprintln!("Failed to serialize response: {:?}", e),
                }
            }
            Err(e) => eprintln!("Failed to parse packet from {}: {:?}", src_addr, e),
        }
    }
}
