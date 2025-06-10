#!/bin/bash
set -e

echo "Setting up ArgoCD with automatic image updates for Heimdall..."

# Check if ArgoCD is installed
if ! kubectl get namespace argocd &> /dev/null; then
    echo "Error: ArgoCD namespace not found. Please install ArgoCD first:"
    echo "kubectl create namespace argocd"
    echo "kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml"
    exit 1
fi

# Install ArgoCD Image Updater if not present
if ! kubectl get deployment argocd-image-updater -n argocd &> /dev/null; then
    echo "Installing ArgoCD Image Updater..."
    kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj-labs/argocd-image-updater/stable/manifests/install.yaml
    
    # Wait for the deployment to be ready
    echo "Waiting for ArgoCD Image Updater to be ready..."
    kubectl wait --for=condition=available --timeout=300s deployment/argocd-image-updater -n argocd
else
    echo "ArgoCD Image Updater already installed"
fi

# Apply the Image Updater configuration
echo "Applying Image Updater configuration..."
kubectl apply -f .argocd/image-updater-config.yaml

# Apply the Heimdall Application
echo "Creating Heimdall Application in ArgoCD..."
kubectl apply -f .argocd/application.yaml

# Wait for application to sync
echo "Waiting for initial sync..."
sleep 10

# Show status
echo ""
echo "✅ ArgoCD setup complete!"
echo ""
echo "📋 Next steps:"
echo "1. Access ArgoCD UI:"
echo "   kubectl port-forward svc/argocd-server -n argocd 8080:443"
echo "   https://localhost:8080"
echo ""
echo "2. Get ArgoCD admin password:"
echo "   kubectl -n argocd get secret argocd-initial-admin-secret -o jsonpath='{.data.password}' | base64 -d"
echo ""
echo "3. Check application status:"
echo "   kubectl get applications -n argocd"
echo ""
echo "4. View application details:"
echo "   kubectl describe application heimdall-dns -n argocd"
echo ""
echo "🔄 Automatic updates are now configured!"
echo "When GHA pushes a new image, ArgoCD Image Updater will:"
echo "- Detect the new image"
echo "- Update the deployment"
echo "- Sync the changes automatically"