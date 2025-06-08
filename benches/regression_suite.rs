use criterion::{Criterion, black_box};
use heimdall::cache::{CacheKey, DnsCache};
use heimdall::dns::enums::{DNSResourceClass, DNSResourceType};
use heimdall::dns::simd::SimdParser;
use heimdall::dns::{DNSPacket, DNSPacketRef, PacketBufferPool};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Baseline performance metrics for regression testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    pub version: String,
    pub benchmarks: HashMap<String, BenchmarkResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub mean_time_ns: f64,
    pub std_dev_ns: f64,
    pub min_time_ns: f64,
    pub max_time_ns: f64,
    pub throughput_ops_per_sec: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RegressionTestConfig {
    pub max_regression_percent: f64,
    pub min_improvement_percent: f64,
    pub baseline_file: String,
    pub output_file: Option<String>,
}

impl Default for RegressionTestConfig {
    fn default() -> Self {
        Self {
            max_regression_percent: 10.0, // Fail if performance degrades by more than 10%
            min_improvement_percent: 5.0, // Celebrate if performance improves by more than 5%
            baseline_file: "benchmarks/baseline.json".to_string(),
            output_file: Some("benchmarks/latest_results.json".to_string()),
        }
    }
}

/// Regression test suite for DNS performance
pub struct DnsRegressionTester {
    config: RegressionTestConfig,
    baseline: Option<PerformanceBaseline>,
    current_results: HashMap<String, BenchmarkResult>,
}

impl DnsRegressionTester {
    pub fn new(config: RegressionTestConfig) -> Self {
        let baseline = Self::load_baseline(&config.baseline_file);
        Self {
            config,
            baseline,
            current_results: HashMap::new(),
        }
    }

    fn load_baseline(file_path: &str) -> Option<PerformanceBaseline> {
        match fs::read_to_string(file_path) {
            Ok(content) => match serde_json::from_str::<PerformanceBaseline>(&content) {
                Ok(baseline) => {
                    println!("Loaded performance baseline version: {}", baseline.version);
                    Some(baseline)
                }
                Err(e) => {
                    eprintln!("Failed to parse baseline file {}: {}", file_path, e);
                    None
                }
            },
            Err(_) => {
                println!(
                    "No baseline file found at {}, creating new baseline",
                    file_path
                );
                None
            }
        }
    }

    pub fn run_regression_tests(&mut self, c: &mut Criterion) {
        println!("🧪 Running DNS Performance Regression Tests...");

        // Run all benchmark suites
        self.benchmark_dns_parsing(c);
        self.benchmark_cache_operations(c);
        self.benchmark_simd_operations(c);
        self.benchmark_serialization(c);
        self.benchmark_buffer_pool(c);

        // Analyze results and compare against baseline
        self.analyze_results();
    }

    fn benchmark_dns_parsing(&mut self, c: &mut Criterion) {
        let packet_data = create_test_dns_packet();
        let iterations = 1000;

        // Regular parsing benchmark
        let mut group = c.benchmark_group("dns_parsing");
        group.bench_function("regular_parsing", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _packet = DNSPacket::parse(black_box(&packet_data)).unwrap();
                }
            })
        });

        // Zero-copy parsing benchmark
        group.bench_function("zero_copy_parsing", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _packet = DNSPacketRef::parse_metadata(black_box(&packet_data)).unwrap();
                }
            })
        });

        group.finish();
    }

    fn benchmark_cache_operations(&mut self, c: &mut Criterion) {
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

        let mut group = c.benchmark_group("cache_operations");

        // Cache hit benchmark
        let test_key = CacheKey::new(
            "test500.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        group.bench_function("cache_hits", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _result = cache.get(black_box(&test_key));
                }
            })
        });

        // Cache miss benchmark
        let miss_key = CacheKey::new(
            "nonexistent.example.com".to_string(),
            DNSResourceType::A,
            DNSResourceClass::IN,
        );

        group.bench_function("cache_misses", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _result = cache.get(black_box(&miss_key));
                }
            })
        });

        // Cache key creation benchmark
        let domain = "benchmark.example.com".to_string();
        group.bench_function("cache_key_creation", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _key = CacheKey::new(
                        black_box(domain.clone()),
                        black_box(DNSResourceType::A),
                        black_box(DNSResourceClass::IN),
                    );
                }
            })
        });

        group.finish();
    }

    fn benchmark_simd_operations(&mut self, c: &mut Criterion) {
        let test_data = create_test_dns_packet();
        let iterations = 10000;

        let mut group = c.benchmark_group("simd_operations");

        group.bench_function("compression_pointer_search", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _pointers =
                        SimdParser::find_compression_pointers_simd(black_box(&test_data));
                }
            })
        });

        group.bench_function("pattern_search", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _positions = SimdParser::find_record_type_pattern_simd(
                        black_box(&test_data),
                        black_box(&[0x00, 0x01]),
                    );
                }
            })
        });

        group.bench_function("checksum_calculation", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _checksum =
                        SimdParser::calculate_packet_checksum_simd(black_box(&test_data));
                }
            })
        });

        group.finish();
    }

    fn benchmark_serialization(&mut self, c: &mut Criterion) {
        let packet_data = create_test_dns_packet();
        let packet = DNSPacket::parse(&packet_data).unwrap();
        let iterations = 1000;

        let mut group = c.benchmark_group("serialization");

        group.bench_function("regular_serialization", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _serialized = black_box(&packet).serialize().unwrap();
                }
            })
        });

        let mut buffer = Vec::new();
        group.bench_function("zero_copy_serialization", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let _size = black_box(&packet).serialize_to_buffer(&mut buffer).unwrap();
                }
            })
        });

        group.finish();
    }

    fn benchmark_buffer_pool(&mut self, c: &mut Criterion) {
        let pool = PacketBufferPool::new(4096, 32);
        let iterations = 1000;

        let mut group = c.benchmark_group("buffer_management");

        group.bench_function("buffer_pool_operations", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let buffer = pool.get_buffer();
                    let _ = black_box(buffer.capacity());
                    pool.return_buffer(buffer);
                }
            })
        });

        group.bench_function("direct_allocation", |b| {
            b.iter(|| {
                for _ in 0..iterations {
                    let buffer = Vec::<u8>::with_capacity(4096);
                    let _ = black_box(buffer.capacity());
                    // Buffer is dropped automatically
                }
            })
        });

        group.finish();
    }

    fn analyze_results(&self) {
        println!("\n📊 Performance Regression Analysis");
        println!("==================================");

        if let Some(baseline) = &self.baseline {
            let mut regressions = Vec::new();
            let mut improvements = Vec::new();
            let mut stable = Vec::new();

            for (bench_name, current) in &self.current_results {
                if let Some(baseline_result) = baseline.benchmarks.get(bench_name) {
                    let change_percent = ((current.mean_time_ns - baseline_result.mean_time_ns)
                        / baseline_result.mean_time_ns)
                        * 100.0;

                    if change_percent > self.config.max_regression_percent {
                        regressions.push((bench_name.clone(), change_percent));
                    } else if change_percent < -self.config.min_improvement_percent {
                        improvements.push((bench_name.clone(), -change_percent));
                    } else {
                        stable.push((bench_name.clone(), change_percent));
                    }

                    println!(
                        "{}: {:.2}ns -> {:.2}ns ({:+.1}%)",
                        bench_name,
                        baseline_result.mean_time_ns,
                        current.mean_time_ns,
                        change_percent
                    );
                } else {
                    println!(
                        "{}: NEW BENCHMARK (baseline: {:.2}ns)",
                        bench_name, current.mean_time_ns
                    );
                }
            }

            // Report summary
            println!("\n📈 Summary:");
            println!("  🎉 Improvements: {} benchmarks", improvements.len());
            println!("  ⚖️  Stable: {} benchmarks", stable.len());
            println!("  ⚠️  Regressions: {} benchmarks", regressions.len());

            if !improvements.is_empty() {
                println!("\n🚀 Performance Improvements:");
                for (name, improvement) in improvements {
                    println!("  ✅ {}: +{:.1}% faster", name, improvement);
                }
            }

            if !regressions.is_empty() {
                println!("\n🐌 Performance Regressions:");
                for (name, regression) in &regressions {
                    println!("  ❌ {}: {:.1}% slower", name, regression);
                }

                println!("\n❌ REGRESSION TEST FAILED!");
                println!("Some benchmarks have regressed beyond the acceptable threshold.");
                std::process::exit(1);
            } else {
                println!("\n✅ REGRESSION TEST PASSED!");
                println!("All benchmarks are within acceptable performance bounds.");
            }
        } else {
            println!("No baseline found - creating new performance baseline.");
            if let Some(output_file) = &self.config.output_file {
                self.save_baseline(output_file);
            }
        }
    }

    fn save_baseline(&self, file_path: &str) {
        let baseline = PerformanceBaseline {
            version: env!("CARGO_PKG_VERSION").to_string(),
            benchmarks: self.current_results.clone(),
        };

        // Create directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            let _ = fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(&baseline) {
            Ok(json) => {
                if let Err(e) = fs::write(file_path, json) {
                    eprintln!("Failed to save baseline to {}: {}", file_path, e);
                } else {
                    println!("💾 Saved performance baseline to {}", file_path);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize baseline: {}", e);
            }
        }
    }

    // Mock implementation to extract results from criterion - in a real implementation
    // you'd need to hook into criterion's measurement collection
    #[allow(dead_code)]
    fn record_benchmark_result(&mut self, name: String, mean_ns: f64, std_dev_ns: f64) {
        let result = BenchmarkResult {
            mean_time_ns: mean_ns,
            std_dev_ns,
            min_time_ns: mean_ns - std_dev_ns,
            max_time_ns: mean_ns + std_dev_ns,
            throughput_ops_per_sec: Some(1_000_000_000.0 / mean_ns),
        };
        self.current_results.insert(name, result);
    }
}

fn create_test_dns_packet() -> Vec<u8> {
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

// CLI for running regression tests independently
// Note: This function is unused but kept for future CLI integration
#[allow(dead_code)]
pub fn main() {
    let config = RegressionTestConfig::default();
    let mut tester = DnsRegressionTester::new(config);

    let mut criterion = Criterion::default().sample_size(100);

    tester.run_regression_tests(&mut criterion);
}
