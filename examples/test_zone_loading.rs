use heimdall::zone::{ZoneStore, ZoneParser};
use std::fs;
use std::time::Instant;

#[tokio::main]
async fn main() {
    println!("Testing zone loading...");
    
    // Create a temporary zone file
    let temp_dir = std::env::temp_dir();
    let zone_file_path = temp_dir.join("test_zone_loading.example.com.zone");
    
    let zone_content = r#"
$ORIGIN example.com.
$TTL 3600

@   IN  SOA ns1.example.com. admin.example.com. 2024010101 3600 900 604800 86400
@   IN  NS  ns1.example.com.
@   IN  A   192.0.2.1
www IN  A   192.0.2.2
"#;
    
    println!("Writing zone file to: {:?}", zone_file_path);
    fs::write(&zone_file_path, zone_content).unwrap();
    
    // Test synchronous loading
    println!("\nTesting synchronous zone loading...");
    let start = Instant::now();
    {
        let mut parser = ZoneParser::new();
        match parser.parse_file(&zone_file_path) {
            Ok(zone) => {
                println!("✓ Sync parse successful: {} records", zone.stats().total_records);
            }
            Err(e) => {
                println!("✗ Sync parse failed: {:?}", e);
            }
        }
    }
    println!("Sync loading took: {:?}", start.elapsed());
    
    // Test asynchronous loading
    println!("\nTesting asynchronous zone loading...");
    let start = Instant::now();
    {
        let mut parser = ZoneParser::new();
        match parser.parse_file_async(&zone_file_path).await {
            Ok(zone) => {
                println!("✓ Async parse successful: {} records", zone.stats().total_records);
            }
            Err(e) => {
                println!("✗ Async parse failed: {:?}", e);
            }
        }
    }
    println!("Async loading took: {:?}", start.elapsed());
    
    // Test zone store loading
    println!("\nTesting zone store loading...");
    let start = Instant::now();
    {
        let store = ZoneStore::new();
        match store.load_zone_file_async(&zone_file_path).await {
            Ok(origin) => {
                println!("✓ Zone store load successful: origin = {}", origin);
                
                // Test query
                match store.query("www.example.com", heimdall::dns::enums::DNSResourceType::A) {
                    heimdall::zone::QueryResult::Success { records, .. } => {
                        println!("✓ Query successful: {} records", records.len());
                    }
                    result => {
                        println!("✗ Query result: {:?}", result);
                    }
                }
            }
            Err(e) => {
                println!("✗ Zone store load failed: {:?}", e);
            }
        }
    }
    println!("Zone store loading took: {:?}", start.elapsed());
    
    // Clean up
    fs::remove_file(&zone_file_path).ok();
    println!("\nTest complete");
}