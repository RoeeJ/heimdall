# Heimdall DNS Server Helm Chart

This Helm chart deploys the Heimdall DNS server on a Kubernetes cluster with built-in Prometheus monitoring support.

## Prerequisites

- Kubernetes 1.21+
- Helm 3.0+
- LoadBalancer support in your Kubernetes cluster (for external IP assignment)
- (Optional) Prometheus Operator for automatic monitoring setup

## Installation

### Add the repository (if published)
```bash
helm repo add heimdall https://your-helm-repo-url
helm repo update
```

### Install from local directory
```bash
helm install heimdall ./helm/heimdall
```

### Install with custom values
```bash
helm install heimdall ./helm/heimdall -f my-values.yaml
```

### Install with ArgoCD
Create an Application resource:

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: heimdall
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/RoeeJ/heimdall
    targetRevision: HEAD
    path: helm/heimdall
    helm:
      values: |
        replicaCount: 3
        service:
          type: LoadBalancer
  destination:
    server: https://kubernetes.default.svc
    namespace: dns
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
    - CreateNamespace=true
```

## Configuration

The following table lists the configurable parameters and their default values:

| Parameter | Description | Default |
| --------- | ----------- | ------- |
| `replicaCount` | Number of replicas | `3` |
| `image.repository` | Image repository | `ghcr.io/roeej/heimdall` |
| `image.tag` | Image tag | `latest` |
| `service.type` | Service type | `LoadBalancer` |
| `service.dnsPort` | DNS service port | `53` |
| `service.httpPort` | HTTP metrics port | `8080` |
| `service.dotPort` | DNS-over-TLS port | `853` |
| `service.dohPort` | DNS-over-HTTPS port | `943` |
| `config.upstreamServers` | Upstream DNS servers | `["1.1.1.1:53", "8.8.8.8:53", "8.8.4.4:53"]` |
| `config.cache.enabled` | Enable caching | `true` |
| `config.cache.maxSize` | Maximum cache entries | `10000` |
| `config.rateLimiting.enabled` | Enable rate limiting | `false` |
| `config.dot.enabled` | Enable DNS-over-TLS | `false` |
| `config.dot.bindAddr` | DoT bind address | `"0.0.0.0:853"` |
| `config.dot.tls.autoGenerate` | Auto-generate self-signed certificates | `true` |
| `config.doh.enabled` | Enable DNS-over-HTTPS | `false` |
| `config.doh.bindAddr` | DoH bind address | `"0.0.0.0:943"` |
| `config.doh.path` | DoH endpoint path | `"/dns-query"` |
| `config.doh.tls.autoGenerate` | Auto-generate self-signed certificates | `true` |
| `persistence.enabled` | Enable persistent cache | `true` |
| `persistence.size` | PVC size | `1Gi` |
| `resources.requests.cpu` | CPU request | `100m` |
| `resources.requests.memory` | Memory request | `128Mi` |
| `resources.limits.cpu` | CPU limit | `1000m` |
| `resources.limits.memory` | Memory limit | `512Mi` |
| `redis.enabled` | Enable Redis for distributed caching | `true` |
| `redis.persistence.enabled` | Enable Redis persistence | `true` |
| `redis.persistence.size` | Redis PVC size | `10Gi` |
| `redis.auth.enabled` | Enable Redis authentication | `false` |

## DNS Transport Protocols

Heimdall supports multiple DNS transport protocols for enhanced privacy and security:

### DNS-over-TLS (DoT)
DNS-over-TLS provides encryption for DNS queries using TLS on port 853.

To enable DoT:
```yaml
# values.yaml
config:
  dot:
    enabled: true
    bindAddr: "0.0.0.0:853"
    tls:
      autoGenerate: true  # For testing only
      # For production, disable autoGenerate and provide certificates:
      # autoGenerate: false
      # certPath: "/tls/tls.crt"
      # keyPath: "/tls/tls.key"
```

### DNS-over-HTTPS (DoH)
DNS-over-HTTPS provides encryption for DNS queries using HTTPS on port 943.

To enable DoH:
```yaml
# values.yaml
config:
  doh:
    enabled: true
    bindAddr: "0.0.0.0:943"
    path: "/dns-query"  # RFC 8484 compliant endpoint
    tls:
      autoGenerate: true  # For testing only
      # For production, see TLS certificate section below
```

### TLS Certificate Management

For production deployments, you should provide proper TLS certificates:

1. **Using cert-manager** (recommended):
```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: heimdall-tls
spec:
  secretName: heimdall-tls
  issuerRef:
    name: your-issuer
    kind: ClusterIssuer
  dnsNames:
  - dns.example.com
```

2. **Manual certificate creation**:
```bash
kubectl create secret tls heimdall-tls \
  --cert=path/to/tls.crt \
  --key=path/to/tls.key \
  -n <namespace>
```

Then update values.yaml:
```yaml
config:
  dot:
    enabled: true
    tls:
      autoGenerate: false
      certPath: "/tls/tls.crt"
      keyPath: "/tls/tls.key"
  doh:
    enabled: true
    tls:
      autoGenerate: false
      certPath: "/tls/tls.crt"  # Can share certificates with DoT
      keyPath: "/tls/tls.key"
```

### Testing Encrypted DNS

Test DNS-over-TLS:
```bash
# Using kdig (from knot-dnsutils package)
kdig -d @<EXTERNAL-IP> +tls -p 853 google.com

# Using systemd-resolve
systemd-resolve --tlsa=<EXTERNAL-IP>#853 google.com
```

Test DNS-over-HTTPS:
```bash
# Using curl
curl -H "accept: application/dns-message" \
  "https://<EXTERNAL-IP>:943/dns-query?dns=<base64-encoded-dns-query>"

# Using dog (DNS client with DoH support)
dog google.com --https @https://<EXTERNAL-IP>:943/dns-query
```

## Health Checks and Monitoring

### Important Note on Container Compatibility
The Heimdall container uses a distroless base image for security. This means:
- **No shell access** - exec probes with shell commands won't work
- **No curl/wget** - HTTP checks must use Kubernetes httpGet probes
- **HTTP endpoint required** - Health checks are available on port 8080

### Health Check Endpoints
- `/health` - Basic health check (returns 200 if healthy)
- `/health/detailed` - Detailed health status with component information
- `/metrics` - Prometheus metrics endpoint

### Probe Configuration
All probes should use httpGet:
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: http  # Port 8080
readinessProbe:
  httpGet:
    path: /health
    port: http
```

## Accessing the DNS Server

Once deployed, the LoadBalancer service will be assigned an external IP. You can get it with:

```bash
kubectl get svc heimdall -n <namespace>
```

Configure your clients to use this IP as their DNS server:

```bash
# Test with dig
dig google.com @<EXTERNAL-IP>

# Check health status
curl http://<EXTERNAL-IP>:8080/health

# View detailed health
curl http://<EXTERNAL-IP>:8080/health/detailed

# Access metrics
curl http://<EXTERNAL-IP>:8080/metrics
```

## Monitoring

Heimdall provides comprehensive Prometheus monitoring that is **enabled by default** when Prometheus Operator is installed. The chart automatically detects if Prometheus CRDs are available and creates the appropriate resources.

### Default Configuration

By default, the following monitoring resources are created (if CRDs are available):
- **ServiceMonitor** - For automatic Prometheus scraping
- **PrometheusRule** - Pre-configured alerts for DNS health
- **Grafana Dashboard** - Comprehensive visualization

To disable monitoring:

```yaml
# values.yaml
metrics:
  enabled: false  # Disable all monitoring
  # Or selectively disable components:
  serviceMonitor:
    enabled: false
  prometheusRule:
    enabled: false
  grafanaDashboard:
    enabled: false
```

### Metrics Endpoint

The Heimdall DNS server exposes Prometheus metrics on port 8080:

```bash
# Port-forward to access metrics locally
kubectl port-forward svc/heimdall 8080:8080 -n <namespace>

# Access metrics
curl http://localhost:8080/metrics

# Health check
curl http://localhost:8080/health
```

### Available Metrics

Key metrics exposed by Heimdall:

| Metric | Description | Type |
|--------|-------------|------|
| `heimdall_queries_total` | Total DNS queries by protocol, type, and response code | Counter |
| `heimdall_query_duration_seconds` | DNS query processing duration in seconds | Histogram |
| `heimdall_cache_hits_total` | Total cache hits | Counter |
| `heimdall_cache_misses_total` | Total cache misses | Counter |
| `heimdall_cache_size` | Current number of entries in cache | Gauge |
| `heimdall_cache_hit_rate` | Cache hit rate percentage (0-100) | Gauge |
| `heimdall_error_responses_total` | Total error responses by type | Counter |
| `heimdall_upstream_response_time_seconds` | Upstream DNS response time | Histogram |
| `heimdall_upstream_health_status` | Upstream server health (1=healthy, 0=unhealthy) | Gauge |
| `heimdall_rate_limit_drops_total` | Total queries dropped due to rate limiting | Counter |

### ServiceMonitor (Prometheus Operator)

When Prometheus Operator is installed, the chart automatically creates a ServiceMonitor:

```yaml
metrics:
  serviceMonitor:
    enabled: true
    # Custom scrape interval
    interval: 30s
    # Custom labels for Prometheus selection
    labels:
      prometheus: kube-prometheus
```

### PrometheusRule (Alerting)

Pre-configured alerts are included:

```yaml
metrics:
  prometheusRule:
    enabled: true
    alerts:
      highQueryRate:
        threshold: 1000  # queries/sec
      highErrorRate:
        threshold: 0.05  # 5% error rate
      lowCacheHitRate:
        threshold: 0.5   # 50% cache hit rate
      highResponseTime:
        threshold: 0.5   # 500ms P95
```

### Grafana Dashboard

An auto-discoverable Grafana dashboard is included:

```yaml
metrics:
  grafanaDashboard:
    enabled: true
    # Label that Grafana uses to discover dashboards
    sidecarLabel: "grafana_dashboard"
```

The dashboard provides:
- Query rate and types
- Cache hit rate
- Response time percentiles (P50, P95, P99)
- Error rates by response code
- Upstream failures
- Pod availability

### Custom Monitoring Setup

If not using Prometheus Operator, you can manually configure Prometheus:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'heimdall'
    kubernetes_sd_configs:
      - role: service
    relabel_configs:
      - source_labels: [__meta_kubernetes_service_name]
        regex: heimdall
        action: keep
      - source_labels: [__meta_kubernetes_namespace]
        target_label: namespace
      - source_labels: [__meta_kubernetes_service_name]
        target_label: service
```

## High Availability

The default configuration includes:
- 3 replicas for high availability
- Pod disruption budget ensuring at least 1 pod is always available
- Anti-affinity rules to spread pods across nodes
- Persistent cache storage for each pod
- Optional Redis backend for shared cache across replicas

## Redis Distributed Cache

When Redis is enabled (default), Heimdall uses a two-tier cache system:
- **L1 Cache**: Local in-memory cache for fastest access
- **L2 Cache**: Redis shared cache for cross-replica consistency

Benefits of Redis integration:
- Shared cache improves overall hit rate
- Consistent view of cached data across all replicas
- Survives pod restarts with Redis persistence
- Automatic failover to local-only cache if Redis is unavailable

To disable Redis:
```yaml
redis:
  enabled: false
```

To enable Redis authentication:
```yaml
redis:
  auth:
    enabled: true
    password: "your-secure-password"  # Or use existingSecret
```

## Security

The deployment includes:
- Non-root container execution
- Read-only root filesystem
- Minimal capabilities (only NET_BIND_SERVICE for port 53)
- Security contexts properly configured

## Automatic Updates with Keel

The Heimdall chart includes built-in support for [Keel](https://keel.sh/), which provides automated Kubernetes deployment updates when new container images are available.

### Default Keel Configuration

By default, the chart is configured with the following Keel settings:

| Parameter | Description | Default |
| --------- | ----------- | ------- |
| `keel.annotations."keel.sh/policy"` | Update policy (major/minor/patch/all) | `patch` |
| `keel.annotations."keel.sh/trigger"` | Trigger type (poll/push) | `poll` |
| `keel.annotations."keel.sh/pollSchedule"` | Poll schedule (cron expression) | `@every 5m` |
| `keel.annotations."keel.sh/approvals"` | Number of approvals required | `0` (auto-approve) |
| `keel.annotations."keel.sh/match-tag"` | Match semantic version tags | `true` |

### Customizing Keel Behavior

You can customize Keel's behavior by overriding values:

```yaml
# values.yaml
keel:
  annotations:
    # Only update minor and patch versions
    keel.sh/policy: minor
    # Check for updates every hour
    keel.sh/pollSchedule: "@every 1h"
    # Require manual approval
    keel.sh/approvals: "1"
```

### Disabling Keel

To disable automatic updates, remove the Keel annotations:

```yaml
keel:
  annotations: {}
```

## Upgrading

```bash
helm upgrade heimdall ./helm/heimdall
```

## Uninstalling

```bash
helm uninstall heimdall
```

This will delete all resources created by the chart.