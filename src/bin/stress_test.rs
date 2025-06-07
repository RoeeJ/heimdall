use std::time::Duration;
use clap::{Arg, Command};
use heimdall::dns::enums::DNSResourceType;

// Import the stress testing module
#[path = "../../tests/stress_test.rs"]
mod stress_test;

use stress_test::{DNSStressTester, StressTestConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let matches = Command::new("Heimdall DNS Stress Tester")
        .version("1.0")
        .about("Stress test the Heimdall DNS server")
        .arg(Arg::new("clients")
            .short('c')
            .long("clients")
            .value_name("NUMBER")
            .help("Number of concurrent clients")
            .default_value("10"))
        .arg(Arg::new("queries")
            .short('q')
            .long("queries")
            .value_name("NUMBER")
            .help("Total number of queries to send")
            .default_value("1000"))
        .arg(Arg::new("server")
            .short('s')
            .long("server")
            .value_name("ADDRESS:PORT")
            .help("Target server address")
            .default_value("127.0.0.1:1053"))
        .arg(Arg::new("timeout")
            .short('t')
            .long("timeout")
            .value_name("SECONDS")
            .help("Query timeout in seconds")
            .default_value("5"))
        .arg(Arg::new("edns")
            .long("edns")
            .help("Enable EDNS in queries")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("buffer-size")
            .long("buffer-size")
            .value_name("BYTES")
            .help("EDNS buffer size to request")
            .default_value("1232"))
        .arg(Arg::new("scenario")
            .long("scenario")
            .value_name("NAME")
            .help("Predefined test scenario")
            .value_parser(["light", "medium", "heavy", "extreme", "endurance"]))
        .arg(Arg::new("query-types")
            .long("query-types")
            .value_name("TYPES")
            .help("Comma-separated list of query types to test")
            .default_value("A,AAAA,MX"))
        .get_matches();

    // Parse command line arguments
    let concurrent_clients: usize = matches.get_one::<String>("clients")
        .unwrap()
        .parse()
        .expect("Invalid number of clients");

    let total_queries: u64 = matches.get_one::<String>("queries")
        .unwrap()
        .parse()
        .expect("Invalid number of queries");

    let server_addr = matches.get_one::<String>("server")
        .unwrap()
        .clone();

    let query_timeout = Duration::from_secs(
        matches.get_one::<String>("timeout")
            .unwrap()
            .parse()
            .expect("Invalid timeout")
    );

    let enable_edns = matches.get_flag("edns");

    let edns_buffer_size: u16 = matches.get_one::<String>("buffer-size")
        .unwrap()
        .parse()
        .expect("Invalid buffer size");

    // Parse query types
    let query_types_str = matches.get_one::<String>("query-types").unwrap();
    let query_types: Vec<DNSResourceType> = query_types_str
        .split(',')
        .map(|s| match s.trim().to_uppercase().as_str() {
            "A" => DNSResourceType::A,
            "AAAA" => DNSResourceType::AAAA,
            "MX" => DNSResourceType::MX,
            "NS" => DNSResourceType::NS,
            "CNAME" => DNSResourceType::CNAME,
            "TXT" => DNSResourceType::TXT,
            "SOA" => DNSResourceType::SOA,
            _ => {
                eprintln!("Warning: Unknown query type '{}', using A instead", s);
                DNSResourceType::A
            }
        })
        .collect();

    // Create config based on scenario or custom parameters
    let config = if let Some(scenario) = matches.get_one::<String>("scenario") {
        create_scenario_config(scenario, &server_addr)
    } else {
        StressTestConfig {
            concurrent_clients,
            total_queries,
            server_addr,
            query_timeout,
            query_types,
            enable_edns,
            edns_buffer_size,
            test_domains: vec![
                "google.com".to_string(),
                "cloudflare.com".to_string(),
                "github.com".to_string(),
                "stackoverflow.com".to_string(),
                "rust-lang.org".to_string(),
                "example.com".to_string(),
                "wikipedia.org".to_string(),
                "mozilla.org".to_string(),
            ],
        }
    };

    println!("Starting DNS stress test with configuration:");
    println!("  Server: {}", config.server_addr);
    println!("  Concurrent clients: {}", config.concurrent_clients);
    println!("  Total queries: {}", config.total_queries);
    println!("  Query timeout: {:?}", config.query_timeout);
    println!("  EDNS enabled: {}", config.enable_edns);
    if config.enable_edns {
        println!("  EDNS buffer size: {}", config.edns_buffer_size);
    }
    println!("  Query types: {:?}", config.query_types);
    println!("  Test domains: {:?}", config.test_domains);
    println!("");

    // Run the stress test
    let mut tester = DNSStressTester::new(config)
        .with_resource_monitoring(true);
    tester.run().await;

    Ok(())
}

fn create_scenario_config(scenario: &str, server_addr: &str) -> StressTestConfig {
    let base_config = StressTestConfig {
        server_addr: server_addr.to_string(),
        enable_edns: true,
        edns_buffer_size: 1232,
        query_types: vec![DNSResourceType::A, DNSResourceType::AAAA, DNSResourceType::MX, DNSResourceType::NS],
        test_domains: vec![
            "google.com".to_string(),
            "cloudflare.com".to_string(),
            "github.com".to_string(),
            "stackoverflow.com".to_string(),
            "rust-lang.org".to_string(),
            "example.com".to_string(),
            "wikipedia.org".to_string(),
            "mozilla.org".to_string(),
            "amazon.com".to_string(),
            "microsoft.com".to_string(),
        ],
        ..Default::default()
    };

    match scenario {
        "light" => StressTestConfig {
            concurrent_clients: 5,
            total_queries: 100,
            query_timeout: Duration::from_secs(2),
            ..base_config
        },
        "medium" => StressTestConfig {
            concurrent_clients: 20,
            total_queries: 1000,
            query_timeout: Duration::from_secs(3),
            ..base_config
        },
        "heavy" => StressTestConfig {
            concurrent_clients: 50,
            total_queries: 5000,
            query_timeout: Duration::from_secs(5),
            ..base_config
        },
        "extreme" => StressTestConfig {
            concurrent_clients: 100,
            total_queries: 10000,
            query_timeout: Duration::from_secs(10),
            ..base_config
        },
        "endurance" => StressTestConfig {
            concurrent_clients: 25,
            total_queries: 50000,
            query_timeout: Duration::from_secs(30),
            test_domains: vec![
                "google.com".to_string(),
                "cloudflare.com".to_string(),
                "github.com".to_string(),
                "stackoverflow.com".to_string(),
                "rust-lang.org".to_string(),
                "example.com".to_string(),
                "wikipedia.org".to_string(),
                "mozilla.org".to_string(),
                "amazon.com".to_string(),
                "microsoft.com".to_string(),
                "apple.com".to_string(),
                "facebook.com".to_string(),
                "twitter.com".to_string(),
                "linkedin.com".to_string(),
                "reddit.com".to_string(),
            ],
            ..base_config
        },
        _ => {
            eprintln!("Unknown scenario '{}', using default config", scenario);
            base_config
        }
    }
}