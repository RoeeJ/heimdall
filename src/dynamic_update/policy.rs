//! Update policy management for dynamic DNS updates

use crate::dns::DNSPacket;
use std::collections::HashMap;
use std::net::IpAddr;
use tracing::{debug, info};

/// Update permission types
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatePermission {
    /// Deny all updates
    Deny,
    /// Allow all updates (dangerous!)
    AllowAll,
    /// Allow updates to specific names
    AllowNames(Vec<String>),
    /// Allow updates to names matching a pattern
    AllowPattern(String),
    /// Allow updates from specific IP addresses
    AllowFromIPs(Vec<IpAddr>),
    /// Allow updates with valid TSIG key
    RequireTsig,
    /// Allow updates with specific TSIG keys
    RequireTsigKeys(Vec<String>),
}

/// Update policy for a zone
#[derive(Debug, Clone)]
pub struct ZoneUpdatePolicy {
    /// Zone name
    pub zone: String,
    /// Permissions for this zone
    pub permissions: Vec<UpdatePermission>,
}

impl ZoneUpdatePolicy {
    /// Create a new zone update policy
    pub fn new(zone: String) -> Self {
        Self {
            zone: zone.to_lowercase(),
            permissions: vec![UpdatePermission::Deny], // Deny by default
        }
    }

    /// Add a permission
    pub fn add_permission(&mut self, permission: UpdatePermission) {
        self.permissions.push(permission);
    }

    /// Check if an update is allowed
    pub fn is_allowed(
        &self,
        authenticated_key: &Option<String>,
        packet: &DNSPacket,
        source_ip: Option<IpAddr>,
    ) -> bool {
        // Check each permission
        for permission in &self.permissions {
            match permission {
                UpdatePermission::Deny => {
                    debug!("Update denied by explicit deny rule");
                    return false;
                }

                UpdatePermission::AllowAll => {
                    debug!("Update allowed by allow-all rule");
                    return true;
                }

                UpdatePermission::AllowNames(names) => {
                    // Check if all updated names are in the allowed list
                    if self.check_allowed_names(packet, names) {
                        debug!("Update allowed by name whitelist");
                        return true;
                    }
                }

                UpdatePermission::AllowPattern(pattern) => {
                    // Check if all updated names match the pattern
                    if self.check_name_pattern(packet, pattern) {
                        debug!("Update allowed by name pattern");
                        return true;
                    }
                }

                UpdatePermission::AllowFromIPs(ips) => {
                    if let Some(ip) = source_ip {
                        if ips.contains(&ip) {
                            debug!("Update allowed by IP whitelist");
                            return true;
                        }
                    }
                }

                UpdatePermission::RequireTsig => {
                    if authenticated_key.is_some() {
                        debug!("Update allowed by TSIG authentication");
                        return true;
                    }
                }

                UpdatePermission::RequireTsigKeys(keys) => {
                    if let Some(key) = authenticated_key {
                        if keys.contains(key) {
                            debug!("Update allowed by specific TSIG key: {}", key);
                            return true;
                        }
                    }
                }
            }
        }

        // No permission matched
        debug!("Update denied - no matching permission");
        false
    }

    /// Check if all updated names are in the allowed list
    fn check_allowed_names(&self, packet: &DNSPacket, allowed_names: &[String]) -> bool {
        // Extract all names from update section (authority)
        for rr in &packet.authorities {
            let name = rr.labels.join(".").to_lowercase();
            if !allowed_names
                .iter()
                .any(|allowed| allowed.to_lowercase() == name)
            {
                return false;
            }
        }
        true
    }

    /// Check if all updated names match the pattern
    fn check_name_pattern(&self, packet: &DNSPacket, pattern: &str) -> bool {
        // Simple pattern matching (e.g., "*.dyn.example.com")
        let pattern_lower = pattern.to_lowercase();

        for rr in &packet.authorities {
            let name = rr.labels.join(".").to_lowercase();

            if let Some(suffix) = pattern_lower.strip_prefix('*') {
                // Wildcard pattern
                if !name.ends_with(suffix) {
                    return false;
                }
            } else if name != pattern_lower {
                return false;
            }
        }
        true
    }
}

/// Global update policy manager
#[derive(Debug, Clone)]
pub struct UpdatePolicy {
    /// Per-zone policies
    zone_policies: HashMap<String, ZoneUpdatePolicy>,
    /// Default policy for zones without specific policy
    default_policy: UpdatePermission,
}

impl UpdatePolicy {
    /// Create a new update policy manager
    pub fn new() -> Self {
        Self {
            zone_policies: HashMap::new(),
            default_policy: UpdatePermission::Deny, // Deny by default
        }
    }

    /// Set the default policy
    pub fn set_default_policy(&mut self, policy: UpdatePermission) {
        self.default_policy = policy;
    }

    /// Add a zone-specific policy
    pub fn add_zone_policy(&mut self, policy: ZoneUpdatePolicy) {
        self.zone_policies.insert(policy.zone.clone(), policy);
    }

    /// Check if an update is allowed
    pub fn is_allowed(
        &self,
        zone: &str,
        authenticated_key: &Option<String>,
        packet: &DNSPacket,
    ) -> bool {
        let zone_lower = zone.to_lowercase();

        // Check zone-specific policy
        if let Some(zone_policy) = self.zone_policies.get(&zone_lower) {
            return zone_policy.is_allowed(authenticated_key, packet, None);
        }

        // Fall back to default policy
        match &self.default_policy {
            UpdatePermission::Deny => {
                info!("Update denied by default policy for zone: {}", zone);
                false
            }
            UpdatePermission::AllowAll => {
                info!(
                    "Update allowed by default allow-all policy for zone: {}",
                    zone
                );
                true
            }
            UpdatePermission::RequireTsig => {
                if authenticated_key.is_some() {
                    info!(
                        "Update allowed by default TSIG requirement for zone: {}",
                        zone
                    );
                    true
                } else {
                    info!(
                        "Update denied - TSIG required by default policy for zone: {}",
                        zone
                    );
                    false
                }
            }
            _ => {
                // Other permissions don't make sense as defaults
                info!("Update denied - invalid default policy for zone: {}", zone);
                false
            }
        }
    }
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::DNSPacket;
    use crate::dns::enums::DNSResourceType;
    use crate::dns::resource::DNSResource;

    #[test]
    fn test_zone_policy_deny() {
        let policy = ZoneUpdatePolicy::new("example.com".to_string());
        let packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };

        assert!(!policy.is_allowed(&None, &packet, None));
    }

    #[test]
    fn test_zone_policy_allow_all() {
        let mut policy = ZoneUpdatePolicy::new("example.com".to_string());
        policy.permissions.clear();
        policy.add_permission(UpdatePermission::AllowAll);

        let packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };
        assert!(policy.is_allowed(&None, &packet, None));
    }

    #[test]
    fn test_zone_policy_require_tsig() {
        let mut policy = ZoneUpdatePolicy::new("example.com".to_string());
        policy.permissions.clear();
        policy.add_permission(UpdatePermission::RequireTsig);

        let packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };

        // Without TSIG
        assert!(!policy.is_allowed(&None, &packet, None));

        // With TSIG
        let key = Some("test-key".to_string());
        assert!(policy.is_allowed(&key, &packet, None));
    }

    #[test]
    fn test_zone_policy_allowed_names() {
        let mut policy = ZoneUpdatePolicy::new("example.com".to_string());
        policy.permissions.clear();
        policy.add_permission(UpdatePermission::AllowNames(vec![
            "www.example.com".to_string(),
            "mail.example.com".to_string(),
        ]));

        // Create packet with update
        let mut packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };
        let rr = DNSResource {
            labels: vec!["www".to_string(), "example".to_string(), "com".to_string()],
            rtype: DNSResourceType::A,
            rclass: crate::dns::enums::DNSResourceClass::IN,
            ttl: 300,
            rdlength: 4,
            rdata: vec![192, 168, 1, 1],
            parsed_rdata: None,
            raw_class: None,
        };
        packet.authorities.push(rr);

        assert!(policy.is_allowed(&None, &packet, None));

        // Try with disallowed name
        packet.authorities[0].labels =
            vec!["ftp".to_string(), "example".to_string(), "com".to_string()];
        assert!(!policy.is_allowed(&None, &packet, None));
    }

    #[test]
    fn test_global_policy_default() {
        let policy = UpdatePolicy::new();
        let packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };

        // Default should deny
        assert!(!policy.is_allowed("example.com", &None, &packet));
    }

    #[test]
    fn test_global_policy_with_zone_policy() {
        let mut policy = UpdatePolicy::new();

        // Add zone-specific policy
        let mut zone_policy = ZoneUpdatePolicy::new("example.com".to_string());
        zone_policy.permissions.clear();
        zone_policy.add_permission(UpdatePermission::AllowAll);
        policy.add_zone_policy(zone_policy);

        let packet = DNSPacket {
            header: crate::dns::header::DNSHeader::default(),
            questions: Vec::new(),
            answers: Vec::new(),
            authorities: Vec::new(),
            resources: Vec::new(),
            edns: None,
        };

        // Should allow for example.com
        assert!(policy.is_allowed("example.com", &None, &packet));

        // Should deny for other zones
        assert!(!policy.is_allowed("other.com", &None, &packet));
    }
}
