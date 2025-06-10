# ArgoCD Setup for Heimdall DNS Server

This guide sets up ArgoCD with automatic image updates for continuous deployment of Heimdall.

## 🎯 Overview

**Workflow:**
1. **GHA** builds and pushes image to `ghcr.io/roeej/heimdall:latest`
2. **ArgoCD Image Updater** detects the new image
3. **ArgoCD** automatically updates the deployment
4. **Old ReplicaSets** are cleaned up (only keeps 3 latest)

## 📋 Prerequisites

- Kubernetes cluster with ArgoCD installed
- kubectl configured for your cluster
- GitHub Container Registry access (public repo)

## 🚀 Quick Setup

### Option 1: Automated Setup

```bash
# Run the setup script
./scripts/setup-argocd.sh
```

### Option 2: Manual Setup

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

## 🔧 Configuration Details

### Automatic Image Updates

The setup includes:

- **Image monitoring**: Watches `ghcr.io/roeej/heimdall:latest`
- **Update strategy**: Latest tag
- **Sync policy**: Automated with self-healing
- **Tag filtering**: Accepts `latest` and `main-*` tags

### ReplicaSet Cleanup

- **Deployment**: `revisionHistoryLimit: 3`
- **ArgoCD Application**: `revisionHistoryLimit: 3`
- **Result**: Only keeps 3 old ReplicaSets instead of default 10

### Update Annotations

```yaml
# These annotations enable automatic updates
argocd-image-updater.argoproj.io/image-list: heimdall=ghcr.io/roeej/heimdall
argocd-image-updater.argoproj.io/heimdall.update-strategy: latest
argocd-image-updater.argoproj.io/heimdall.allow-tags: regexp:^main-.*$|^latest$
```

## 🖥️ Accessing ArgoCD

### Get Admin Password
```bash
kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath='{.data.password}' | base64 -d
```

### Access UI
```bash
# Port forward to ArgoCD server
kubectl port-forward svc/argocd-server -n argocd 8080:443

# Open browser to https://localhost:8080
# Username: admin
# Password: (from command above)
```

## 📊 Monitoring Deployment

### Check Application Status
```bash
# List applications
kubectl get applications -n argocd

# Describe Heimdall application
kubectl describe application heimdall-dns -n argocd

# Check sync status
kubectl get application heimdall-dns -n argocd -o jsonpath='{.status.sync.status}'
```

### Check Image Updater Logs
```bash
# View image updater logs
kubectl logs -n argocd deployment/argocd-image-updater -f

# Check for update activities
kubectl logs -n argocd deployment/argocd-image-updater | grep heimdall
```

### Check Deployment Status
```bash
# Check pods in target namespace
kubectl get pods -n dns-system

# Check ReplicaSets (should only see 3 max)
kubectl get rs -n dns-system

# View deployment rollout status
kubectl rollout status deployment/heimdall -n dns-system
```

## 🔄 How Automatic Updates Work

1. **GHA Trigger**: Code push triggers GitHub Actions
2. **Image Build**: GHA builds and pushes to `ghcr.io/roeej/heimdall:latest`
3. **Image Detection**: ArgoCD Image Updater polls registry every 2 minutes
4. **Update Trigger**: New image SHA detected → triggers update
5. **Deployment Update**: ArgoCD updates the Helm values with new image
6. **Rolling Update**: Kubernetes performs rolling update with zero downtime
7. **Cleanup**: Old ReplicaSets beyond limit (3) are automatically deleted

## 🛠️ Customization

### Change Update Frequency

Edit the Image Updater config:
```yaml
# In .argocd/image-updater-config.yaml
data:
  interval: "30s"  # Check for updates every 30 seconds (default: 2m)
```

### Change Tag Strategy

For specific tags instead of latest:
```yaml
# In .argocd/application.yaml
annotations:
  argocd-image-updater.argoproj.io/heimdall.update-strategy: semver
  argocd-image-updater.argoproj.io/heimdall.allow-tags: regexp:^v[0-9]+\.[0-9]+\.[0-9]+$
```

### Adjust ReplicaSet Retention

In `helm/heimdall/values.yaml`:
```yaml
revisionHistoryLimit: 5  # Keep 5 old ReplicaSets instead of 3
```

## 🚨 Troubleshooting

### Application Not Syncing
```bash
# Force sync
kubectl patch application heimdall-dns -n argocd --type merge -p='{"operation":{"initiatedBy":{"username":"admin"},"sync":{"syncStrategy":{"hook":{},"apply":{"force":true}}}}}'

# Or use ArgoCD CLI
argocd app sync heimdall-dns
```

### Image Updates Not Working
```bash
# Check Image Updater logs
kubectl logs -n argocd deployment/argocd-image-updater | grep -E "(error|heimdall)"

# Verify registry access
kubectl run test-pull --image=ghcr.io/roeej/heimdall:latest --rm -it --restart=Never
```

### Too Many ReplicaSets
```bash
# Manual cleanup (if needed)
kubectl delete rs -n dns-system $(kubectl get rs -n dns-system -o name | grep heimdall | tail -n +4)
```

## 🔐 Security Considerations

### Private Registries

If using private registries, create pull secrets:
```bash
kubectl create secret docker-registry ghcr-credentials \
  --docker-server=ghcr.io \
  --docker-username=your-username \
  --docker-password=your-token \
  --namespace=argocd
```

### RBAC

ArgoCD Image Updater needs permissions to:
- Read/write Application resources
- Access container registries
- Update Git repositories (if using Git write-back)

## 📈 Benefits

✅ **Zero-touch deployment**: Push code → automatic deployment  
✅ **Resource cleanup**: No accumulation of old ReplicaSets  
✅ **GitOps compliance**: All changes tracked in Git  
✅ **Rollback capability**: Easy rollback through ArgoCD UI  
✅ **Monitoring**: Full visibility through ArgoCD dashboard  

Your CI/CD pipeline is now fully automated! 🎉