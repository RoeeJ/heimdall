use redis::{aio::Connection, AsyncCommands, Client};
use reqwest;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use trust_dns_client::client::ClientHandle;
use trust_dns_client::{
    client::AsyncClient,
    op::DnsResponse,
    rr::{DNSClass, Name, RecordType},
    tcp::TcpClientStream,
};

use super::conversion::{get_cache_key, get_cache_value, FromTrustDns};
use crate::constants::{BLOCKLIST_REFRESH_INTERVAL, BLOCKLIST_URL, PUBLIC_SUFFIX_LIST_URL};
use crate::prelude::*;
use tokio::net::TcpStream as TokioTcpStream;
use trust_dns_client::proto::iocompat::AsyncIoTokioAsStd;

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    values: Vec<String>,
    record_type: u16,
}

struct StringInterner {
    strings: Vec<String>,
    lookup: HashSet<&'static str>,
}

impl StringInterner {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            lookup: HashSet::new(),
        }
    }

    fn clear(&mut self) {
        self.strings.clear();
        self.lookup.clear();
    }

    fn intern(&mut self, s: String) -> &'static str {
        if let Some(&existing) = self.lookup.get(s.as_str()) {
            return existing;
        }
        self.strings.push(s);
        let s = self.strings.last().unwrap();
        let s: &'static str = unsafe { std::mem::transmute(s.as_str()) };
        self.lookup.insert(s);
        s
    }
}

pub struct DnsResolver {
    redis_client: Client,
    blocklist: Arc<RwLock<(StringInterner, HashSet<&'static str>)>>,
    public_suffixes: Arc<RwLock<(StringInterner, HashSet<&'static str>)>>,
    last_blocklist_update: Arc<RwLock<u64>>,
    last_suffix_update: Arc<RwLock<u64>>,
}

impl DnsResolver {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let redis_client = Client::open(redis_url.to_string())?;

        Ok(Self {
            redis_client,
            blocklist: Arc::new(RwLock::new((StringInterner::new(), HashSet::new()))),
            public_suffixes: Arc::new(RwLock::new((StringInterner::new(), HashSet::new()))),
            last_blocklist_update: Arc::new(RwLock::new(0)),
            last_suffix_update: Arc::new(RwLock::new(0)),
        })
    }

    pub async fn check_updates(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check blocklist update
        {
            let should_update = {
                let last_update = self.last_blocklist_update.read().await;
                now - *last_update > BLOCKLIST_REFRESH_INTERVAL
            };
            if should_update {
                if let Err(e) = self.update_blocklist().await {
                    eprintln!("Failed to update blocklist: {}", e);
                }
            }
        }

        // Check suffix list update
        {
            let should_update = {
                let last_update = self.last_suffix_update.read().await;
                now - *last_update > BLOCKLIST_REFRESH_INTERVAL
            };
            if should_update {
                if let Err(e) = self.update_public_suffixes().await {
                    eprintln!("Failed to update public suffix list: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn update_blocklist(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Updating blocklist...");

        // Download blocklist
        let response = reqwest::get(BLOCKLIST_URL).await?;
        let content = response.text().await?;

        let mut new_domains = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                new_domains.push(line.to_lowercase());
            }
        }

        println!("Blocklist updated with {} entries", new_domains.len());

        // Update blocklist after all async operations
        {
            let mut blocklist = self.blocklist.write().await;
            let mut last_update = self.last_blocklist_update.write().await;
            
            let (ref mut interner, ref mut domains) = *blocklist;
            interner.clear();
            domains.clear();

            for domain in new_domains {
                domains.insert(interner.intern(domain));
            }

            *last_update = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        }

        Ok(())
    }

    async fn update_public_suffixes(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Updating public suffix list...");

        // Download public suffix list
        let response = reqwest::get(PUBLIC_SUFFIX_LIST_URL).await?;
        let content = response.text().await?;

        let mut new_suffixes = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with("//") && !line.starts_with('!') && !line.starts_with('*') {
                new_suffixes.push(line.to_lowercase());
            }
        }

        println!("Public suffix list updated with {} entries", new_suffixes.len());

        // Update suffix list after all async operations
        {
            let mut suffixes = self.public_suffixes.write().await;
            let mut last_update = self.last_suffix_update.write().await;
            
            let (ref mut interner, ref mut domains) = *suffixes;
            interner.clear();
            domains.clear();

            for suffix in new_suffixes {
                domains.insert(interner.intern(suffix));
            }

            *last_update = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        }

        Ok(())
    }

    async fn is_public_suffix(&self, domain: &str) -> bool {
        let contains = {
            let suffixes = self.public_suffixes.read().await;
            let (_, ref domains) = *suffixes;
            domains.contains(domain)
        };
        contains
    }

    async fn get_effective_domain_parts(&self, domain: &str) -> Vec<String> {
        let parts: Vec<&str> = domain.split('.').collect();
        if parts.len() < 2 {
            return Vec::new();
        }

        // Build domain from right to left to find the public suffix
        let mut suffix = String::new();
        let mut suffix_len = 0;

        // Build from right to left until we find a public suffix
        for i in (0..parts.len()).rev() {
            if suffix.is_empty() {
                suffix = parts[i].to_string();
            } else {
                suffix = format!("{}.{}", parts[i], suffix);
            }
            if self.is_public_suffix(&suffix).await {
                suffix_len = suffix.split('.').count();
                break;
            }
        }

        // If we didn't find a public suffix, assume the last part is the TLD
        if suffix_len == 0 {
            suffix_len = 1;
        }

        // Handle single-label domains (e.g. "0.org")
        if parts.len() == 2 {
            return vec![domain.to_string()];
        }

        // Generate all possible combinations before the public suffix
        let mut result = Vec::new();
        if parts.len() > suffix_len {
            for i in 0..=(parts.len() - suffix_len - 1) {
                result.push(parts[i..].join("."));
            }
        }
        result
    }

    pub async fn lookup(
        &self,
        packet: &DnsPacket,
        forward_resolver: &mut AsyncClient,
    ) -> Result<DnsPacket, Box<dyn std::error::Error + Send + Sync>> {
        let mut response = packet.create_response();

        for question in &packet.questions {
            // Normalize domain name for checking
            let domain = question.name.trim_end_matches('.').to_lowercase();

            // Check if domain is blocked
            if self.is_domain_blocked(&domain).await {
                response.header.rcode = DnsResponseCode::NameError;
                return Ok(response);
            }

            let cache_key = get_cache_key(&question.name, question.qtype, question.qclass);

            // Try to get from cache first
            let cached_response = {
                let mut redis_conn = self.redis_client.get_async_connection().await?;
                if let Ok(cached_json) = redis_conn.get::<_, String>(&cache_key).await {
                    if let Ok(cache_entry) = serde_json::from_str::<CacheEntry>(&cached_json) {
                        let ttl: u32 = redis_conn.ttl(&cache_key).await?;
                        if ttl > 0 {
                            let mut answers = Vec::new();
                            for value in cache_entry.values {
                                if let Some(rdata) = self.parse_cached_value(&value, question.qtype)
                                {
                                    answers.push(DnsAnswer {
                                        name: question.name.clone(),
                                        qtype: question.qtype,
                                        qclass: question.qclass,
                                        ttl,
                                        length: 0,
                                        rdata,
                                    });
                                }
                            }
                            Some(answers)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(answers) = cached_response {
                response.answers.extend(answers);
                continue;
            }

            // Forward query if not in cache
            match self
                .forward_query(forward_resolver, &question.name, question.qtype)
                .await
            {
                Ok(forward_response) => {
                    let mut cache_values = Vec::new();
                    let mut min_ttl = u32::MAX;

                    for answer in forward_response.answers() {
                        if let Some(dns_answer) = DnsAnswer::from_trust_dns(answer) {
                            min_ttl = min_ttl.min(dns_answer.ttl);

                            // Add to cache values if possible
                            if let Some(cache_value) = get_cache_value(&dns_answer.rdata) {
                                cache_values.push(cache_value);
                            }
                            response.answers.push(dns_answer);
                        }
                    }

                    // Cache all records together if we have any
                    if !cache_values.is_empty() {
                        let cache_entry = CacheEntry {
                            values: cache_values,
                            record_type: Into::<u16>::into(question.qtype),
                        };

                        if let Err(e) = {
                            let mut redis_conn = self.redis_client.get_async_connection().await?;
                            redis_conn
                                .set_ex::<_, _, ()>(
                                    &cache_key,
                                    serde_json::to_string(&cache_entry)?,
                                    min_ttl as usize,
                                )
                                .await
                        } {
                            eprintln!("Failed to cache response: {}", e);
                        }
                    }
                }
                Err(_) => {
                    // If forward query fails, set server failure response code
                    response.header.rcode = DnsResponseCode::ServerFailure;
                }
            }
        }

        Ok(response)
    }

    fn parse_cached_value(&self, value: &str, qtype: DnsQType) -> Option<RData> {
        match qtype {
            DnsQType::A => value.parse().ok().map(RData::A),
            DnsQType::AAAA => value.parse().ok().map(RData::AAAA),
            DnsQType::CNAME => Some(RData::CNAME(value.to_string())),
            DnsQType::NS => Some(RData::NS(value.to_string())),
            DnsQType::PTR => Some(RData::PTR(value.to_string())),
            DnsQType::TXT => Some(RData::TXT(vec![value.to_string()])),
            DnsQType::MX => {
                let parts: Vec<&str> = value.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    if let Ok(preference) = parts[0].parse() {
                        return Some(RData::MX {
                            preference,
                            exchange: parts[1].to_string(),
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    async fn forward_query(
        &self,
        forward_resolver: &mut AsyncClient,
        name: &str,
        qtype: DnsQType,
    ) -> Result<DnsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let name = Name::from_str(name)?;
        let record_type = match qtype {
            DnsQType::A => RecordType::A,
            DnsQType::AAAA => RecordType::AAAA,
            DnsQType::NS => RecordType::NS,
            DnsQType::CNAME => RecordType::CNAME,
            DnsQType::MX => RecordType::MX,
            DnsQType::TXT => RecordType::TXT,
            DnsQType::SOA => RecordType::SOA,
            DnsQType::PTR => RecordType::PTR,
            DnsQType::SRV => RecordType::SRV,
            _ => return Err("Unsupported query type".into()),
        };

        forward_resolver
            .query(name, DNSClass::IN, record_type)
            .await
            .map_err(|e| e.into())
    }

    async fn is_domain_blocked(&self, domain: &str) -> bool {
        let check_domain = domain.trim_end_matches('.').to_lowercase();
        // Check exact match first
        let exact_match = {
            let blocklist = self.blocklist.read().await;
            let (_, ref domains) = *blocklist;
            domains.contains(check_domain.as_str())
        };

        if exact_match {
            return true;
        }

        // Get effective domain parts
        let domain_parts = self.get_effective_domain_parts(&check_domain).await;

        // For single-label domains, only check exact match
        let parts_count = check_domain.split('.').count();

        // Check wildcard patterns
        let contains_pattern = {
            let blocklist = self.blocklist.read().await;
            let (_, ref domains) = *blocklist;

            domain_parts.iter().any(|part| {
                if *part != check_domain {
                    let pattern = format!("*.{}", part);
                    if domains.contains(pattern.as_str()) {
                        return true;
                    }
                }
                false
            })
        };

        contains_pattern
    }
}
