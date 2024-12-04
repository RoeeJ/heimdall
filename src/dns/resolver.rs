use anyhow::{anyhow, Result};
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

use crate::prelude::*;

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    answers: Vec<DnsResourceRecord>,
    ttl: u32,
}

impl CacheEntry {
    fn new(answers: Vec<DnsResourceRecord>, ttl: u32) -> Self {
        Self { answers, ttl }
    }
}

pub struct DnsResolver {
    redis_client: Client,
    blocklist: Arc<RwLock<HashSet<String>>>,
    public_suffixes: Arc<RwLock<HashSet<String>>>,
    forward_dns: String,
}

impl DnsResolver {
    pub async fn new(redis_url: &str, forward_dns: &str) -> Result<Self> {
        let redis_client = Client::open(redis_url.to_string())?;

        Ok(Self {
            redis_client,
            blocklist: Arc::new(RwLock::new(HashSet::new())),
            public_suffixes: Arc::new(RwLock::new(HashSet::new())),
            forward_dns: forward_dns.to_string(),
        })
    }

    pub async fn set_blocklist(&self, domains: HashSet<String>) {
        let mut blocklist = self.blocklist.write().await;
        *blocklist = domains;
    }

    pub async fn set_public_suffixes(&self, suffixes: HashSet<String>) {
        let mut public_suffixes = self.public_suffixes.write().await;
        *public_suffixes = suffixes;
    }

    pub async fn is_domain_blocked(&self, domain: &str) -> bool {
        let check_domain = domain.trim_end_matches('.').to_lowercase();
        let blocklist = self.blocklist.read().await;
        let public_suffixes = self.public_suffixes.read().await;

        // Skip if domain is a public suffix
        if public_suffixes.contains(&check_domain) {
            return false;
        }

        // Check exact match
        if blocklist.contains(&check_domain) {
            return true;
        }

        // Check all domain parts
        let parts: Vec<&str> = check_domain.split('.').collect();
        for i in 0..parts.len() {
            let wildcard = parts[i..].join(".").to_string();
            if blocklist.contains(&wildcard) {
                return true;
            }
        }

        false
    }

    pub async fn handle_query(&self, query: &[u8]) -> Result<Vec<u8>> {
        // Parse the DNS packet
        let packet = DnsPacket::from_wire(&mut BitReader::new(Cursor::new(query)))
            .map_err(|e| anyhow!("Failed to parse DNS packet: {}", e))?;

        // Check if any of the questions are blocked
        for question in &packet.questions {
            if self.is_domain_blocked(&question.name).await {
                let mut response = packet.create_response();
                response.header.rcode = DnsResponseCode::NameError;
                return Ok(response.to_wire());
            }

            // Try cache first
            let cache_key = format!(
                "dns:{}:{}:{}",
                question.name,
                Into::<u16>::into(question.qtype),
                Into::<u16>::into(question.qclass)
            );

            if let Ok(mut redis) = self.redis_client.get_async_connection().await {
                if let Ok(cached) = redis.get::<_, String>(&cache_key).await {
                    if let Ok(entry) = serde_json::from_str::<CacheEntry>(&cached) {
                        let mut response = packet.create_response();
                        response.answers = entry.answers;
                        response.update_counts();
                        return Ok(response.to_wire());
                    }
                }
            }
        }

        // Forward the query if not blocked or cached
        match self.forward_dns_query(query).await {
            Ok(response) => {
                if let Ok(response_packet) =
                    DnsPacket::from_wire(&mut BitReader::new(Cursor::new(&response)))
                {
                    let mut dns_response = packet.create_response();

                    // Fix the answer records to maintain the correct name
                    let fixed_answers: Vec<DnsResourceRecord> = response_packet
                        .answers
                        .into_iter()
                        .map(|mut rr| {
                            // Use the question name if the record name is "."
                            if rr.name == "." {
                                rr.name = packet.questions[0].name.clone();
                            }
                            rr
                        })
                        .collect();

                    dns_response.answers = fixed_answers;
                    dns_response.update_counts();

                    if !dns_response.answers.is_empty() {
                        let min_ttl = dns_response
                            .answers
                            .iter()
                            .map(|a| a.ttl)
                            .min()
                            .unwrap_or(300);

                        let cache_key = get_cache_key(
                            &packet.questions[0].name,
                            packet.questions[0].qtype,
                            packet.questions[0].qclass,
                        );

                        if !self
                            .cache_dns_response(&cache_key, dns_response.answers.clone(), min_ttl)
                            .await
                        {
                            eprintln!("Failed to cache DNS response");
                        }

                        Ok(dns_response.to_wire())
                    } else {
                        Ok(dns_response.to_wire())
                    }
                } else {
                    let mut dns_response = packet.create_response();
                    dns_response.header.rcode = DnsResponseCode::ServerFailure;
                    Ok(dns_response.to_wire())
                }
            }
            Err(e) => {
                eprintln!("Forward query failed: {}", e);
                let mut response = packet.create_response();
                response.header.rcode = DnsResponseCode::ServerFailure;
                Ok(response.to_wire())
            }
        }
    }

    async fn forward_dns_query(&self, query: &[u8]) -> Result<Vec<u8>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let addr = format!("{}:53", self.forward_dns).parse::<SocketAddr>()?;
        socket.send_to(query, addr).await?;

        let mut buf = vec![0; 512];
        let (len, _) = socket.recv_from(&mut buf).await?;
        buf.truncate(len);
        Ok(buf)
    }

    async fn cache_dns_response(
        &self,
        key: &str,
        answers: Vec<DnsResourceRecord>,
        ttl: u32,
    ) -> bool {
        let cache_entry = CacheEntry::new(answers, ttl);

        if let Ok(json) = serde_json::to_string(&cache_entry) {
            if let Ok(mut redis) = self.redis_client.get_async_connection().await {
                return redis
                    .set_ex::<_, _, ()>(key, json, ttl as usize)
                    .await
                    .is_ok();
            }
        }
        true
    }
}
