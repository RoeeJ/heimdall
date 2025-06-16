#!/bin/bash

echo "Testing DNS-over-TLS on port 8853..."

# Test 1: Check if port is open
echo "1. Checking if port 8853 is open..."
nc -zv 127.0.0.1 8853

# Test 2: Test TLS connection with openssl
echo -e "\n2. Testing TLS handshake with openssl..."
echo | timeout 5 openssl s_client -connect 127.0.0.1:8853 -servername localhost 2>&1 | grep -E "subject=|issuer=|Verify return code"

# Test 3: Send a DNS query over TLS using a simple Python script
echo -e "\n3. Testing DNS query over TLS..."
python3 -c "
import socket
import ssl
import struct

# Create DNS query for google.com
def create_dns_query(domain):
    # Transaction ID
    transaction_id = b'\\x12\\x34'
    # Flags: standard query
    flags = b'\\x01\\x00'
    # Questions: 1
    questions = b'\\x00\\x01'
    # Answer RRs: 0
    answer_rrs = b'\\x00\\x00'
    # Authority RRs: 0
    authority_rrs = b'\\x00\\x00'
    # Additional RRs: 0
    additional_rrs = b'\\x00\\x00'
    
    header = transaction_id + flags + questions + answer_rrs + authority_rrs + additional_rrs
    
    # Question section
    question = b''
    for part in domain.split('.'):
        question += bytes([len(part)]) + part.encode()
    question += b'\\x00'  # End of domain
    question += b'\\x00\\x01'  # Type A
    question += b'\\x00\\x01'  # Class IN
    
    return header + question

# Create query
query = create_dns_query('google.com')
length = struct.pack('!H', len(query))

# Connect with TLS (disable cert verification for self-signed cert)
context = ssl.create_default_context()
context.check_hostname = False
context.verify_mode = ssl.CERT_NONE

try:
    with socket.create_connection(('127.0.0.1', 8853)) as sock:
        with context.wrap_socket(sock) as ssock:
            # Send length-prefixed DNS query
            ssock.send(length + query)
            
            # Read response length
            resp_length_data = ssock.recv(2)
            if len(resp_length_data) == 2:
                resp_length = struct.unpack('!H', resp_length_data)[0]
                print(f'Response length: {resp_length} bytes')
                
                # Read response
                response = ssock.recv(resp_length)
                print(f'Received {len(response)} bytes')
                print('DoT test successful!')
            else:
                print('Failed to read response length')
except Exception as e:
    print(f'Error: {e}')
"

echo -e "\n4. Alternative test with dig through stunnel..."
echo "To test with dig, you would need to set up stunnel with a config like:"
echo "
[dns-tls]
client = yes
accept = 127.0.0.1:5353
connect = 127.0.0.1:8853
"
echo "Then use: dig @127.0.0.1 -p 5353 google.com"