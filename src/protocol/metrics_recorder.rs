use crate::metrics::DnsMetrics;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum ResponseStatus {
    Success,
    CacheHit,
    Error,
    RateLimited,
    ParseError,
    Timeout,
    Refused,
    NotImplemented,
}

#[derive(Debug, Clone)]
pub enum MetricEvent {
    QueryReceived {
        protocol: String,
    },
    ResponseSent {
        protocol: String,
        status: ResponseStatus,
    },
    ErrorOccurred {
        protocol: String,
        error_type: String,
    },
    LatencyRecorded {
        protocol: String,
        duration: Duration,
    },
    ConnectionEstablished {
        protocol: String,
    },
    ConnectionClosed {
        protocol: String,
    },
    BytesReceived {
        protocol: String,
        bytes: usize,
    },
    BytesSent {
        protocol: String,
        bytes: usize,
    },
}

pub trait MetricsRecorder: Send + Sync {
    fn record(&self, metrics: &DnsMetrics, event: MetricEvent);
}

pub struct StandardMetricsRecorder;

impl MetricsRecorder for StandardMetricsRecorder {
    fn record(&self, metrics: &DnsMetrics, event: MetricEvent) {
        match event {
            MetricEvent::QueryReceived { protocol } => {
                metrics.increment_queries_by_protocol(&protocol);
            }
            MetricEvent::ResponseSent { protocol, status } => {
                metrics.increment_responses_by_protocol(&protocol);
                match status {
                    ResponseStatus::Success => metrics.increment_successful_queries(),
                    ResponseStatus::CacheHit => metrics.increment_cache_hits(),
                    ResponseStatus::Error => metrics.increment_resolution_errors(),
                    ResponseStatus::RateLimited => metrics.increment_rate_limited(),
                    ResponseStatus::ParseError => metrics.increment_parse_errors(),
                    ResponseStatus::Timeout => metrics.increment_timeouts(),
                    ResponseStatus::Refused => metrics.increment_refused(),
                    ResponseStatus::NotImplemented => metrics.increment_not_implemented(),
                }
            }
            MetricEvent::ErrorOccurred {
                protocol,
                error_type,
            } => {
                metrics.increment_errors_by_type(&protocol, &error_type);
            }
            MetricEvent::LatencyRecorded {
                protocol: _,
                duration,
            } => {
                metrics.record_query_duration(duration);
            }
            MetricEvent::ConnectionEstablished { protocol } => {
                metrics.increment_connections_by_protocol(&protocol);
            }
            MetricEvent::ConnectionClosed { protocol } => {
                metrics.decrement_connections_by_protocol(&protocol);
            }
            MetricEvent::BytesReceived { protocol, bytes } => {
                metrics.add_bytes_received(&protocol, bytes);
            }
            MetricEvent::BytesSent { protocol, bytes } => {
                metrics.add_bytes_sent(&protocol, bytes);
            }
        }
    }
}
