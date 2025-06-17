# Kubernetes Deployment Guide for Heimdall DNS Server

This comprehensive guide covers deploying Heimdall DNS server on Kubernetes using Helm, plain manifests, and GitOps with ArgoCD.

## Prerequisites

- Kubernetes cluster (1.21+)
- kubectl configured to access your cluster
- LoadBalancer support (for external IP assignment)
- (Optional) Helm 3.0+ for Helm deployment
- (Optional) ArgoCD for GitOps deployment

## Deployment Options

### Option 1: Deploy with Helm Chart (Recommended)

#### Quick Start

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

#### Custom Values Example

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

For a complete list of configuration options, see the [Helm Chart documentation](HELM.md).

### Option 2: Deploy with Kubernetes Manifests

#### Quick Start

```bash
# Apply the manifest
kubectl apply -f k8s-manifest.yaml

# Check deployment status
kubectl get all -n heimdall-dns

# Get the LoadBalancer IP
kubectl get svc heimdall -n heimdall-dns
```

#### Customizing the Manifest

Edit `k8s-manifest.yaml` to customize:
- Replica count
- Resource limits
- Environment variables
- Storage size

### Option 3: GitOps with ArgoCD (Automated CI/CD)

#### 🎯 Overview

**Automated Workflow:**
1. **GitHub Actions** builds and pushes image to `ghcr.io/roeej/heimdall:latest`
2. **ArgoCD Image Updater** detects the new image
3. **ArgoCD** automatically updates the deployment
4. **Old ReplicaSets** are cleaned up (only keeps 3 latest)

#### 🚀 Quick Setup

##### Automated Setup (Recommended)

```bash
# Run the automated setup script
./scripts/setup-argocd.sh
```

This sets up:
- ✅ ArgoCD Application for Heimdall
- ✅ Automatic image updates when GHA pushes new images  
- ✅ ReplicaSet cleanup (keeps only 3 old versions)
- ✅ Zero-touch deployment pipeline

##### Manual Setup

1. **Install ArgoCD** (if not already installed):
```bash
kubectl create namespace argocd
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml
```

2. **Install ArgoCD Image Updater**:
```bash
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj-labs/argocd-image-updater/stable/manifests/install.yaml
```

3. **Apply configurations**:
```bash
kubectl apply -f .argocd/image-updater-config.yaml
kubectl apply -f .argocd/application.yaml
```

#### 🖥️ Accessing ArgoCD

```bash
# Get admin password
kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath='{.data.password}' | base64 -d

# Port forward to ArgoCD server
kubectl port-forward svc/argocd-server -n argocd 8080:443

# Access UI at https://localhost:8080
# Username: admin
# Password: (from command above)
```

#### 🔄 How Automatic Updates Work

1. **Code Push**: Triggers GitHub Actions
2. **Image Build**: GHA builds and pushes to `ghcr.io/roeej/heimdall:latest`
3. **Detection**: ArgoCD Image Updater polls registry every 2 minutes
4. **Update**: New image SHA triggers deployment update
5. **Rolling Update**: Kubernetes performs zero-downtime update
6. **Cleanup**: Old ReplicaSets beyond limit (3) are deleted

### Option 4: Automatic Updates with Keel

The Helm chart includes built-in support for [Keel](https://keel.sh/) automatic updates. When deployed with default settings, Keel will:

- ✅ Monitor the container registry for new images
- ✅ Automatically update deployments when new patch versions are released
- ✅ Poll for updates every 5 minutes
- ✅ Match semantic version tags (e.g., v1.2.3)

#### Enabling Keel Updates

Keel annotations are included by default in the Helm chart. To customize:

```yaml
# values.yaml
keel:
  annotations:
    keel.sh/policy: minor       # Update minor and patch versions
    keel.sh/pollSchedule: "@every 1h"  # Check hourly
    keel.sh/approvals: "1"      # Require manual approval
```

#### Disabling Keel Updates

To disable automatic updates:

```yaml
keel:
  annotations: {}  # Empty annotations disable Keel
```

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

# Using host
host google.com <EXTERNAL-IP>

# Test with specific record types
dig AAAA google.com @<EXTERNAL-IP>
dig MX gmail.com @<EXTERNAL-IP>
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
  value: "heimdall=info,warn"  # Production logging
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

### ArgoCD Specific Troubleshooting

#### Application Not Syncing
```bash
# Force sync
kubectl patch application heimdall-dns -n argocd --type merge -p='{"operation":{"initiatedBy":{"username":"admin"},"sync":{"syncStrategy":{"hook":{},"apply":{"force":true}}}}}'

# Or use ArgoCD CLI
argocd app sync heimdall-dns
```

#### Image Updates Not Working
```bash
# Check Image Updater logs
kubectl logs -n argocd deployment/argocd-image-updater | grep -E "(error|heimdall)"

# Verify registry access
kubectl run test-pull --image=ghcr.io/roeej/heimdall:latest --rm -it --restart=Never
```

#### Too Many ReplicaSets
```bash
# Manual cleanup (if needed)
kubectl delete rs -n dns-system $(kubectl get rs -n dns-system -o name | grep heimdall | tail -n +4)
```

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

## Advanced Configuration

### DNS-over-TLS (DoT) and DNS-over-HTTPS (DoH)

Heimdall supports encrypted DNS protocols. To enable:

```yaml
# values.yaml
config:
  dot:
    enabled: true
    bindAddr: "0.0.0.0:853"
  doh:
    enabled: true
    bindAddr: "0.0.0.0:943"
```

### Redis Distributed Cache

Enable Redis for shared cache across replicas:

```yaml
redis:
  enabled: true
  persistence:
    enabled: true
    size: 10Gi
```

For detailed configuration options, see the [Helm Chart documentation](HELM.md).

## Cleanup

### Helm

```bash
helm uninstall heimdall -n heimdall-dns
```

### Kubernetes Manifests

```bash
kubectl delete -f k8s-manifest.yaml
```

### ArgoCD

```bash
kubectl delete application heimdall-dns -n argocd
```

## Benefits of Each Deployment Method

### Helm
✅ **Templatable configuration**  
✅ **Easy upgrades and rollbacks**  
✅ **Built-in best practices**  
✅ **Reusable across environments**  

### ArgoCD + Image Updater
✅ **Zero-touch deployment**: Push code → automatic deployment  
✅ **Resource cleanup**: No accumulation of old ReplicaSets  
✅ **GitOps compliance**: All changes tracked in Git  
✅ **Rollback capability**: Easy rollback through ArgoCD UI  
✅ **Monitoring**: Full visibility through ArgoCD dashboard  

### Keel
✅ **Simple setup**: Just annotations on deployments  
✅ **Flexible policies**: Control update behavior  
✅ **No additional infrastructure**: Works with existing deployments  
✅ **Manual approval options**: For production safety