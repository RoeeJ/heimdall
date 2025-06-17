# Heimdall DNS Server Documentation

Welcome to the Heimdall DNS Server documentation. This directory contains all technical documentation organized by category.

## Documentation Structure

### 📦 Deployment
Documentation for deploying Heimdall in various environments.

- [Docker Deployment](deployment/DOCKER.md) - Running Heimdall in Docker containers
- [Kubernetes Deployment](deployment/KUBERNETES.md) - Deploying on Kubernetes (includes Helm, ArgoCD, and Keel)
- [Helm Charts](deployment/HELM.md) - Detailed Helm chart configuration

### 🚀 Features
Core features and capabilities of Heimdall.

- [DNS Blocking](features/DNS_BLOCKING.md) - Blocklist-based DNS filtering
- [DNSSEC](features/DNSSEC.md) - DNSSEC validation implementation
- [Redis L2 Cache](features/REDIS_L2_CACHE.md) - Redis-based second-level caching
- [Replica Coordination](features/REPLICA_COORDINATION.md) - Multi-instance coordination

### 🛠️ Development
Guides for developers working on Heimdall.

- [Testing Guide](development/TESTING_GUIDE.md) - Test coverage, writing tests, and best practices
- [Optimization Guide](development/OPTIMIZATION_GUIDE.md) - Performance optimization strategies and results
- [Technical Debt](development/TECHNICAL_DEBT.md) - Known issues and improvement plans
- [Git Hooks](development/git-hooks.md) - Development workflow automation
- [Test Best Practices](development/test-best-practices.md) - Testing standards and patterns

### 📊 Operations
Operational guides for running Heimdall in production.

- [Performance Tuning](operations/PERFORMANCE_TUNING.md) - Runtime configuration and tuning
- [Observability](operations/OBSERVABILITY.md) - Metrics, monitoring, and alerting
- [Diagnostics](operations/DIAGNOSTICS.md) - Troubleshooting and debugging

### 🔧 Technical
Low-level technical documentation.

- [RFC Compliance](technical/RFC_COMPLIANCE.md) - DNS RFC implementation status
- [RDATA Parsing](technical/RDATA_PARSING.md) - DNS record type parsing details
- [Packet Handling](technical/PACKET_HANDLING.md) - Malformed packet handling
- [UDP Truncation](technical/UDP_TRUNCATION.md) - UDP protocol specifics

## Quick Links

- [Main README](../README.md) - Project overview
- [Architecture](../ARCHITECTURE.md) - System architecture
- [Roadmap](../ROADMAP.md) - Development roadmap
- [CLAUDE.md](../CLAUDE.md) - AI assistant instructions

## Contributing

When adding new documentation:

1. Place it in the appropriate category directory
2. Update this README with a link to your document
3. Follow the existing naming conventions (UPPERCASE.md for major docs)
4. Include a clear title and overview at the top of your document
5. Cross-reference related documents where appropriate