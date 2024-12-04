use crate::prelude::*;
use std::net::IpAddr;
use trust_dns_client::rr::{RData as TrustDnsRData, Record as TrustDnsRecord};

pub trait FromTrustDns {
    type Source;
    fn from_trust_dns(source: &Self::Source) -> Option<Self>
    where
        Self: Sized;
}

impl FromTrustDns for RData {
    type Source = TrustDnsRData;

    fn from_trust_dns(source: &TrustDnsRData) -> Option<Self> {
        match source {
            TrustDnsRData::A(ipv4) => Some(RData::A(*ipv4)),
            TrustDnsRData::AAAA(ipv6) => Some(RData::AAAA(*ipv6)),
            TrustDnsRData::CNAME(name) => Some(RData::CNAME(name.to_string())),
            TrustDnsRData::NS(name) => Some(RData::NS(name.to_string())),
            TrustDnsRData::PTR(name) => Some(RData::PTR(name.to_string())),
            TrustDnsRData::MX(exchange) => Some(RData::MX {
                preference: exchange.preference(),
                exchange: exchange.exchange().to_string(),
            }),
            TrustDnsRData::TXT(strings) => Some(RData::TXT(
                strings
                    .iter()
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .collect(),
            )),
            TrustDnsRData::SOA(soa) => Some(RData::SOA {
                mname: soa.mname().to_string(),
                rname: soa.rname().to_string(),
                serial: soa.serial() as u32,
                refresh: soa.refresh() as u32,
                retry: soa.retry() as u32,
                expire: soa.expire() as u32,
                minimum: soa.minimum() as u32,
            }),
            TrustDnsRData::SRV(srv) => Some(RData::SRV {
                priority: srv.priority(),
                weight: srv.weight() as u16,
                port: srv.port() as u16,
                target: srv.target().to_string(),
            }),
            _ => None,
        }
    }
}

impl FromTrustDns for DnsAnswer {
    type Source = TrustDnsRecord;

    fn from_trust_dns(source: &TrustDnsRecord) -> Option<Self> {
        let rdata = RData::from_trust_dns(source.data()?)?;
        let qtype = DnsQType::from(u16::from(source.record_type()));

        let ours = DnsAnswer {
            name: source.name().to_string(),
            qtype,
            qclass: DnsQClass::IN,
            ttl: source.ttl(),
            length: 0, // Will be calculated during wire format
            rdata,
        };

        Some(ours)
    }
}

pub fn get_cache_key(name: &str, qtype: DnsQType, qclass: DnsQClass) -> String {
    format!(
        "{}:{}:{}",
        name,
        Into::<u16>::into(qtype),
        Into::<u16>::into(qclass)
    )
}

pub fn get_cache_value(rdata: &RData) -> Option<String> {
    match rdata {
        RData::A(ipv4) => Some(IpAddr::V4(*ipv4).to_string()),
        RData::AAAA(ipv6) => Some(IpAddr::V6(*ipv6).to_string()),
        RData::CNAME(name) => Some(name.clone()),
        RData::NS(name) => Some(name.clone()),
        RData::PTR(name) => Some(name.clone()),
        RData::MX {
            preference,
            exchange,
        } => Some(format!("{} {}", preference, exchange)),
        RData::TXT(strings) => Some(strings.join(" ")),
        RData::SOA { mname, .. } => Some(mname.clone()),
        RData::SRV { target, .. } => Some(target.clone()),
        _ => None,
    }
}
