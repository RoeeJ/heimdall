use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use heimdall::blocking::{DnsBlocker, BlockingMode};
use heimdall::blocking::blocker_v2::DnsBlockerV2;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn create_test_domains() -> Vec<String> {
    let mut domains = Vec::new();
    
    // Mix of blocked and non-blocked domains
    for i in 0..1000 {
        domains.push(format!("test{}.example.com", i));
        domains.push(format!("ads{}.tracking.com", i));
        domains.push(format!("safe{}.website.org", i));
    }
    
    domains
}

fn bench_original_blocker(c: &mut Criterion) {
    let mut group = c.benchmark_group("original_blocker");
    
    // Setup blocker with test data
    let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);
    
    // Add some blocked domains
    for i in 0..10000 {
        blocker.add_blocked_domain(&format!("blocked{}.example.com", i));
    }
    
    let test_domains = create_test_domains();
    
    group.bench_function("lookup", |b| {
        b.iter(|| {
            for domain in &test_domains {
                black_box(blocker.is_blocked(domain));
            }
        });
    });
    
    group.bench_function("single_lookup", |b| {
        b.iter(|| {
            black_box(blocker.is_blocked("test123.example.com"));
        });
    });
    
    group.finish();
}

fn bench_v2_blocker(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("v2_blocker");
    
    // Setup blocker
    let blocker = rt.block_on(async {
        DnsBlockerV2::new(BlockingMode::NxDomain, true).await.unwrap()
    });
    
    let test_domains = create_test_domains();
    
    group.bench_function("lookup", |b| {
        b.iter(|| {
            for domain in &test_domains {
                black_box(blocker.is_blocked(domain));
            }
        });
    });
    
    group.bench_function("single_lookup", |b| {
        b.iter(|| {
            black_box(blocker.is_blocked("test123.example.com"));
        });
    });
    
    group.finish();
}

fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    // Test with different numbers of domains
    for size in [1000, 10000, 100000].iter() {
        group.bench_with_input(
            BenchmarkId::new("original", size),
            size,
            |b, &size| {
                b.iter_custom(|iters| {
                    let mut total_time = std::time::Duration::new(0, 0);
                    
                    for _ in 0..iters {
                        let start = std::time::Instant::now();
                        
                        let blocker = DnsBlocker::new(BlockingMode::NxDomain, true);
                        for i in 0..size {
                            blocker.add_blocked_domain(&format!("domain{}.example.com", i));
                        }
                        
                        total_time += start.elapsed();
                        
                        // Force drop to measure allocation/deallocation
                        drop(blocker);
                    }
                    
                    total_time
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, bench_original_blocker, bench_v2_blocker, bench_memory_usage);
criterion_main!(benches);