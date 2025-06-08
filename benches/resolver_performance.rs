use criterion::{Criterion, criterion_group, criterion_main};
use heimdall::config::DnsConfig;
use heimdall::dns::DNSPacket;
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::question::DNSQuestion;
use heimdall::resolver::DnsResolver;
use std::hint::black_box;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

fn create_query_packet(domain: &str) -> DNSPacket {
    let mut packet = DNSPacket::default();
    packet.header.id = 1234;
    packet.header.rd = true;
    packet.header.qdcount = 1;

    let question = DNSQuestion {
        labels: domain.split('.').map(|s| s.to_string()).collect(),
        qtype: DNSResourceType::A,
        qclass: DNSResourceClass::IN,
    };

    packet.questions.push(question);
    packet
}

fn benchmark_parallel_vs_sequential(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("resolver_parallel");

    // Create configs with multiple upstream servers
    let servers = vec![
        "8.8.8.8:53".parse::<SocketAddr>().unwrap(),
        "8.8.4.4:53".parse::<SocketAddr>().unwrap(),
        "1.1.1.1:53".parse::<SocketAddr>().unwrap(),
    ];

    // Config with parallel queries enabled
    let parallel_config = DnsConfig {
        upstream_servers: servers.clone(),
        enable_parallel_queries: true,
        enable_caching: false, // Disable caching for fair comparison
        ..Default::default()
    };

    // Config with parallel queries disabled
    let sequential_config = DnsConfig {
        upstream_servers: servers,
        enable_parallel_queries: false,
        enable_caching: false,
        ..Default::default()
    };

    let query = create_query_packet("example.com");

    // Benchmark parallel resolution
    group.bench_function("parallel_resolution", |b| {
        let resolver = rt
            .block_on(DnsResolver::new(parallel_config.clone()))
            .unwrap();
        b.iter(|| {
            rt.block_on(async {
                let response = resolver.resolve(black_box(query.clone()), 1234).await;
                let _ = black_box(response);
            })
        });
    });

    // Benchmark sequential resolution
    group.bench_function("sequential_resolution", |b| {
        let resolver = rt
            .block_on(DnsResolver::new(sequential_config.clone()))
            .unwrap();
        b.iter(|| {
            rt.block_on(async {
                let response = resolver.resolve(black_box(query.clone()), 1234).await;
                let _ = black_box(response);
            })
        });
    });

    group.finish();
}

fn benchmark_query_deduplication(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("concurrent_identical_queries", |b| {
        let config = DnsConfig::default();
        let resolver = Arc::new(rt.block_on(DnsResolver::new(config)).unwrap());
        let query = create_query_packet("test.example.com");

        b.iter(|| {
            rt.block_on(async {
                // Launch 10 identical queries concurrently
                let mut handles = vec![];
                for i in 0..10 {
                    let resolver_clone = Arc::clone(&resolver);
                    let query_clone = query.clone();
                    let handle =
                        tokio::spawn(
                            async move { resolver_clone.resolve(query_clone, 1234 + i).await },
                        );
                    handles.push(handle);
                }

                // Wait for all to complete
                for handle in handles {
                    let _ = handle.await;
                }
            })
        });
    });
}

fn benchmark_connection_pooling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("connection_pooling");

    let config = DnsConfig::default();
    let resolver = rt.block_on(DnsResolver::new(config)).unwrap();

    // Create different queries to avoid cache hits
    let queries: Vec<DNSPacket> = (0..100)
        .map(|i| create_query_packet(&format!("test{}.example.com", i)))
        .collect();

    group.bench_function("with_connection_pool", |b| {
        let mut query_idx = 0;
        b.iter(|| {
            rt.block_on(async {
                let query = &queries[query_idx % queries.len()];
                query_idx += 1;
                let response = resolver.resolve(black_box(query.clone()), 1234).await;
                let _ = black_box(response);
            })
        });
    });

    // For comparison, we'd need a version without connection pooling
    // This would require modifying the resolver or creating a mock

    group.finish();
}

fn benchmark_cache_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("cache_performance");

    // Config with caching enabled
    let cached_config = DnsConfig {
        enable_caching: true,
        max_cache_size: 10000,
        ..Default::default()
    };

    // Config with caching disabled
    let uncached_config = DnsConfig {
        enable_caching: false,
        ..Default::default()
    };

    let query = create_query_packet("cached.example.com");

    // Benchmark with cache
    group.bench_function("with_cache", |b| {
        let resolver = rt
            .block_on(DnsResolver::new(cached_config.clone()))
            .unwrap();

        // Prime the cache
        rt.block_on(async {
            let _ = resolver.resolve(query.clone(), 1234).await;
        });

        b.iter(|| {
            rt.block_on(async {
                let response = resolver.resolve(black_box(query.clone()), 1234).await;
                let _ = black_box(response);
            })
        });
    });

    // Benchmark without cache
    group.bench_function("without_cache", |b| {
        let resolver = rt
            .block_on(DnsResolver::new(uncached_config.clone()))
            .unwrap();
        b.iter(|| {
            rt.block_on(async {
                let response = resolver.resolve(black_box(query.clone()), 1234).await;
                let _ = black_box(response);
            })
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50) // Fewer samples due to network I/O
        .measurement_time(Duration::from_secs(10));
    targets =
        benchmark_parallel_vs_sequential,
        benchmark_query_deduplication,
        benchmark_connection_pooling,
        benchmark_cache_performance
}

criterion_main!(benches);
