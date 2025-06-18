#!/usr/bin/env python3
import socket
import ssl
import struct
import sys

def test_dot_query(host='127.0.0.1', port=8853):
    # Create a DNS query for google.com
    # Header: ID=0x0001, Flags=0x0100 (RD=1), QDCOUNT=1
    dns_query = bytearray()
    dns_query.extend(b'\x00\x01')  # ID
    dns_query.extend(b'\x01\x00')  # Flags (RD=1)
    dns_query.extend(b'\x00\x01')  # QDCOUNT=1
    dns_query.extend(b'\x00\x00')  # ANCOUNT=0
    dns_query.extend(b'\x00\x00')  # NSCOUNT=0
    dns_query.extend(b'\x00\x00')  # ARCOUNT=0
    
    # Question: google.com A
    dns_query.extend(b'\x06google\x03com\x00')  # Domain name
    dns_query.extend(b'\x00\x01')  # Type A
    dns_query.extend(b'\x00\x01')  # Class IN
    
    # Add 2-byte length prefix for DoT
    length_prefix = struct.pack('!H', len(dns_query))
    full_query = length_prefix + dns_query
    
    # Create SSL context
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    
    # Connect with TLS
    with socket.create_connection((host, port)) as sock:
        with context.wrap_socket(sock, server_hostname='localhost') as tls_sock:
            print(f"Connected to {host}:{port}")
            print(f"TLS version: {tls_sock.version()}")
            
            # Send query
            tls_sock.sendall(full_query)
            print(f"Sent DNS query ({len(full_query)} bytes)")
            
            # Read response length
            length_data = tls_sock.recv(2)
            if len(length_data) < 2:
                print("Failed to read response length")
                return
            
            response_length = struct.unpack('!H', length_data)[0]
            print(f"Response length: {response_length} bytes")
            
            # Read response
            response = bytearray()
            while len(response) < response_length:
                chunk = tls_sock.recv(response_length - len(response))
                if not chunk:
                    break
                response.extend(chunk)
            
            if len(response) >= 12:
                # Parse basic header
                id = struct.unpack('!H', response[0:2])[0]
                flags = struct.unpack('!H', response[2:4])[0]
                qdcount = struct.unpack('!H', response[4:6])[0]
                ancount = struct.unpack('!H', response[6:8])[0]
                
                rcode = flags & 0x0F
                print(f"Response: ID={id}, RCODE={rcode}, Questions={qdcount}, Answers={ancount}")
                
                if rcode == 0:
                    print("✅ DoT query successful!")
                else:
                    print(f"❌ DoT query failed with RCODE={rcode}")
            else:
                print("❌ Response too short")

if __name__ == "__main__":
    test_dot_query()