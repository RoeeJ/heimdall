use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::{DNSPacket, DNSPacketRef, PacketBufferPool};
use std::time::Instant;

fn create_test_packet() -> Vec<u8> {
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags: standard query
        0x00, 0x01, // Questions: 1
        0x00, 0x00, // Answers: 0
        0x00, 0x00, // Authority: 0
        0x00, 0x00, // Additional: 0
        // Question: example.com
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm',
        0x00, // End of name
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ]
}

#[test]
fn benchmark_parsing_comparison() {
    let packet_data = create_test_packet();
    let iterations = 1000;

    // Benchmark regular parsing
    let start = Instant::now();
    for _ in 0..iterations {
        let _packet = DNSPacket::parse(&packet_data).unwrap();
    }
    let regular_duration = start.elapsed();

    // Benchmark zero-copy parsing
    let start = Instant::now();
    for _ in 0..iterations {
        let _packet = DNSPacketRef::parse_metadata(&packet_data).unwrap();
    }
    let zerocopy_duration = start.elapsed();

    println!(
        "Regular parsing: {:?} ({:.2} μs/packet)",
        regular_duration,
        regular_duration.as_micros() as f64 / iterations as f64
    );
    println!(
        "Zero-copy parsing: {:?} ({:.2} μs/packet)",
        zerocopy_duration,
        zerocopy_duration.as_micros() as f64 / iterations as f64
    );

    let speedup = regular_duration.as_nanos() as f64 / zerocopy_duration.as_nanos() as f64;
    println!("Zero-copy speedup: {:.2}x", speedup);
}

#[test]
fn benchmark_buffer_pool_vs_allocation() {
    let pool = PacketBufferPool::new(4096, 32);
    let iterations = 1000;

    // Benchmark buffer pool
    let start = Instant::now();
    for _ in 0..iterations {
        let buffer = pool.get_buffer();
        // Simulate some work
        let _ = buffer.capacity();
        pool.return_buffer(buffer);
    }
    let pool_duration = start.elapsed();

    // Benchmark direct allocation
    let start = Instant::now();
    for _ in 0..iterations {
        let buffer = Vec::<u8>::with_capacity(4096);
        // Simulate some work
        let _ = buffer.capacity();
        // Buffer is dropped automatically
    }
    let alloc_duration = start.elapsed();

    println!(
        "Buffer pool: {:?} ({:.2} μs/operation)",
        pool_duration,
        pool_duration.as_micros() as f64 / iterations as f64
    );
    println!(
        "Direct allocation: {:?} ({:.2} μs/operation)",
        alloc_duration,
        alloc_duration.as_micros() as f64 / iterations as f64
    );

    let speedup = alloc_duration.as_nanos() as f64 / pool_duration.as_nanos() as f64;
    println!("Pool speedup: {:.2}x", speedup);
}

#[test]
fn benchmark_cache_key_optimization() {
    let iterations = 10000;
    let domain = "test.example.com".to_string();

    // Benchmark optimized cache key creation (with pre-computed hash)
    let start = Instant::now();
    for _ in 0..iterations {
        let _key = CacheKey::new(domain.clone(), DNSResourceType::A, DNSResourceClass::IN);
    }
    let optimized_duration = start.elapsed();

    println!(
        "Optimized cache key creation: {:?} ({:.2} ns/key)",
        optimized_duration,
        optimized_duration.as_nanos() as f64 / iterations as f64
    );
}

#[test]
fn benchmark_serialization_methods() {
    let packet_data = create_test_packet();
    let packet = DNSPacket::parse(&packet_data).unwrap();
    let iterations = 1000;

    // Benchmark regular serialization
    let start = Instant::now();
    for _ in 0..iterations {
        let _serialized = packet.serialize().unwrap();
    }
    let regular_duration = start.elapsed();

    // Benchmark zero-copy serialization
    let mut buffer = Vec::new();
    let start = Instant::now();
    for _ in 0..iterations {
        let _size = packet.serialize_to_buffer(&mut buffer).unwrap();
    }
    let zerocopy_duration = start.elapsed();

    println!(
        "Regular serialization: {:?} ({:.2} μs/packet)",
        regular_duration,
        regular_duration.as_micros() as f64 / iterations as f64
    );
    println!(
        "Zero-copy serialization: {:?} ({:.2} μs/packet)",
        zerocopy_duration,
        zerocopy_duration.as_micros() as f64 / iterations as f64
    );

    let speedup = regular_duration.as_nanos() as f64 / zerocopy_duration.as_nanos() as f64;
    println!("Zero-copy serialization speedup: {:.2}x", speedup);
}

#[test]
fn benchmark_cache_operations() {
    let cache = DnsCache::new(10000, 300);
    let iterations = 1000;

    // Pre-populate cache
    for i in 0..iterations {
        let key = CacheKey::new(
            format!("test{}.example.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let packet = DNSPacket::default();
        cache.put(key, packet);
    }

    // Benchmark cache hits
    let test_key = CacheKey::new(
        "test500.example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    let start = Instant::now();
    for _ in 0..iterations {
        let _result = cache.get(&test_key);
    }
    let hit_duration = start.elapsed();

    // Benchmark cache misses
    let miss_key = CacheKey::new(
        "nonexistent.example.com".to_string(),
        DNSResourceType::A,
        DNSResourceClass::IN,
    );

    let start = Instant::now();
    for _ in 0..iterations {
        let _result = cache.get(&miss_key);
    }
    let miss_duration = start.elapsed();

    println!(
        "Cache hits: {:?} ({:.2} ns/lookup)",
        hit_duration,
        hit_duration.as_nanos() as f64 / iterations as f64
    );
    println!(
        "Cache misses: {:?} ({:.2} ns/lookup)",
        miss_duration,
        miss_duration.as_nanos() as f64 / iterations as f64
    );

    let hit_miss_ratio = miss_duration.as_nanos() as f64 / hit_duration.as_nanos() as f64;
    println!("Miss/Hit ratio: {:.2}x", hit_miss_ratio);
}
