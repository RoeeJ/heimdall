# Heimdall DNS Server Helm Chart

This Helm chart deploys the Heimdall DNS server on a Kubernetes cluster.

## Prerequisites

- Kubernetes 1.21+
- Helm 3.0+
- LoadBalancer support in your Kubernetes cluster (for external IP assignment)

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
| `config.upstreamServers` | Upstream DNS servers | `["1.1.1.1:53", "8.8.8.8:53", "8.8.4.4:53"]` |
| `config.cache.enabled` | Enable caching | `true` |
| `config.cache.maxSize` | Maximum cache entries | `10000` |
| `config.rateLimiting.enabled` | Enable rate limiting | `false` |
| `persistence.enabled` | Enable persistent cache | `true` |
| `persistence.size` | PVC size | `1Gi` |
| `resources.requests.cpu` | CPU request | `100m` |
| `resources.requests.memory` | Memory request | `128Mi` |
| `resources.limits.cpu` | CPU limit | `1000m` |
| `resources.limits.memory` | Memory limit | `512Mi` |

## Accessing the DNS Server

Once deployed, the LoadBalancer service will be assigned an external IP. You can get it with:

```bash
kubectl get svc heimdall -n <namespace>
```

Configure your clients to use this IP as their DNS server:

```bash
# Test with dig
dig google.com @<EXTERNAL-IP>

# Test with nslookup
nslookup google.com <EXTERNAL-IP>
```

## Monitoring

The Heimdall DNS server exposes Prometheus metrics on port 8080:

```bash
# Port-forward to access metrics locally
kubectl port-forward svc/heimdall 8080:8080 -n <namespace>

# Access metrics
curl http://localhost:8080/metrics

# Health check
curl http://localhost:8080/health
```

## High Availability

The default configuration includes:
- 3 replicas for high availability
- Pod disruption budget ensuring at least 1 pod is always available
- Anti-affinity rules to spread pods across nodes
- Persistent cache storage for each pod

## Security

The deployment includes:
- Non-root container execution
- Read-only root filesystem
- Minimal capabilities (only NET_BIND_SERVICE for port 53)
- Security contexts properly configured

## Upgrading

```bash
helm upgrade heimdall ./helm/heimdall
```

## Uninstalling

```bash
helm uninstall heimdall
```

This will delete all resources created by the chart.