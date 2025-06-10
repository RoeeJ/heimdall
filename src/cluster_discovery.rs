use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Information about a cluster member
#[derive(Debug, Clone)]
pub struct ClusterMember {
    pub address: String,
    pub hostname: String,
    pub last_seen: Instant,
    pub status: MemberStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberStatus {
    Healthy,
    Unhealthy,
    Unknown,
}

/// Cluster discovery for Heimdall instances
pub struct ClusterDiscovery {
    members: Arc<RwLock<HashMap<String, ClusterMember>>>,
    namespace: String,
    service_name: String,
    port: u16,
}

impl ClusterDiscovery {
    pub fn new() -> Option<Self> {
        // Only enable in Kubernetes
        if std::env::var("KUBERNETES_SERVICE_HOST").is_err() {
            debug!("Not running in Kubernetes, cluster discovery disabled");
            return None;
        }

        let namespace = std::env::var("HEIMDALL_NAMESPACE")
            .or_else(|_| std::env::var("KUBERNETES_NAMESPACE"))
            .unwrap_or_else(|_| "default".to_string());

        let service_name =
            std::env::var("HEIMDALL_SERVICE_NAME").unwrap_or_else(|_| "heimdall".to_string());

        let port = std::env::var("HEIMDALL_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);

        info!(
            "Cluster discovery enabled: namespace={}, service={}, port={}, headless_service={}-headless",
            namespace, service_name, port, service_name
        );

        Some(Self {
            members: Arc::new(RwLock::new(HashMap::new())),
            namespace,
            service_name,
            port,
        })
    }

    /// Discover cluster members using DNS lookup
    pub async fn discover_members(&self) -> Vec<ClusterMember> {
        // Try headless service first, then regular service
        let headless_service_name = format!("{}-headless", self.service_name);
        let headless_service = format!(
            "{}.{}.svc.cluster.local",
            headless_service_name, self.namespace
        );

        debug!("Discovering cluster members via DNS: {}", headless_service);

        // Use tokio's DNS resolver
        match tokio::net::lookup_host(format!("{}:{}", headless_service, self.port)).await {
            Ok(addrs) => {
                let mut members = Vec::new();
                let mut members_map = self.members.write().await;
                members_map.clear(); // Clear old entries

                for addr in addrs {
                    let member_id = addr.ip().to_string();
                    let hostname = self
                        .reverse_dns_lookup(addr.ip())
                        .await
                        .unwrap_or_else(|| member_id.clone());

                    let member = ClusterMember {
                        address: addr.to_string(),
                        hostname,
                        last_seen: Instant::now(),
                        status: MemberStatus::Unknown, // Will be updated by health checks
                    };

                    members.push(member.clone());
                    members_map.insert(member_id, member);
                }

                info!("Discovered {} cluster members", members.len());
                members
            }
            Err(e) => {
                warn!(
                    "Failed to discover cluster members for {}: {}",
                    headless_service, e
                );

                // Try alternative: just the service name without FQDN
                let simple_name = &self.service_name;
                debug!("Trying simple service name: {}-headless", simple_name);

                match tokio::net::lookup_host(format!("{}-headless:{}", simple_name, self.port))
                    .await
                {
                    Ok(addrs) => {
                        let mut members = Vec::new();
                        let mut members_map = self.members.write().await;
                        members_map.clear();

                        for addr in addrs {
                            let member_id = addr.ip().to_string();
                            let hostname = self
                                .reverse_dns_lookup(addr.ip())
                                .await
                                .unwrap_or_else(|| member_id.clone());

                            let member = ClusterMember {
                                address: addr.to_string(),
                                hostname,
                                last_seen: Instant::now(),
                                status: MemberStatus::Unknown,
                            };

                            members.push(member.clone());
                            members_map.insert(member_id, member);
                        }

                        info!(
                            "Discovered {} cluster members using simple name",
                            members.len()
                        );
                        members
                    }
                    Err(e2) => {
                        warn!("Failed with simple name too: {}", e2);
                        // Return cached members if available
                        let members = self.members.read().await;
                        members.values().cloned().collect()
                    }
                }
            }
        }
    }

    /// Perform reverse DNS lookup for an IP
    async fn reverse_dns_lookup(&self, ip: IpAddr) -> Option<String> {
        // In Kubernetes, pod names follow a pattern: pod-ip with dots replaced by hyphens
        let ip_str = ip.to_string().replace('.', "-");
        let ptr_name = format!("{}.{}.pod.cluster.local", ip_str, self.namespace);

        // For now, return the constructed PTR name
        // In a real implementation, we'd do an actual reverse DNS lookup
        Some(ptr_name)
    }

    /// Get current cluster members
    pub async fn get_members(&self) -> Vec<ClusterMember> {
        let members = self.members.read().await;
        members.values().cloned().collect()
    }

    /// Update member health status
    pub async fn update_member_health(&self, address: &str, healthy: bool) {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(address) {
            member.status = if healthy {
                MemberStatus::Healthy
            } else {
                MemberStatus::Unhealthy
            };
            member.last_seen = Instant::now();
        }
    }

    /// Remove stale members (not seen for > 5 minutes)
    pub async fn cleanup_stale_members(&self) {
        let mut members = self.members.write().await;
        let stale_threshold = Duration::from_secs(300);

        members.retain(|_, member| member.last_seen.elapsed() < stale_threshold);
    }

    /// Get cluster statistics
    pub async fn get_stats(&self) -> ClusterStats {
        let members = self.members.read().await;

        let total = members.len();
        let healthy = members
            .values()
            .filter(|m| m.status == MemberStatus::Healthy)
            .count();
        let unhealthy = members
            .values()
            .filter(|m| m.status == MemberStatus::Unhealthy)
            .count();
        let unknown = members
            .values()
            .filter(|m| m.status == MemberStatus::Unknown)
            .count();

        ClusterStats {
            total_members: total,
            healthy_members: healthy,
            unhealthy_members: unhealthy,
            unknown_members: unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterStats {
    pub total_members: usize,
    pub healthy_members: usize,
    pub unhealthy_members: usize,
    pub unknown_members: usize,
}

/// Background task to periodically discover cluster members
pub async fn cluster_discovery_task(discovery: Arc<ClusterDiscovery>) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        // Discover members
        let _ = discovery.discover_members().await;

        // Cleanup stale members
        discovery.cleanup_stale_members().await;
    }
}
