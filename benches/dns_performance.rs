use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::simd::SimdParser;
use heimdall::dns::{DNSPacket, DNSPacketRef, PacketBufferPool};
use std::hint::black_box;
use std::time::Duration;

fn create_test_packet() -> Vec<u8> {
    // A real DNS query packet for google.com
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags: standard query
        0x00, 0x01, // Questions: 1
        0x00, 0x00, // Answers: 0
        0x00, 0x00, // Authority: 0
        0x00, 0x00, // Additional: 0
        // Question: google.com
        0x06, b'g', b'o', b'o', b'g', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, // End of name
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ]
}

fn create_large_packet() -> Vec<u8> {
    // Create a larger packet with multiple questions and compression
    let mut packet = vec![
        0x12, 0x34, // ID
        0x81, 0x80, // Flags: standard response
        0x00, 0x03, // Questions: 3
        0x00, 0x03, // Answers: 3
        0x00, 0x00, // Authority: 0
        0x00, 0x01, // Additional: 1
    ];

    // Add questions
    for i in 0..3 {
        packet.extend_from_slice(&[0x04, b't', b'e', b's', b't']);
        packet.push(i + b'0');
        packet.extend_from_slice(&[0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e']);
        packet.extend_from_slice(&[0x03, b'c', b'o', b'm', 0x00]);
        packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A record, IN class
    }

    // Add answers with compression pointers
    for i in 0..3 {
        packet.extend_from_slice(&[0xC0, 0x0C + (i * 18)]); // Compression pointer
        packet.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A record, IN class
        packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x3C]); // TTL: 60
        packet.extend_from_slice(&[0x00, 0x04]); // RDLENGTH: 4
        packet.extend_from_slice(&[192, 168, 1, i + 1]); // IP address
    }

    // Add EDNS OPT record
    packet.extend_from_slice(&[0x00]); // Root domain
    packet.extend_from_slice(&[0x00, 0x29]); // OPT
    packet.extend_from_slice(&[0x10, 0x00]); // UDP payload size: 4096
    packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Extended RCODE and flags
    packet.extend_from_slice(&[0x00, 0x00]); // RDLENGTH: 0

    packet
}

fn benchmark_packet_parsing(c: &mut Criterion) {
    let small_packet = create_test_packet();
    let large_packet = create_large_packet();

    let mut group = c.benchmark_group("packet_parsing");

    // Benchmark regular parsing
    group.bench_with_input(
        BenchmarkId::new("regular", "small_packet"),
        &small_packet,
        |b, packet| {
            b.iter(|| {
                let parsed = DNSPacket::parse(black_box(packet)).unwrap();
                black_box(parsed);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("regular", "large_packet"),
        &large_packet,
        |b, packet| {
            b.iter(|| {
                let parsed = DNSPacket::parse(black_box(packet)).unwrap();
                black_box(parsed);
            });
        },
    );

    // Benchmark zero-copy parsing
    group.bench_with_input(
        BenchmarkId::new("zero_copy", "small_packet"),
        &small_packet,
        |b, packet| {
            b.iter(|| {
                let parsed = DNSPacketRef::parse_metadata(black_box(packet)).unwrap();
                black_box(parsed);
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("zero_copy", "large_packet"),
        &large_packet,
        |b, packet| {
            b.iter(|| {
                let parsed = DNSPacketRef::parse_metadata(black_box(packet)).unwrap();
                black_box(parsed);
            });
        },
    );

    // Benchmark SIMD-hint parsing
    group.bench_with_input(
        BenchmarkId::new("simd_hint", "large_packet"),
        &large_packet,
        |b, packet| {
            b.iter(|| {
                let parsed = DNSPacket::parse_with_simd_hint(black_box(packet)).unwrap();
                black_box(parsed);
            });
        },
    );

    group.finish();
}

fn benchmark_buffer_pool(c: &mut Criterion) {
    let pool = PacketBufferPool::new(4096, 32);

    c.bench_function("buffer_pool_get_return", |b| {
        b.iter(|| {
            let buffer = pool.get_buffer();
            black_box(&buffer);
            pool.return_buffer(buffer);
        });
    });

    c.bench_function("buffer_allocation", |b| {
        b.iter(|| {
            let buffer = Vec::<u8>::with_capacity(4096);
            black_box(buffer);
        });
    });
}

fn benchmark_cache_operations(c: &mut Criterion) {
    let cache = DnsCache::new(10000, 300);
    let mut group = c.benchmark_group("cache_operations");

    // Pre-populate cache
    for i in 0..1000 {
        let key = CacheKey::new(
            format!("test{}.example.com", i),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        let packet = DNSPacket::default();
        cache.put(key, packet);
    }

    // Benchmark cache hits
    group.bench_function("cache_hit", |b| {
        let key = CacheKey::new(
            "test500.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        b.iter(|| {
            let result = cache.get(black_box(&key));
            black_box(result);
        });
    });

    // Benchmark cache misses
    group.bench_function("cache_miss", |b| {
        let key = CacheKey::new(
            "nonexistent.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );
        b.iter(|| {
            let result = cache.get(black_box(&key));
            black_box(result);
        });
    });

    // Benchmark cache key creation with pre-computed hash
    group.bench_function("cache_key_creation", |b| {
        b.iter(|| {
            let key = CacheKey::new(
                black_box("test.example.com".to_string()),
                black_box(DNSResourceType::A),
                black_box(DNSResourceClass::IN),
            );
            black_box(key);
        });
    });

    group.finish();
}

fn benchmark_simd_operations(c: &mut Criterion) {
    let test_data = create_large_packet();
    let mut group = c.benchmark_group("simd_operations");

    // Benchmark compression pointer search
    group.bench_function("find_compression_pointers", |b| {
        b.iter(|| {
            let pointers = SimdParser::find_compression_pointers_simd(black_box(&test_data));
            black_box(pointers);
        });
    });

    // Benchmark record type pattern search
    group.bench_function("find_a_records", |b| {
        b.iter(|| {
            let positions = SimdParser::find_record_type_pattern_simd(
                black_box(&test_data),
                black_box(&[0x00, 0x01]),
            );
            black_box(positions);
        });
    });

    // Benchmark checksum calculation
    group.bench_function("packet_checksum", |b| {
        b.iter(|| {
            let checksum = SimdParser::calculate_packet_checksum_simd(black_box(&test_data));
            black_box(checksum);
        });
    });

    // Benchmark domain validation
    let domain_data = b"test.example.com";
    group.bench_function("domain_validation", |b| {
        b.iter(|| {
            let valid = SimdParser::validate_domain_name_simd(black_box(domain_data));
            black_box(valid);
        });
    });

    group.finish();
}

fn benchmark_serialization(c: &mut Criterion) {
    let packet = DNSPacket::parse(&create_large_packet()).unwrap();
    let mut buffer = Vec::with_capacity(512);

    let mut group = c.benchmark_group("serialization");

    // Benchmark regular serialization
    group.bench_function("regular_serialize", |b| {
        b.iter(|| {
            let serialized = packet.serialize().unwrap();
            black_box(serialized);
        });
    });

    // Benchmark zero-copy serialization
    group.bench_function("zero_copy_serialize", |b| {
        b.iter(|| {
            buffer.clear();
            let size = packet.serialize_to_buffer(black_box(&mut buffer)).unwrap();
            black_box(size);
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5));
    targets =
        benchmark_packet_parsing,
        benchmark_buffer_pool,
        benchmark_cache_operations,
        benchmark_simd_operations,
        benchmark_serialization
}

criterion_main!(benches);
