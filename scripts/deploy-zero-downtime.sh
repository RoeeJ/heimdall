#!/bin/bash
# Deploy Heimdall with zero-downtime configuration
set -euo pipefail

# Configuration
NAMESPACE="${NAMESPACE:-default}"
RELEASE_NAME="${RELEASE_NAME:-heimdall}"
CHART_PATH="${CHART_PATH:-./helm/heimdall}"
VALUES_FILE="${VALUES_FILE:-./helm/heimdall/values-production.yaml}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}=== Zero-Downtime Heimdall Deployment ===${NC}"

# Function to wait for deployment
wait_for_deployment() {
    echo "Waiting for deployment to be ready..."
    kubectl rollout status deployment/${RELEASE_NAME} -n ${NAMESPACE} --timeout=10m
}

# Function to check current replicas
check_replicas() {
    local ready=$(kubectl get deployment ${RELEASE_NAME} -n ${NAMESPACE} -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
    local desired=$(kubectl get deployment ${RELEASE_NAME} -n ${NAMESPACE} -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")
    echo "Current replicas: ${ready}/${desired}"
}

# Pre-deployment checks
echo -e "\n${YELLOW}Pre-deployment checks...${NC}"
check_replicas

# Ensure we have enough replicas before starting
echo -e "\n${YELLOW}Scaling up to ensure capacity...${NC}"
kubectl scale deployment ${RELEASE_NAME} -n ${NAMESPACE} --replicas=4 || true
sleep 5
wait_for_deployment

# Deploy with zero-downtime settings
echo -e "\n${GREEN}Deploying Heimdall with zero-downtime configuration...${NC}"

helm upgrade --install ${RELEASE_NAME} ${CHART_PATH} \
  --namespace ${NAMESPACE} \
  --values ${VALUES_FILE} \
  --set replicaCount=3 \
  --set podDisruptionBudget.enabled=true \
  --set podDisruptionBudget.minAvailable=2 \
  --set deploymentStrategy.type=RollingUpdate \
  --set deploymentStrategy.rollingUpdate.maxUnavailable=0 \
  --set deploymentStrategy.rollingUpdate.maxSurge=1 \
  --set minReadySeconds=10 \
  --set terminationGracePeriodSeconds=60 \
  --set service.sessionAffinity=ClientIP \
  --set service.externalTrafficPolicy=Local \
  --set service.publishNotReadyAddresses=false \
  --wait \
  --timeout 10m \
  --debug

# Monitor the rollout
echo -e "\n${YELLOW}Monitoring rollout...${NC}"
kubectl rollout status deployment/${RELEASE_NAME} -n ${NAMESPACE} -w

# Post-deployment verification
echo -e "\n${GREEN}Post-deployment verification...${NC}"
check_replicas

# Check endpoints
echo -e "\n${YELLOW}Checking service endpoints...${NC}"
kubectl get endpoints ${RELEASE_NAME} -n ${NAMESPACE} -o wide

# Test DNS resolution
echo -e "\n${YELLOW}Testing DNS resolution...${NC}"
if command -v dig &> /dev/null; then
    SERVICE_IP=$(kubectl get service ${RELEASE_NAME} -n ${NAMESPACE} -o jsonpath='{.status.loadBalancer.ingress[0].ip}' 2>/dev/null || echo "")
    if [ -n "$SERVICE_IP" ]; then
        echo "Testing DNS at ${SERVICE_IP}..."
        dig @${SERVICE_IP} +short google.com || echo "DNS test failed"
    else
        echo "LoadBalancer IP not yet assigned"
    fi
fi

echo -e "\n${GREEN}✓ Deployment complete!${NC}"
echo ""
echo "To test zero-downtime during future deployments, run:"
echo "  ./scripts/test-zero-downtime.sh --vip <YOUR_METALLB_IP> --monitor-deployment"