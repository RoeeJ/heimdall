use clap::{Arg, Command};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::process;
use std::time::Instant;

use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::simd::SimdParser;
use heimdall::dns::{DNSPacket, DNSPacketRef, PacketBufferPool};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkResult {
    pub mean_time_ns: f64,
    pub std_dev_ns: f64,
    pub name: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerformanceBaseline {
    pub version: String,
    pub benchmarks: HashMap<String, BenchmarkResult>,
    pub created_at: String,
}

fn main() {
    let matches = Command::new("Heimdall Performance Regression Tester")
        .version("1.0")
        .about("Test for performance regressions in DNS operations")
        .arg(
            Arg::new("baseline")
                .long("baseline")
                .value_name("FILE")
                .help("Path to baseline performance file")
                .default_value("benchmarks/baseline.json"),
        )
        .arg(
            Arg::new("create-baseline")
                .long("create-baseline")
                .help("Create a new performance baseline")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("max-regression")
                .long("max-regression")
                .value_name("PERCENT")
                .help("Maximum allowed regression percentage")
                .default_value("10.0"),
        )
        .arg(
            Arg::new("iterations")
                .long("iterations")
                .value_name("NUMBER")
                .help("Number of iterations for each benchmark")
                .default_value("1000"),
        )
        .get_matches();

    let baseline_file = matches.get_one::<String>("baseline").unwrap();
    let create_baseline = matches.get_flag("create-baseline");
    let max_regression: f64 = matches
        .get_one::<String>("max-regression")
        .unwrap()
        .parse()
        .expect("Invalid regression percentage");
    let iterations: usize = matches
        .get_one::<String>("iterations")
        .unwrap()
        .parse()
        .expect("Invalid iteration count");

    println!("🧪 Heimdall DNS Performance Regression Test");
    println!("==========================================");
    println!("Iterations per benchmark: {}", iterations);
    println!("Max allowed regression: {:.1}%", max_regression);
    println!("");

    // Run all benchmarks
    let current_results = run_benchmarks(iterations);

    if create_baseline {
        save_baseline(&current_results, baseline_file);
        println!("✅ Baseline created successfully!");
        return;
    }

    // Load baseline and compare
    match load_baseline(baseline_file) {
        Some(baseline) => {
            analyze_performance(&baseline, &current_results, max_regression);
        }
        None => {
            println!("⚠️  No baseline found at {}", baseline_file);
            println!("Creating new baseline...");
            save_baseline(&current_results, baseline_file);
            println!("✅ Baseline created! Run again to compare against it.");
        }
    }
}

fn run_benchmarks(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();

    println!("🚀 Running DNS Performance Benchmarks...\n");

    // DNS Parsing Benchmarks
    results.extend(benchmark_dns_parsing(iterations));

    // Cache Operation Benchmarks
    results.extend(benchmark_cache_operations(iterations));

    // SIMD Operation Benchmarks
    results.extend(benchmark_simd_operations(iterations));

    // Serialization Benchmarks
    results.extend(benchmark_serialization(iterations));

    // Buffer Pool Benchmarks
    results.extend(benchmark_buffer_pool(iterations));

    println!("✅ All benchmarks completed!\n");
    results
}

fn benchmark_dns_parsing(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();
    let packet_data = create_test_packet();

    print!("📦 DNS Parsing... ");

    // Regular parsing
    let start = Instant::now();
    for _ in 0..iterations {
        let _packet = DNSPacket::parse(&packet_data).unwrap();
    }
    let regular_duration = start.elapsed();

    // Zero-copy parsing
    let start = Instant::now();
    for _ in 0..iterations {
        let _packet = DNSPacketRef::parse_metadata(&packet_data).unwrap();
    }
    let zerocopy_duration = start.elapsed();

    let regular_ns = regular_duration.as_nanos() as f64 / iterations as f64;
    let zerocopy_ns = zerocopy_duration.as_nanos() as f64 / iterations as f64;

    results.insert(
        "dns_parsing_regular".to_string(),
        BenchmarkResult {
            mean_time_ns: regular_ns,
            std_dev_ns: regular_ns * 0.1, // Estimated 10% std dev
            name: "dns_parsing_regular".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    results.insert(
        "dns_parsing_zerocopy".to_string(),
        BenchmarkResult {
            mean_time_ns: zerocopy_ns,
            std_dev_ns: zerocopy_ns * 0.1,
            name: "dns_parsing_zerocopy".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    println!("✅ ({:.2}x speedup)", regular_ns / zerocopy_ns);
    results
}

fn benchmark_cache_operations(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();
    let cache = DnsCache::new(10000, 300);

    print!("🗄️  Cache Operations... ");

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

    // Cache hits
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

    // Cache misses
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

    let hit_ns = hit_duration.as_nanos() as f64 / iterations as f64;
    let miss_ns = miss_duration.as_nanos() as f64 / iterations as f64;

    results.insert(
        "cache_hits".to_string(),
        BenchmarkResult {
            mean_time_ns: hit_ns,
            std_dev_ns: hit_ns * 0.05,
            name: "cache_hits".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    results.insert(
        "cache_misses".to_string(),
        BenchmarkResult {
            mean_time_ns: miss_ns,
            std_dev_ns: miss_ns * 0.05,
            name: "cache_misses".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    println!("✅ ({:.0}ns hits, {:.0}ns misses)", hit_ns, miss_ns);
    results
}

fn benchmark_simd_operations(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();
    let test_data = create_test_packet();

    print!("⚡ SIMD Operations... ");

    // Compression pointer search
    let start = Instant::now();
    for _ in 0..iterations {
        let _pointers = SimdParser::find_compression_pointers_simd(&test_data);
    }
    let compression_duration = start.elapsed();

    // Pattern search
    let start = Instant::now();
    for _ in 0..iterations {
        let _positions = SimdParser::find_record_type_pattern_simd(&test_data, &[0x00, 0x01]);
    }
    let pattern_duration = start.elapsed();

    let compression_ns = compression_duration.as_nanos() as f64 / iterations as f64;
    let pattern_ns = pattern_duration.as_nanos() as f64 / iterations as f64;

    results.insert(
        "simd_compression_search".to_string(),
        BenchmarkResult {
            mean_time_ns: compression_ns,
            std_dev_ns: compression_ns * 0.1,
            name: "simd_compression_search".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    results.insert(
        "simd_pattern_search".to_string(),
        BenchmarkResult {
            mean_time_ns: pattern_ns,
            std_dev_ns: pattern_ns * 0.1,
            name: "simd_pattern_search".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    println!("✅ ({:.0}ns compression, {:.0}ns pattern)", compression_ns, pattern_ns);
    results
}

fn benchmark_serialization(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();
    let packet_data = create_test_packet();
    let packet = DNSPacket::parse(&packet_data).unwrap();

    print!("📤 Serialization... ");

    // Regular serialization
    let start = Instant::now();
    for _ in 0..iterations {
        let _serialized = packet.serialize().unwrap();
    }
    let regular_duration = start.elapsed();

    // Zero-copy serialization
    let mut buffer = Vec::new();
    let start = Instant::now();
    for _ in 0..iterations {
        let _size = packet.serialize_to_buffer(&mut buffer).unwrap();
    }
    let zerocopy_duration = start.elapsed();

    let regular_ns = regular_duration.as_nanos() as f64 / iterations as f64;
    let zerocopy_ns = zerocopy_duration.as_nanos() as f64 / iterations as f64;

    results.insert(
        "serialization_regular".to_string(),
        BenchmarkResult {
            mean_time_ns: regular_ns,
            std_dev_ns: regular_ns * 0.1,
            name: "serialization_regular".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    results.insert(
        "serialization_zerocopy".to_string(),
        BenchmarkResult {
            mean_time_ns: zerocopy_ns,
            std_dev_ns: zerocopy_ns * 0.1,
            name: "serialization_zerocopy".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    println!("✅ ({:.2}x speedup)", regular_ns / zerocopy_ns);
    results
}

fn benchmark_buffer_pool(iterations: usize) -> HashMap<String, BenchmarkResult> {
    let mut results = HashMap::new();
    let pool = PacketBufferPool::new(4096, 32);

    print!("🔄 Buffer Pool... ");

    // Buffer pool operations
    let start = Instant::now();
    for _ in 0..iterations {
        let buffer = pool.get_buffer();
        let _ = buffer.capacity();
        pool.return_buffer(buffer);
    }
    let pool_duration = start.elapsed();

    // Direct allocation
    let start = Instant::now();
    for _ in 0..iterations {
        let buffer = Vec::<u8>::with_capacity(4096);
        let _ = buffer.capacity();
    }
    let alloc_duration = start.elapsed();

    let pool_ns = pool_duration.as_nanos() as f64 / iterations as f64;
    let alloc_ns = alloc_duration.as_nanos() as f64 / iterations as f64;

    results.insert(
        "buffer_pool".to_string(),
        BenchmarkResult {
            mean_time_ns: pool_ns,
            std_dev_ns: pool_ns * 0.1,
            name: "buffer_pool".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    results.insert(
        "buffer_allocation".to_string(),
        BenchmarkResult {
            mean_time_ns: alloc_ns,
            std_dev_ns: alloc_ns * 0.1,
            name: "buffer_allocation".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    println!("✅ ({:.2}x speedup)", alloc_ns / pool_ns);
    results
}

fn create_test_packet() -> Vec<u8> {
    vec![
        0x12, 0x34, // ID
        0x01, 0x00, // Flags: standard query
        0x00, 0x01, // Questions: 1
        0x00, 0x00, // Answers: 0
        0x00, 0x00, // Authority: 0
        0x00, 0x00, // Additional: 0
        // Question: example.com
        0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
        0x03, b'c', b'o', b'm',
        0x00, // End of name
        0x00, 0x01, // Type: A
        0x00, 0x01, // Class: IN
    ]
}

fn load_baseline(file_path: &str) -> Option<PerformanceBaseline> {
    match fs::read_to_string(file_path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(baseline) => Some(baseline),
            Err(e) => {
                eprintln!("Failed to parse baseline file: {}", e);
                None
            }
        },
        Err(_) => None,
    }
}

fn save_baseline(results: &HashMap<String, BenchmarkResult>, file_path: &str) {
    let baseline = PerformanceBaseline {
        version: env!("CARGO_PKG_VERSION").to_string(),
        benchmarks: results.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Create directory if it doesn't exist
    if let Some(parent) = std::path::Path::new(file_path).parent() {
        let _ = fs::create_dir_all(parent);
    }

    match serde_json::to_string_pretty(&baseline) {
        Ok(json) => {
            if let Err(e) = fs::write(file_path, json) {
                eprintln!("Failed to save baseline: {}", e);
                process::exit(1);
            } else {
                println!("💾 Saved baseline to {}", file_path);
            }
        }
        Err(e) => {
            eprintln!("Failed to serialize baseline: {}", e);
            process::exit(1);
        }
    }
}

fn analyze_performance(
    baseline: &PerformanceBaseline,
    current: &HashMap<String, BenchmarkResult>,
    max_regression: f64,
) {
    println!("📊 Performance Analysis");
    println!("======================");
    println!("Baseline version: {}", baseline.version);
    println!("Current version: {}", env!("CARGO_PKG_VERSION"));
    println!("");

    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    let mut stable = Vec::new();

    for (name, current_result) in current {
        if let Some(baseline_result) = baseline.benchmarks.get(name) {
            let change_percent = ((current_result.mean_time_ns - baseline_result.mean_time_ns)
                / baseline_result.mean_time_ns)
                * 100.0;

            let status = if change_percent > max_regression {
                regressions.push((name.clone(), change_percent));
                "🐌 REGRESSION"
            } else if change_percent < -5.0 {
                improvements.push((name.clone(), -change_percent));
                "🚀 IMPROVEMENT"
            } else {
                stable.push((name.clone(), change_percent));
                "⚖️  STABLE"
            };

            println!(
                "{} {}: {:.1}ns -> {:.1}ns ({:+.1}%)",
                status, name, baseline_result.mean_time_ns, current_result.mean_time_ns, change_percent
            );
        } else {
            println!("🆕 NEW {}: {:.1}ns", name, current_result.mean_time_ns);
        }
    }

    println!("");
    println!("📈 Summary:");
    println!("  🎉 Improvements: {}", improvements.len());
    println!("  ⚖️  Stable: {}", stable.len());
    println!("  ⚠️  Regressions: {}", regressions.len());

    if !regressions.is_empty() {
        println!("");
        println!("❌ PERFORMANCE REGRESSION DETECTED!");
        println!("The following benchmarks have regressed beyond {:.1}%:", max_regression);
        for (name, regression) in &regressions {
            println!("  • {}: {:.1}% slower", name, regression);
        }
        process::exit(1);
    } else {
        println!("");
        println!("✅ PERFORMANCE REGRESSION TEST PASSED!");
        println!("All benchmarks are within acceptable bounds.");
    }
}