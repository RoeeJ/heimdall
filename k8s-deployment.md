# Kubernetes Deployment Guide for Heimdall DNS Server

This guide provides instructions for deploying Heimdall DNS server on Kubernetes using either Helm or plain Kubernetes manifests.

## Prerequisites

- Kubernetes cluster (1.21+)
- kubectl configured to access your cluster
- LoadBalancer support (for external IP assignment)
- (Optional) Helm 3.0+ for Helm deployment
- (Optional) ArgoCD for GitOps deployment

## Option 1: Deploy with Helm Chart

### Quick Start

```bash
# Clone the repository
git clone https://github.com/RoeeJ/heimdall.git
cd heimdall

# Install with default values
helm install heimdall ./helm/heimdall

# Install with custom namespace
helm install heimdall ./helm/heimdall -n dns-system --create-namespace

# Install with custom values
helm install heimdall ./helm/heimdall -f custom-values.yaml
```

### Custom Values Example

Create a `custom-values.yaml` file:

```yaml
replicaCount: 5

service:
  type: LoadBalancer
  annotations:
    # For AWS NLB
    service.beta.kubernetes.io/aws-load-balancer-type: "nlb"

config:
  upstreamServers:
    - "1.1.1.1:53"
    - "1.0.0.1:53"
  cache:
    maxSize: 20000
  rateLimiting:
    enabled: true
    queriesPerSecondPerIP: 50

resources:
  requests:
    cpu: 200m
    memory: 256Mi
  limits:
    cpu: 2000m
    memory: 1Gi

persistence:
  size: 5Gi
  storageClass: fast-ssd
```

## Option 2: Deploy with Kubernetes Manifests

### Quick Start

```bash
# Apply the manifest
kubectl apply -f k8s-manifest.yaml

# Check deployment status
kubectl get all -n heimdall-dns

# Get the LoadBalancer IP
kubectl get svc heimdall -n heimdall-dns
```

### Customizing the Manifest

Edit `k8s-manifest.yaml` to customize:
- Replica count
- Resource limits
- Environment variables
- Storage size

## Option 3: Deploy with ArgoCD (Automated CI/CD)

### Quick Setup with Automated Updates

```bash
# Run the automated setup script
./scripts/setup-argocd.sh
```

This sets up:
- ✅ ArgoCD Application for Heimdall
- ✅ Automatic image updates when GHA pushes new images  
- ✅ ReplicaSet cleanup (keeps only 3 old versions)
- ✅ Zero-touch deployment pipeline

## Option 4: Automatic Updates with Keel

The Helm chart includes built-in support for [Keel](https://keel.sh/) automatic updates. When deployed with default settings, Keel will:

- ✅ Monitor the container registry for new images
- ✅ Automatically update deployments when new patch versions are released
- ✅ Poll for updates every 5 minutes
- ✅ Match semantic version tags (e.g., v1.2.3)

### Enabling Keel Updates

Keel annotations are included by default in the Helm chart. To customize:

```yaml
# values.yaml
keel:
  annotations:
    keel.sh/policy: minor       # Update minor and patch versions
    keel.sh/pollSchedule: "@every 1h"  # Check hourly
    keel.sh/approvals: "1"      # Require manual approval
```

### Disabling Keel Updates

To disable automatic updates:

```yaml
keel:
  annotations: {}  # Empty annotations disable Keel
```

### Manual ArgoCD Application

If you prefer manual setup, apply the pre-configured application:

```bash
kubectl apply -f .argocd/application.yaml
```

**Features:**
- **Automatic sync** when code changes
- **Image updates** when GHA pushes new images
- **Self-healing** if resources are modified
- **Revision cleanup** to prevent ReplicaSet accumulation

For detailed ArgoCD setup instructions, see [argocd-setup.md](argocd-setup.md).

## Verifying the Deployment

### 1. Check Pod Status

```bash
kubectl get pods -n heimdall-dns
# Should show 3 running pods by default
```

### 2. Get LoadBalancer IP

```bash
kubectl get svc heimdall -n heimdall-dns
# Look for EXTERNAL-IP column
```

### 3. Test DNS Resolution

```bash
# Using dig
dig google.com @<EXTERNAL-IP>

# Using dig (recommended)
dig google.com @<EXTERNAL-IP>

# Using host
host google.com <EXTERNAL-IP>
```

### 4. Check Metrics and Health

```bash
# Port-forward to access locally
kubectl port-forward -n heimdall-dns svc/heimdall 8080:8080

# In another terminal:
# Check health
curl http://localhost:8080/health

# View metrics
curl http://localhost:8080/metrics

# Get cache stats
curl http://localhost:8080/cache/stats
```

## Production Considerations

### High Availability

The default configuration provides:
- **3 replicas** distributed across nodes
- **PodDisruptionBudget** ensuring at least 1 pod is always available
- **Anti-affinity rules** to spread pods across different nodes
- **Persistent cache** for each pod

### Security

- Runs as non-root user (65534)
- Read-only root filesystem
- Minimal capabilities (only NET_BIND_SERVICE)
- Network policies can be added as needed

### Performance Tuning

Adjust these environment variables for your workload:

```yaml
env:
- name: HEIMDALL_WORKER_THREADS
  value: "0"  # 0 = auto-detect CPU cores
- name: HEIMDALL_MAX_CONCURRENT_QUERIES
  value: "1000"
- name: HEIMDALL_MAX_CACHE_SIZE
  value: "10000"
- name: HEIMDALL_ENABLE_PARALLEL_QUERIES
  value: "true"
- name: RUST_LOG
  value: "heimdall=info,warn"  # Production logging (info for operations, debug for troubleshooting)
```

### Logging Levels

For different environments, adjust the `RUST_LOG` environment variable:

```yaml
# Production - minimal logging
- name: RUST_LOG
  value: "heimdall=info,warn"

# Staging/Debug - detailed query logging  
- name: RUST_LOG
  value: "heimdall=debug"

# Deep troubleshooting - full trace logging
- name: RUST_LOG
  value: "heimdall=trace"
```

### Monitoring with Prometheus

If you have Prometheus operator installed:

```yaml
# Enable ServiceMonitor in values.yaml
metrics:
  serviceMonitor:
    enabled: true
    namespace: monitoring
    labels:
      prometheus: kube-prometheus
```

## Troubleshooting

### View Logs

```bash
# All pods
kubectl logs -n heimdall-dns -l app=heimdall

# Specific pod
kubectl logs -n heimdall-dns heimdall-xxxxx

# Follow logs
kubectl logs -n heimdall-dns -l app=heimdall -f
```

### Debug DNS Issues

```bash
# Run a debug pod
kubectl run -n heimdall-dns debug --image=nicolaka/netshoot -it --rm

# Inside the debug pod:
dig google.com @heimdall.heimdall-dns.svc.cluster.local
```

### Common Issues

1. **No External IP**: Ensure your cluster supports LoadBalancer services
2. **Pods not starting**: Check resource limits and node capacity
3. **DNS timeouts**: Check upstream server connectivity
4. **High latency**: Enable cache and parallel queries

## Updating

### Helm

```bash
helm upgrade heimdall ./helm/heimdall -n heimdall-dns
```

### Kubernetes Manifests

```bash
kubectl apply -f k8s-manifest.yaml
```

### Rolling Updates

The deployment uses RollingUpdate strategy by default, ensuring zero downtime.

## Cleanup

### Helm

```bash
helm uninstall heimdall -n heimdall-dns
```

### Kubernetes Manifests

```bash
kubectl delete -f k8s-manifest.yaml
```