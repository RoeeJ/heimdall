# MetalLB Zero-Downtime Deployment Guide for Heimdall

This guide ensures MetalLB only routes traffic to healthy, active replicas during Kubernetes deployments.

## Problem
During deployment rotations, MetalLB may route traffic to pods that are:
- Still starting up
- In the process of shutting down
- Not yet ready to handle DNS queries

## Solution Components

### 1. Update Helm Values

Add these configurations to your `values.yaml` or override them during deployment:

```yaml
# Ensure multiple replicas for high availability
replicaCount: 3

# Pod Disruption Budget - Maintain minimum availability
podDisruptionBudget:
  enabled: true
  minAvailable: 2  # Or use maxUnavailable: 1

# Enhanced Readiness Probe
readinessProbe:
  exec:
    command:
    - /bin/sh
    - -c
    - |
      # Test actual DNS resolution capability
      dig @127.0.0.1 -p 1053 +short google.com A && \
      dig @127.0.0.1 -p 1053 +tcp +short google.com A
  initialDelaySeconds: 5
  periodSeconds: 2
  timeoutSeconds: 3
  successThreshold: 2  # Require 2 successful checks
  failureThreshold: 2  # Fail fast to remove from load balancer

# Startup Probe for slow starts
startupProbe:
  httpGet:
    path: /health
    port: http
  initialDelaySeconds: 5
  periodSeconds: 2
  timeoutSeconds: 5
  failureThreshold: 30  # Allow up to 60 seconds for startup

# Keep liveness probe less aggressive
livenessProbe:
  httpGet:
    path: /health
    port: http
  initialDelaySeconds: 30
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 5

# Deployment Strategy
deploymentStrategy:
  type: RollingUpdate
  rollingUpdate:
    maxSurge: 1
    maxUnavailable: 0  # Never remove pods until new ones are ready

# Service Configuration
service:
  type: LoadBalancer
  # Important: Add session affinity to prevent mid-query disruptions
  sessionAffinity: ClientIP
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 10800  # 3 hours
  
  # MetalLB-specific annotations
  annotations:
    # Optional: Use specific address pool
    # metallb.universe.tf/address-pool: production-public-ips
    
    # Optional: Use Layer 2 mode for faster failover
    # metallb.universe.tf/mode: layer2
```

### 2. Create Custom Health Check Script

Create a ConfigMap with a proper DNS health check:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: heimdall-health-scripts
data:
  dns-health-check.sh: |
    #!/bin/sh
    set -e
    
    # Test UDP DNS
    timeout 2 dig @127.0.0.1 -p 1053 +short +time=1 +tries=1 health.local A > /dev/null 2>&1 || exit 1
    
    # Test TCP DNS
    timeout 2 dig @127.0.0.1 -p 1053 +tcp +short +time=1 +tries=1 health.local A > /dev/null 2>&1 || exit 1
    
    # Check cache is working (optional)
    timeout 1 wget -q -O - http://127.0.0.1:8080/health | grep -q "ok" || exit 1
    
    exit 0
```

### 3. Enhanced Deployment Configuration

Add to your Helm deployment:

```yaml
# values-production.yaml
# Graceful shutdown
terminationGracePeriodSeconds: 60

# Anti-affinity to spread pods across nodes
affinity:
  podAntiAffinity:
    preferredDuringSchedulingIgnoredDuringExecution:
    - weight: 100
      podAffinityTerm:
        labelSelector:
          matchExpressions:
          - key: app.kubernetes.io/name
            operator: In
            values:
            - heimdall
        topologyKey: kubernetes.io/hostname

# Priority class for DNS service
priorityClassName: system-cluster-critical

# Volume mount for health check script
volumeMounts:
  - name: health-scripts
    mountPath: /scripts
    readOnly: true

volumes:
  - name: health-scripts
    configMap:
      name: heimdall-health-scripts
      defaultMode: 0755

# Updated probes using the script
readinessProbe:
  exec:
    command: ["/scripts/dns-health-check.sh"]
  initialDelaySeconds: 5
  periodSeconds: 2
  timeoutSeconds: 3
  successThreshold: 2
  failureThreshold: 2
```

### 4. MetalLB-Specific Service Configuration

Create a separate service configuration for MetalLB:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: heimdall-lb
  annotations:
    # MetalLB annotations
    metallb.universe.tf/address-pool: dns-pool
    metallb.universe.tf/allow-shared-ip: "dns-vip"
    
    # Health check annotations (if using cloud LB)
    service.beta.kubernetes.io/aws-load-balancer-healthcheck-interval: "2"
    service.beta.kubernetes.io/aws-load-balancer-healthcheck-timeout: "2"
    service.beta.kubernetes.io/aws-load-balancer-healthcheck-healthy-threshold: "2"
    service.beta.kubernetes.io/aws-load-balancer-healthcheck-unhealthy-threshold: "2"
spec:
  type: LoadBalancer
  loadBalancerIP: 10.0.0.53  # Your DNS VIP
  
  # Important: Only route to ready endpoints
  publishNotReadyAddresses: false
  
  # Session affinity for DNS queries
  sessionAffinity: ClientIP
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 10800
  
  # External traffic policy for better performance
  externalTrafficPolicy: Local
  
  ports:
  - name: dns-udp
    port: 53
    targetPort: 1053
    protocol: UDP
  - name: dns-tcp  
    port: 53
    targetPort: 1053
    protocol: TCP
    
  selector:
    app.kubernetes.io/name: heimdall
    app.kubernetes.io/instance: heimdall
```

### 5. Deployment Command

Deploy with zero-downtime settings:

```bash
# First, scale up to ensure capacity
kubectl scale deployment heimdall --replicas=4

# Deploy with careful settings
helm upgrade heimdall ./helm/heimdall \
  --set replicaCount=3 \
  --set podDisruptionBudget.enabled=true \
  --set podDisruptionBudget.minAvailable=2 \
  --set deploymentStrategy.rollingUpdate.maxUnavailable=0 \
  --set deploymentStrategy.rollingUpdate.maxSurge=1 \
  --set service.sessionAffinity=ClientIP \
  --wait \
  --timeout 10m

# Monitor the rollout
kubectl rollout status deployment/heimdall -w
```

### 6. Testing Zero-Downtime Deployment

Create a test script to verify no DNS interruptions:

```bash
#!/bin/bash
# test-dns-availability.sh

DNS_VIP="10.0.0.53"  # Your MetalLB VIP
TEST_DOMAIN="google.com"
INTERVAL=0.1

echo "Testing DNS availability during deployment..."
echo "Press Ctrl+C to stop"

SUCCESS=0
FAILURE=0

while true; do
    if timeout 1 dig @${DNS_VIP} +short ${TEST_DOMAIN} > /dev/null 2>&1; then
        SUCCESS=$((SUCCESS + 1))
        echo -ne "\rSuccess: $SUCCESS | Failures: $FAILURE"
    else
        FAILURE=$((FAILURE + 1))
        echo -ne "\rSuccess: $SUCCESS | Failures: $FAILURE"
        echo -e "\n$(date): DNS query failed!"
    fi
    sleep $INTERVAL
done
```

### 7. MetalLB Configuration

Ensure your MetalLB configuration supports health checking:

```yaml
apiVersion: metallb.io/v1beta1
kind: IPAddressPool
metadata:
  name: dns-pool
  namespace: metallb-system
spec:
  addresses:
  - 10.0.0.53/32  # Your DNS VIP

---
apiVersion: metallb.io/v1beta1
kind: L2Advertisement
metadata:
  name: dns-advertisement
  namespace: metallb-system
spec:
  ipAddressPools:
  - dns-pool
  # Optional: Specify interfaces
  # interfaces:
  # - eth0
```

## Best Practices

1. **Always use multiple replicas** (minimum 3 for production)
2. **Set `maxUnavailable: 0`** in rolling update strategy
3. **Use proper readiness probes** that test actual DNS functionality
4. **Enable session affinity** to prevent query disruptions
5. **Set `publishNotReadyAddresses: false`** (default) in Service
6. **Use `externalTrafficPolicy: Local`** for better performance
7. **Monitor deployment with continuous DNS queries** during updates

## Troubleshooting

If you still experience connectivity issues:

1. Check pod readiness:
   ```bash
   kubectl get pods -l app.kubernetes.io/name=heimdall -o wide
   kubectl describe endpoints heimdall
   ```

2. Verify MetalLB speaker logs:
   ```bash
   kubectl logs -n metallb-system -l app=metallb,component=speaker
   ```

3. Test DNS from within cluster:
   ```bash
   kubectl run -it --rm debug --image=nicolaka/netshoot --restart=Never -- \
     dig @heimdall.default.svc.cluster.local google.com
   ```

4. Check service endpoints:
   ```bash
   kubectl get endpoints heimdall -o yaml
   ```

## Alternative: Using headless service with MetalLB

For even better control, you can use a headless service with MetalLB BGP mode, which provides per-pod IPs and allows for more granular health checking by upstream routers.