#!/usr/bin/env python3
"""Test script to debug dig +trace issues with Heimdall DNS server"""

import socket
import struct
import sys

def build_dns_query(query_id=12345, domain=".", qtype=2, qclass=1, recursive=False):
    """Build a DNS query packet for the given domain and type"""
    
    # DNS header
    flags = 0
    if not recursive:
        flags = 0  # RD bit not set
    else:
        flags = 0x0100  # RD bit set
    
    header = struct.pack('!HHHHHH', 
        query_id,    # ID
        flags,       # Flags
        1,           # QDCOUNT
        0,           # ANCOUNT  
        0,           # NSCOUNT
        0            # ARCOUNT
    )
    
    # DNS question
    question = b''
    
    # Encode domain name
    if domain == ".":
        # Root zone - just a null byte
        question += b'\x00'
    else:
        # Normal domain
        parts = domain.rstrip('.').split('.')
        for part in parts:
            question += bytes([len(part)]) + part.encode('ascii')
        question += b'\x00'
    
    # Add type and class
    question += struct.pack('!HH', qtype, qclass)
    
    return header + question

def parse_dns_response(data):
    """Parse DNS response and show what was received"""
    if len(data) < 12:
        print("Response too short")
        return
        
    # Parse header
    header = struct.unpack('!HHHHHH', data[:12])
    query_id = header[0]
    flags = header[1]
    qdcount = header[2]
    ancount = header[3]
    nscount = header[4]
    arcount = header[5]
    
    print(f"Response ID: {query_id}")
    print(f"Flags: 0x{flags:04x}")
    print(f"Questions: {qdcount}")
    print(f"Answers: {ancount}")
    print(f"Authority: {nscount}")
    print(f"Additional: {arcount}")
    
    # Parse question section
    offset = 12
    for i in range(qdcount):
        print(f"\nQuestion {i+1}:")
        # Read domain name
        domain_parts = []
        while offset < len(data):
            length = data[offset]
            offset += 1
            if length == 0:
                break
            elif (length & 0xc0) == 0xc0:
                # Compression pointer
                pointer = ((length & 0x3f) << 8) | data[offset]
                offset += 1
                print(f"  [Compression pointer to offset {pointer}]")
                break
            else:
                domain_part = data[offset:offset+length].decode('ascii', errors='replace')
                domain_parts.append(domain_part)
                offset += length
        
        if domain_parts:
            print(f"  Domain: {'.'.join(domain_parts)}")
        else:
            print(f"  Domain: . (root)")
            
        if offset + 4 <= len(data):
            qtype, qclass = struct.unpack('!HH', data[offset:offset+4])
            offset += 4
            print(f"  Type: {qtype}")
            print(f"  Class: {qclass}")
        else:
            print(f"  [Incomplete question at offset {offset}]")

def test_query(server="127.0.0.1", port=1053, domain=".", qtype=2):
    """Send a DNS query and display the response"""
    
    print(f"\nTesting query for '{domain}' type {qtype} (NS={qtype==2})")
    
    # Build query
    query = build_dns_query(domain=domain, qtype=qtype, recursive=False)
    print(f"Query packet ({len(query)} bytes): {query.hex()}")
    
    # Send UDP query
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(5.0)
    
    try:
        sock.sendto(query, (server, port))
        response, addr = sock.recvfrom(4096)
        print(f"\nReceived response ({len(response)} bytes) from {addr}")
        print(f"Response hex: {response.hex()}")
        print("\nParsed response:")
        parse_dns_response(response)
        
    except socket.timeout:
        print("No response received (timeout)")
    except Exception as e:
        print(f"Error: {e}")
    finally:
        sock.close()

if __name__ == "__main__":
    # Make sure Heimdall is running
    print("Testing Heimdall DNS server on port 1053")
    print("Make sure the server is running: cargo run")
    
    # Test 1: Root zone NS query (what dig +trace does first)
    test_query(domain=".", qtype=2)
    
    # Test 2: Unknown type query  
    print("\n" + "="*60)
    test_query(domain=".", qtype=512)
    
    # Test 3: Normal domain query for comparison
    print("\n" + "="*60)
    test_query(domain="google.com", qtype=1)