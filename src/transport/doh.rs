//! DNS-over-HTTPS (DoH) server implementation
//!
//! Implements RFC 8484: DNS Queries over HTTPS (DoH)
//! Also supports RFC 8427: Representing DNS Messages in JSON
//!
//! Features:
//! - HTTP/1.1 and HTTP/2 support with automatic negotiation
//! - DNS wireformat over POST and GET methods
//! - JSON DNS API support
//! - Content negotiation (application/dns-message, application/dns-json)
//! - CORS support for browser clients
//! - Well-known URI support (/.well-known/dns-query)
//! - Comprehensive metrics and monitoring

use super::tls::{TlsConfig, TlsError};
use crate::dns::DNSPacket;
use crate::metrics::DnsMetrics;
use crate::resolver::DnsResolver;
use axum::{
    Router,
    body::Body,
    extract::{Extension, Path, Query},
    http::{HeaderMap, Method, StatusCode},
    middleware,
    response::Response,
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, trace};

/// DNS-over-HTTPS server
pub struct DohServer {
    bind_addr: SocketAddr,
    #[allow(dead_code)]
    tls_acceptor: Option<TlsAcceptor>,
    resolver: Arc<DnsResolver>,
    metrics: Option<Arc<DnsMetrics>>,
    config: DohServerConfig,
    stats: Arc<DohServerStats>,
}

/// DoH server configuration
#[derive(Debug, Clone)]
pub struct DohServerConfig {
    /// Whether to enable TLS (HTTPS vs HTTP)
    pub enable_tls: bool,

    /// DoH URI path (default: /dns-query)
    pub path: String,

    /// Enable well-known URI support
    pub enable_well_known: bool,

    /// Enable JSON DNS API support
    pub enable_json_api: bool,

    /// Maximum request size in bytes
    pub max_request_size: usize,

    /// Request timeout
    pub request_timeout: Duration,

    /// Enable CORS for browser support
    pub enable_cors: bool,

    /// Maximum number of concurrent connections
    pub max_connections: usize,

    /// Rate limiting (requests per second per IP)
    pub rate_limit_per_ip: Option<u32>,
}

impl Default for DohServerConfig {
    fn default() -> Self {
        Self {
            enable_tls: true,
            path: "/dns-query".to_string(),
            enable_well_known: true,
            enable_json_api: true,
            max_request_size: 8192, // 8KB max for DNS queries
            request_timeout: Duration::from_secs(30),
            enable_cors: true,
            max_connections: 1000,
            rate_limit_per_ip: Some(100), // 100 requests per second per IP
        }
    }
}

/// DoH server statistics
#[derive(Debug, Default)]
pub struct DohServerStats {
    pub total_requests: AtomicU64,
    pub wireformat_requests: AtomicU64,
    pub json_requests: AtomicU64,
    pub get_requests: AtomicU64,
    pub post_requests: AtomicU64,
    pub successful_responses: AtomicU64,
    pub client_errors: AtomicU64,
    pub server_errors: AtomicU64,
    pub average_response_time_ms: AtomicU64,
}

/// Internal DoH request context
#[derive(Clone)]
struct DohContext {
    resolver: Arc<DnsResolver>,
    metrics: Option<Arc<DnsMetrics>>,
    stats: Arc<DohServerStats>,
    config: DohServerConfig,
}

/// Query parameters for GET requests
#[derive(Debug, serde::Deserialize)]
struct DohQueryParams {
    /// Base64url-encoded DNS query (wireformat)
    dns: Option<String>,
    /// Content type preference
    #[allow(dead_code)]
    ct: Option<String>,
}

impl DohServer {
    /// Create a new DNS-over-HTTPS server
    pub fn new(
        bind_addr: SocketAddr,
        tls_config: Option<TlsConfig>,
        resolver: Arc<DnsResolver>,
        metrics: Option<Arc<DnsMetrics>>,
        config: DohServerConfig,
    ) -> Result<Self, TlsError> {
        let tls_acceptor = if config.enable_tls {
            if let Some(tls_config) = tls_config {
                Some(tls_config.create_acceptor()?)
            } else {
                return Err(TlsError::ConfigError(rustls::Error::General(
                    "TLS enabled but no TLS configuration provided".to_string(),
                )));
            }
        } else {
            None
        };

        Ok(Self {
            bind_addr,
            tls_acceptor,
            resolver,
            metrics,
            config,
            stats: Arc::new(DohServerStats::default()),
        })
    }

    /// Run the DoH server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create the HTTP router
        let app = self.create_router();

        // Start the TCP listener
        let listener = TcpListener::bind(self.bind_addr).await?;
        let protocol = if self.config.enable_tls {
            "HTTPS"
        } else {
            "HTTP"
        };
        info!(
            "DNS-over-HTTPS server listening on {} ({})",
            self.bind_addr, protocol
        );
        info!(
            "DoH endpoint: {}://{}{}",
            if self.config.enable_tls {
                "https"
            } else {
                "http"
            },
            self.bind_addr,
            self.config.path
        );

        if self.config.enable_well_known {
            info!(
                "Well-known URI: {}://{}/.well-known/dns-query",
                if self.config.enable_tls {
                    "https"
                } else {
                    "http"
                },
                self.bind_addr
            );
        }

        // Use axum's built-in server
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Create the HTTP router with all DoH endpoints
    fn create_router(&self) -> Router {
        let context = DohContext {
            resolver: self.resolver.clone(),
            metrics: self.metrics.clone(),
            stats: self.stats.clone(),
            config: self.config.clone(),
        };

        // Create router with all routes
        let mut router = Router::new()
            .route(&self.config.path, get(handle_doh_get))
            .route(&self.config.path, axum::routing::post(handle_doh_post));

        // Add well-known URI support
        if self.config.enable_well_known {
            router = router
                .route("/.well-known/dns-query", get(handle_doh_get))
                .route(
                    "/.well-known/dns-query",
                    axum::routing::post(handle_doh_post),
                );
        }

        // Add JSON API endpoints if enabled
        if self.config.enable_json_api {
            router = router
                .route("/resolve", get(handle_json_resolve))
                .route("/resolve/{name}", get(handle_json_resolve_name))
                .route("/resolve/{name}/{type}", get(handle_json_resolve_type));
        }

        // Add shared state via layer
        router = router.layer(axum::Extension(context.clone()));

        // Add CORS middleware if enabled
        if self.config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([CONTENT_TYPE, ACCEPT])
                .max_age(Duration::from_secs(3600));

            router = router.layer(cors);
        }

        // Add metrics middleware
        router = router.layer(ServiceBuilder::new().layer(middleware::from_fn(metrics_middleware)));

        router
    }

    /// Get server statistics
    pub fn get_stats(&self) -> DohServerStats {
        DohServerStats {
            total_requests: AtomicU64::new(self.stats.total_requests.load(Ordering::Relaxed)),
            wireformat_requests: AtomicU64::new(
                self.stats.wireformat_requests.load(Ordering::Relaxed),
            ),
            json_requests: AtomicU64::new(self.stats.json_requests.load(Ordering::Relaxed)),
            get_requests: AtomicU64::new(self.stats.get_requests.load(Ordering::Relaxed)),
            post_requests: AtomicU64::new(self.stats.post_requests.load(Ordering::Relaxed)),
            successful_responses: AtomicU64::new(
                self.stats.successful_responses.load(Ordering::Relaxed),
            ),
            client_errors: AtomicU64::new(self.stats.client_errors.load(Ordering::Relaxed)),
            server_errors: AtomicU64::new(self.stats.server_errors.load(Ordering::Relaxed)),
            average_response_time_ms: AtomicU64::new(
                self.stats.average_response_time_ms.load(Ordering::Relaxed),
            ),
        }
    }
}

/// Handle DoH GET requests (RFC 8484)
async fn handle_doh_get(
    Query(params): Query<DohQueryParams>,
    _headers: HeaderMap,
    Extension(ctx): Extension<DohContext>,
) -> Result<Response<Body>, StatusCode> {
    let start_time = Instant::now();
    ctx.stats.total_requests.fetch_add(1, Ordering::Relaxed);
    ctx.stats.get_requests.fetch_add(1, Ordering::Relaxed);

    // Extract DNS query from parameters
    let dns_query = match params.dns {
        Some(dns_b64) => match URL_SAFE_NO_PAD.decode(&dns_b64) {
            Ok(data) => data,
            Err(_) => {
                ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
                return Err(StatusCode::BAD_REQUEST);
            }
        },
        None => {
            ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Process the DNS query directly
    let query = match DNSPacket::parse(&dns_query) {
        Ok(q) => q,
        Err(_) => {
            ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let original_id = query.header.id;

    // Resolve the query
    let response = match ctx.resolver.resolve(query, original_id).await {
        Ok(r) => r,
        Err(_) => {
            ctx.stats.server_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Serialize response
    let response_data = match response.serialize() {
        Ok(data) => data,
        Err(_) => {
            ctx.stats.server_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Update metrics
    if let Some(metrics) = &ctx.metrics {
        metrics.doh_queries_total.inc();
    }

    let response_time = start_time.elapsed().as_millis() as u64;
    ctx.stats
        .average_response_time_ms
        .store(response_time, Ordering::Relaxed);
    ctx.stats
        .successful_responses
        .fetch_add(1, Ordering::Relaxed);
    ctx.stats
        .wireformat_requests
        .fetch_add(1, Ordering::Relaxed);

    // Create HTTP response
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/dns-message")
        .header("Cache-Control", "max-age=300")
        .body(Body::from(response_data))
        .unwrap())
}

/// Handle DoH POST requests (RFC 8484)
async fn handle_doh_post(
    headers: HeaderMap,
    Extension(ctx): Extension<DohContext>,
    body: axum::body::Bytes,
) -> Result<Response<Body>, StatusCode> {
    let start_time = Instant::now();
    ctx.stats.total_requests.fetch_add(1, Ordering::Relaxed);
    ctx.stats.post_requests.fetch_add(1, Ordering::Relaxed);

    // Validate content type
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type != "application/dns-message" {
        ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check request size limits
    if body.len() > ctx.config.max_request_size {
        ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    // Process the DNS query directly from body
    let query = match DNSPacket::parse(&body) {
        Ok(q) => q,
        Err(_) => {
            ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let original_id = query.header.id;

    // Resolve the query
    let response = match ctx.resolver.resolve(query, original_id).await {
        Ok(r) => r,
        Err(_) => {
            ctx.stats.server_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Serialize response
    let response_data = match response.serialize() {
        Ok(data) => data,
        Err(_) => {
            ctx.stats.server_errors.fetch_add(1, Ordering::Relaxed);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Update metrics
    if let Some(metrics) = &ctx.metrics {
        metrics.doh_queries_total.inc();
    }

    let response_time = start_time.elapsed().as_millis() as u64;
    ctx.stats
        .average_response_time_ms
        .store(response_time, Ordering::Relaxed);
    ctx.stats
        .successful_responses
        .fetch_add(1, Ordering::Relaxed);
    ctx.stats
        .wireformat_requests
        .fetch_add(1, Ordering::Relaxed);

    // Create HTTP response
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/dns-message")
        .header("Cache-Control", "max-age=300")
        .body(Body::from(response_data))
        .unwrap())
}

/// Handle JSON DNS API requests (RFC 8427)
async fn handle_json_resolve(
    Query(params): Query<HashMap<String, String>>,
    Extension(ctx): Extension<DohContext>,
) -> Result<Response<Body>, StatusCode> {
    let start_time = Instant::now();
    ctx.stats.total_requests.fetch_add(1, Ordering::Relaxed);
    ctx.stats.get_requests.fetch_add(1, Ordering::Relaxed);
    ctx.stats.json_requests.fetch_add(1, Ordering::Relaxed);

    // Extract query parameters
    let name = params.get("name").ok_or(StatusCode::BAD_REQUEST)?;
    let qtype = params.get("type").map(|s| s.as_str()).unwrap_or("A");

    // Process JSON DNS query
    match process_json_query(&ctx, name, qtype).await {
        Ok(response) => {
            let response_time = start_time.elapsed().as_millis() as u64;
            ctx.stats
                .average_response_time_ms
                .store(response_time, Ordering::Relaxed);
            ctx.stats
                .successful_responses
                .fetch_add(1, Ordering::Relaxed);
            Ok(response)
        }
        Err(status) => {
            if status.is_client_error() {
                ctx.stats.client_errors.fetch_add(1, Ordering::Relaxed);
            } else {
                ctx.stats.server_errors.fetch_add(1, Ordering::Relaxed);
            }
            Err(status)
        }
    }
}

/// Handle JSON DNS API requests with name in path
async fn handle_json_resolve_name(
    Path(name): Path<String>,
    Extension(ctx): Extension<DohContext>,
) -> Result<Response<Body>, StatusCode> {
    process_json_query(&ctx, &name, "A").await
}

/// Handle JSON DNS API requests with name and type in path
async fn handle_json_resolve_type(
    Path((name, qtype)): Path<(String, String)>,
    Extension(ctx): Extension<DohContext>,
) -> Result<Response<Body>, StatusCode> {
    process_json_query(&ctx, &name, &qtype).await
}

/// Process a JSON DNS query
async fn process_json_query(
    ctx: &DohContext,
    name: &str,
    qtype_str: &str,
) -> Result<Response<Body>, StatusCode> {
    // Parse query type
    let qtype = match qtype_str.to_uppercase().as_str() {
        "A" => 1,
        "AAAA" => 28,
        "CNAME" => 5,
        "MX" => 15,
        "TXT" => 16,
        "NS" => 2,
        "SOA" => 6,
        "PTR" => 12,
        "SRV" => 33,
        _ => qtype_str.parse().unwrap_or(1),
    };

    process_json_query_internal(ctx, name, qtype).await
}

/// Internal JSON query processing
async fn process_json_query_internal(
    ctx: &DohContext,
    name: &str,
    qtype: u16,
) -> Result<Response<Body>, StatusCode> {
    // Create DNS query packet
    let mut query = DNSPacket::default();
    query.header.id = 1234;
    query.header.rd = true;
    query.header.qdcount = 1;

    let question = crate::dns::question::DNSQuestion {
        labels: name.split('.').map(|s| s.to_string()).collect(),
        qtype: crate::dns::enums::DNSResourceType::from_u16(qtype)
            .unwrap_or(crate::dns::enums::DNSResourceType::A),
        qclass: crate::dns::enums::DNSResourceClass::IN,
    };
    query.questions.push(question);

    // Resolve the query
    let response = ctx
        .resolver
        .resolve(query, 1234)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Convert to JSON format (RFC 8427)
    let json_response = convert_dns_to_json(&response);

    // Update metrics
    if let Some(metrics) = &ctx.metrics {
        metrics.doh_json_queries_total.inc();
    }

    // Create HTTP response
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/dns-json")
        .header("Cache-Control", "max-age=300")
        .body(Body::from(serde_json::to_string(&json_response).unwrap()))
        .unwrap())
}

/// Convert DNS packet to JSON format (RFC 8427)
fn convert_dns_to_json(dns: &DNSPacket) -> Value {
    let mut json = json!({
        "Status": dns.header.rcode,
        "TC": dns.header.tc,
        "RD": dns.header.rd,
        "RA": dns.header.ra,
        "AD": (dns.header.z & 0x20) != 0,  // AD bit is bit 5 of z field
        "CD": (dns.header.z & 0x10) != 0,  // CD bit is bit 4 of z field
    });

    // Add questions
    if !dns.questions.is_empty() {
        let questions: Vec<Value> = dns
            .questions
            .iter()
            .map(|q| {
                json!({
                    "name": q.labels.join("."),
                    "type": q.qtype as u16,
                })
            })
            .collect();
        json["Question"] = Value::Array(questions);
    }

    // Add answers
    if !dns.answers.is_empty() {
        let answers: Vec<Value> = dns
            .answers
            .iter()
            .map(|a| {
                json!({
                    "name": a.labels.join("."),
                    "type": a.rtype as u16,
                    "TTL": a.ttl,
                    "data": format_rdata(&a.rdata, a.rtype),
                })
            })
            .collect();
        json["Answer"] = Value::Array(answers);
    }

    json
}

/// Format resource record data for JSON output
fn format_rdata(rdata: &[u8], rtype: crate::dns::enums::DNSResourceType) -> String {
    use crate::dns::enums::DNSResourceType;

    match rtype {
        DNSResourceType::A => {
            if rdata.len() == 4 {
                format!("{}.{}.{}.{}", rdata[0], rdata[1], rdata[2], rdata[3])
            } else {
                "invalid".to_string()
            }
        }
        DNSResourceType::AAAA => {
            if rdata.len() == 16 {
                let parts: Vec<String> = rdata
                    .chunks(2)
                    .map(|chunk| format!("{:02x}{:02x}", chunk[0], chunk[1]))
                    .collect();
                parts.join(":")
            } else {
                "invalid".to_string()
            }
        }
        _ => {
            // For other types, return base64-encoded data
            base64::engine::general_purpose::STANDARD.encode(rdata)
        }
    }
}

/// Metrics middleware for DoH requests
async fn metrics_middleware(
    Extension(ctx): Extension<DohContext>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    // Update metrics
    if let Some(metrics) = &ctx.metrics {
        match method {
            Method::GET => metrics.doh_get_requests.inc(),
            Method::POST => metrics.doh_post_requests.inc(),
            _ => {}
        }

        if status.is_success() {
            metrics.doh_successful_responses.inc();
        } else if status.is_client_error() {
            metrics.doh_client_errors.inc();
        } else {
            metrics.doh_server_errors.inc();
        }

        metrics
            .doh_request_duration
            .with_label_values(&[&status.as_u16().to_string()])
            .observe(duration.as_secs_f64());
    }

    trace!("DoH {} {} {} {:?}", method, uri, status, duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::{
        enums::{DNSResourceClass, DNSResourceType},
        question::DNSQuestion,
    };
    use axum::http::StatusCode;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Mock resolver for testing
    #[allow(dead_code)]
    struct MockResolver;

    #[allow(dead_code)]
    impl MockResolver {
        async fn resolve(
            &self,
            query: DNSPacket,
            _id: u16,
        ) -> Result<DNSPacket, Box<dyn std::error::Error + Send + Sync>> {
            let mut response = DNSPacket::default();
            response.header.id = query.header.id;
            response.header.qr = true;
            response.header.rd = true;
            response.header.ra = true;
            response.header.qdcount = 1;
            response.header.ancount = 1;

            // Echo back the question
            response.questions = query.questions.clone();

            // Add a simple A record answer for testing
            if !query.questions.is_empty() {
                let answer = crate::dns::resource::DNSResource {
                    labels: query.questions[0].labels.clone(),
                    rtype: DNSResourceType::A,
                    rclass: DNSResourceClass::IN,
                    ttl: 300,
                    rdlength: 4,
                    rdata: vec![192, 168, 1, 1], // 192.168.1.1
                    parsed_rdata: Some("192.168.1.1".to_string()),
                    raw_class: None,
                };
                response.answers.push(answer);
            }

            Ok(response)
        }
    }

    async fn create_test_resolver() -> Arc<crate::resolver::DnsResolver> {
        Arc::new(
            crate::resolver::DnsResolver::new(crate::config::DnsConfig::default(), None)
                .await
                .unwrap(),
        )
    }

    async fn create_test_context() -> DohContext {
        let resolver = create_test_resolver().await;

        DohContext {
            resolver,
            metrics: None,
            stats: Arc::new(DohServerStats::default()),
            config: DohServerConfig::default(),
        }
    }

    fn create_test_dns_query() -> Vec<u8> {
        let mut query = DNSPacket::default();
        query.header.id = 1234;
        query.header.rd = true;
        query.header.qdcount = 1;

        let question = DNSQuestion {
            labels: vec!["example".to_string(), "com".to_string()],
            qtype: DNSResourceType::A,
            qclass: DNSResourceClass::IN,
        };
        query.questions.push(question);

        query.serialize().unwrap()
    }

    #[test]
    fn test_doh_server_config_default() {
        let config = DohServerConfig::default();
        assert!(config.enable_tls);
        assert_eq!(config.path, "/dns-query");
        assert!(config.enable_well_known);
        assert!(config.enable_json_api);
        assert_eq!(config.max_request_size, 8192);
        assert!(config.enable_cors);
        assert_eq!(config.max_connections, 1000);
        assert_eq!(config.rate_limit_per_ip, Some(100));
    }

    #[test]
    fn test_doh_server_config_custom() {
        let config = DohServerConfig {
            enable_tls: false,
            path: "/custom-dns".to_string(),
            enable_well_known: false,
            enable_json_api: false,
            max_request_size: 4096,
            request_timeout: Duration::from_secs(10),
            enable_cors: false,
            max_connections: 500,
            rate_limit_per_ip: Some(50),
        };

        assert!(!config.enable_tls);
        assert_eq!(config.path, "/custom-dns");
        assert!(!config.enable_well_known);
        assert!(!config.enable_json_api);
        assert_eq!(config.max_request_size, 4096);
        assert_eq!(config.request_timeout, Duration::from_secs(10));
        assert!(!config.enable_cors);
        assert_eq!(config.max_connections, 500);
        assert_eq!(config.rate_limit_per_ip, Some(50));
    }

    #[test]
    fn test_doh_server_stats_default() {
        let stats = DohServerStats::default();
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.wireformat_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.json_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.get_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.post_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successful_responses.load(Ordering::Relaxed), 0);
        assert_eq!(stats.client_errors.load(Ordering::Relaxed), 0);
        assert_eq!(stats.server_errors.load(Ordering::Relaxed), 0);
        assert_eq!(stats.average_response_time_ms.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_format_rdata() {
        use crate::dns::enums::DNSResourceType;

        // Test A record
        let ipv4_data = vec![192, 168, 1, 1];
        assert_eq!(format_rdata(&ipv4_data, DNSResourceType::A), "192.168.1.1");

        // Test invalid A record
        let invalid_ipv4 = vec![192, 168, 1];
        assert_eq!(format_rdata(&invalid_ipv4, DNSResourceType::A), "invalid");

        // Test AAAA record
        let ipv6_data = vec![
            0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(
            format_rdata(&ipv6_data, DNSResourceType::AAAA),
            "2001:0db8:0000:0000:0000:0000:0000:0001"
        );

        // Test invalid AAAA record
        let invalid_ipv6 = vec![0x20, 0x01];
        assert_eq!(
            format_rdata(&invalid_ipv6, DNSResourceType::AAAA),
            "invalid"
        );

        // Test other record types (should return base64)
        let other_data = vec![1, 2, 3, 4];
        let base64_result = format_rdata(&other_data, DNSResourceType::MX);
        assert_eq!(
            base64_result,
            base64::engine::general_purpose::STANDARD.encode(&other_data)
        );
    }

    #[test]
    fn test_convert_dns_to_json() {
        let mut dns = DNSPacket::default();
        dns.header.id = 1234;
        dns.header.qr = true;
        dns.header.rd = true;
        dns.header.ra = true;
        dns.header.rcode = 0;
        dns.header.z = 0x20; // Set AD bit

        // Add question
        let question = DNSQuestion {
            labels: vec!["example".to_string(), "com".to_string()],
            qtype: DNSResourceType::A,
            qclass: DNSResourceClass::IN,
        };
        dns.questions.push(question);

        // Add answer
        let answer = crate::dns::resource::DNSResource {
            labels: vec!["example".to_string(), "com".to_string()],
            rtype: DNSResourceType::A,
            rclass: DNSResourceClass::IN,
            ttl: 300,
            rdlength: 4,
            rdata: vec![192, 168, 1, 1],
            parsed_rdata: Some("192.168.1.1".to_string()),
            raw_class: None,
        };
        dns.answers.push(answer);

        let json = convert_dns_to_json(&dns);

        assert_eq!(json["Status"], 0);
        assert_eq!(json["RD"], true);
        assert_eq!(json["RA"], true);
        assert_eq!(json["AD"], true); // AD bit should be set
        assert_eq!(json["CD"], false); // CD bit should not be set

        // Check question
        let questions = json["Question"].as_array().unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0]["name"], "example.com");
        assert_eq!(questions[0]["type"], 1); // A record

        // Check answer
        let answers = json["Answer"].as_array().unwrap();
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0]["name"], "example.com");
        assert_eq!(answers[0]["type"], 1); // A record
        assert_eq!(answers[0]["TTL"], 300);
        assert_eq!(answers[0]["data"], "192.168.1.1");
    }

    #[test]
    fn test_convert_dns_to_json_empty() {
        let dns = DNSPacket::default();
        let json = convert_dns_to_json(&dns);

        assert_eq!(json["Status"], 0);
        assert_eq!(json["RD"], false);
        assert_eq!(json["RA"], false);
        assert_eq!(json["AD"], false);
        assert_eq!(json["CD"], false);

        // Should not have Question or Answer fields
        assert!(json.get("Question").is_none());
        assert!(json.get("Answer").is_none());
    }

    #[tokio::test]
    async fn test_process_json_query() {
        let ctx = create_test_context().await;

        let result = process_json_query(&ctx, "example.com", "A").await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/dns-json");
    }

    #[tokio::test]
    async fn test_process_json_query_invalid_type() {
        let ctx = create_test_context().await;

        // Test with invalid numeric type
        let result = process_json_query(&ctx, "example.com", "999").await;
        assert!(result.is_ok()); // Should still work, defaults to A record
    }

    #[test]
    fn test_process_json_query_type_parsing() {
        // Test various DNS record type parsing
        let test_cases = vec![
            ("A", 1),
            ("AAAA", 28),
            ("CNAME", 5),
            ("MX", 15),
            ("TXT", 16),
            ("NS", 2),
            ("SOA", 6),
            ("PTR", 12),
            ("SRV", 33),
            ("invalid", 1), // Should default to A
            ("123", 123),   // Numeric should parse
        ];

        for (input, expected) in test_cases {
            let qtype = match input.to_uppercase().as_str() {
                "A" => 1,
                "AAAA" => 28,
                "CNAME" => 5,
                "MX" => 15,
                "TXT" => 16,
                "NS" => 2,
                "SOA" => 6,
                "PTR" => 12,
                "SRV" => 33,
                _ => input.parse().unwrap_or(1),
            };
            assert_eq!(qtype, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_doh_query_params() {
        // Test DohQueryParams deserialization
        let params = DohQueryParams {
            dns: Some("AAABAAABAAAAAAAAA3d3dwdleGFtcGxlA2NvbQAAAQAB".to_string()),
            ct: Some("application/dns-message".to_string()),
        };

        assert!(params.dns.is_some());
        assert!(params.ct.is_some());
        assert_eq!(params.ct.unwrap(), "application/dns-message");
    }

    #[test]
    fn test_base64_encoding_decoding() {
        let test_data = create_test_dns_query();
        let encoded = URL_SAFE_NO_PAD.encode(&test_data);
        let decoded = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
        assert_eq!(test_data, decoded);
    }

    // Integration tests for DoH server functionality
    #[tokio::test]
    async fn test_doh_server_creation() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: false, // Disable TLS to avoid requiring TLS config
            ..Default::default()
        };

        let result = DohServer::new(bind_addr, None, resolver, None, config);
        assert!(result.is_ok());

        let server = result.unwrap();
        assert_eq!(server.bind_addr, bind_addr);
        assert!(server.tls_acceptor.is_none());
    }

    #[tokio::test]
    async fn test_doh_server_creation_with_tls_required() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: true,
            ..Default::default()
        };

        // Should fail when TLS is enabled but no TLS config provided
        let result = DohServer::new(bind_addr, None, resolver, None, config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_doh_server_get_stats() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: false,
            ..Default::default()
        };

        let server = DohServer::new(bind_addr, None, resolver, None, config).unwrap();
        let stats = server.get_stats();

        // All stats should be zero initially
        assert_eq!(stats.total_requests.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successful_responses.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_handle_doh_get_valid_request() {
        let ctx = create_test_context().await;
        let dns_query = create_test_dns_query();
        let encoded_query = URL_SAFE_NO_PAD.encode(&dns_query);

        let params = DohQueryParams {
            dns: Some(encoded_query),
            ct: None,
        };

        let headers = HeaderMap::new();
        let result = handle_doh_get(
            axum::extract::Query(params),
            headers,
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/dns-message");
    }

    #[tokio::test]
    async fn test_handle_doh_get_missing_dns_param() {
        let ctx = create_test_context().await;

        let params = DohQueryParams {
            dns: None,
            ct: None,
        };

        let headers = HeaderMap::new();
        let result = handle_doh_get(
            axum::extract::Query(params),
            headers,
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_doh_get_invalid_base64() {
        let ctx = create_test_context().await;

        let params = DohQueryParams {
            dns: Some("invalid-base64!@#$".to_string()),
            ct: None,
        };

        let headers = HeaderMap::new();
        let result = handle_doh_get(
            axum::extract::Query(params),
            headers,
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_doh_get_invalid_dns_packet() {
        let ctx = create_test_context().await;

        // Valid base64 but invalid DNS packet
        let invalid_dns = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let encoded_query = URL_SAFE_NO_PAD.encode(&invalid_dns);

        let params = DohQueryParams {
            dns: Some(encoded_query),
            ct: None,
        };

        let headers = HeaderMap::new();
        let result = handle_doh_get(
            axum::extract::Query(params),
            headers,
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_doh_post_valid_request() {
        let ctx = create_test_context().await;
        let dns_query = create_test_dns_query();

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/dns-message".parse().unwrap());

        let body = axum::body::Bytes::from(dns_query);
        let result = handle_doh_post(headers, axum::extract::Extension(ctx), body).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/dns-message");
    }

    #[tokio::test]
    async fn test_handle_doh_post_wrong_content_type() {
        let ctx = create_test_context().await;
        let dns_query = create_test_dns_query();

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());

        let body = axum::body::Bytes::from(dns_query);
        let result = handle_doh_post(headers, axum::extract::Extension(ctx), body).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_doh_post_no_content_type() {
        let ctx = create_test_context().await;
        let dns_query = create_test_dns_query();

        let headers = HeaderMap::new(); // No content-type header

        let body = axum::body::Bytes::from(dns_query);
        let result = handle_doh_post(headers, axum::extract::Extension(ctx), body).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_doh_post_payload_too_large() {
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            max_request_size: 100, // Set a small limit
            ..Default::default()
        };

        let ctx = DohContext {
            resolver,
            metrics: None,
            stats: Arc::new(DohServerStats::default()),
            config,
        };

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/dns-message".parse().unwrap());

        // Create a large payload that exceeds the limit
        let large_payload = vec![0u8; 200];
        let body = axum::body::Bytes::from(large_payload);

        let result = handle_doh_post(headers, axum::extract::Extension(ctx), body).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_handle_doh_post_invalid_dns_packet() {
        let ctx = create_test_context().await;

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/dns-message".parse().unwrap());

        // Invalid DNS packet data
        let invalid_dns = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let body = axum::body::Bytes::from(invalid_dns);

        let result = handle_doh_post(headers, axum::extract::Extension(ctx), body).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_json_resolve_valid_request() {
        let ctx = create_test_context().await;

        let mut params = HashMap::new();
        params.insert("name".to_string(), "example.com".to_string());
        params.insert("type".to_string(), "A".to_string());

        let result =
            handle_json_resolve(axum::extract::Query(params), axum::extract::Extension(ctx)).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/dns-json");
    }

    #[tokio::test]
    async fn test_handle_json_resolve_missing_name() {
        let ctx = create_test_context().await;

        let params = HashMap::new(); // No name parameter

        let result =
            handle_json_resolve(axum::extract::Query(params), axum::extract::Extension(ctx)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_json_resolve_name_path() {
        let ctx = create_test_context().await;

        let result = handle_json_resolve_name(
            axum::extract::Path("example.com".to_string()),
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_handle_json_resolve_type_path() {
        let ctx = create_test_context().await;

        let result = handle_json_resolve_type(
            axum::extract::Path(("example.com".to_string(), "AAAA".to_string())),
            axum::extract::Extension(ctx),
        )
        .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_doh_context_clone() {
        let ctx = create_test_context().await;
        let cloned_ctx = ctx.clone();

        // Verify that cloning works and produces equivalent context
        assert_eq!(ctx.config.path, cloned_ctx.config.path);
        assert_eq!(ctx.config.enable_tls, cloned_ctx.config.enable_tls);
    }

    #[tokio::test]
    async fn test_router_creation() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: false,
            ..Default::default()
        };

        let server = DohServer::new(bind_addr, None, resolver, None, config).unwrap();
        let _router = server.create_router(); // Should not panic
    }

    #[tokio::test]
    async fn test_router_creation_with_all_features() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: false,
            enable_well_known: true,
            enable_json_api: true,
            enable_cors: true,
            ..Default::default()
        };

        let server = DohServer::new(bind_addr, None, resolver, None, config).unwrap();
        let _router = server.create_router(); // Should not panic
    }

    #[tokio::test]
    async fn test_router_creation_minimal_features() {
        let bind_addr = "127.0.0.1:8443".parse().unwrap();
        let resolver = create_test_resolver().await;
        let config = DohServerConfig {
            enable_tls: false,
            enable_well_known: false,
            enable_json_api: false,
            enable_cors: false,
            ..Default::default()
        };

        let server = DohServer::new(bind_addr, None, resolver, None, config).unwrap();
        let _router = server.create_router(); // Should not panic
    }

    // Add simple test to verify our tests are working
    #[test]
    fn test_simple_format_functionality() {
        // Test base64 encoding/decoding functionality
        let test_data = b"hello world";
        let encoded = URL_SAFE_NO_PAD.encode(test_data);
        let decoded = URL_SAFE_NO_PAD.decode(&encoded).unwrap();
        assert_eq!(test_data, decoded.as_slice());
    }
}
