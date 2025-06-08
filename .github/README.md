# Heimdall DNS Server - CI/CD Documentation

This directory contains the GitHub Actions workflows and configuration for the Heimdall DNS server project.

## Workflows

### Main CI/CD Pipeline (`ci.yml`)

The primary CI/CD pipeline runs on every push to `master` and on pull requests. It includes:

- **Test Suite**: Code formatting, linting, unit tests, and documentation tests across multiple Rust versions
- **Security Audit**: Vulnerability scanning with `cargo audit`
- **Performance Regression**: Automated performance testing using the existing `./scripts/check_performance.sh`
- **Multi-platform Builds**: Compilation for Linux, Windows, and macOS in both debug and release modes
- **Integration Tests**: Real DNS server testing with system dependencies
- **Docker Build**: Automated container builds and pushes to GitHub Container Registry
- **Code Coverage**: Coverage reporting with Codecov integration
- **Release Management**: Automated releases for version tags

### Nightly Tests (`nightly.yml`)

Extended testing suite that runs daily:

- **Nightly Rust**: Testing with Rust nightly toolchain
- **Extended Stress Tests**: Long-running performance and endurance tests
- **Memory Leak Detection**: Valgrind-based memory analysis
- **Compatibility Testing**: Testing across multiple OS versions and Rust versions

### Performance Baseline Updates (`performance-baseline.yml`)

Weekly automated baseline updates:

- **Scheduled Updates**: Automatic weekly baseline refresh
- **Manual Triggers**: On-demand baseline updates with reason tracking
- **Baseline Management**: Automatic commit and push of new baselines

## Configuration Files

### Dependabot (`dependabot.yml`)

Automated dependency updates for:

- **Rust Dependencies**: Weekly Cargo.toml updates with intelligent grouping
- **GitHub Actions**: Weekly workflow dependency updates
- **Docker Images**: Base image security updates

### Pull Request Template (`PULL_REQUEST_TEMPLATE.md`)

Standardized PR template ensuring:

- **Performance Impact Assessment**: Mandatory performance consideration
- **Security Review**: Security implications checklist
- **Testing Requirements**: Comprehensive test verification
- **Documentation Updates**: Ensuring docs stay current

## Key Features

### Performance-First Design

- **Regression Prevention**: Automatic performance regression detection
- **Baseline Management**: Systematic performance baseline updates
- **Comprehensive Benchmarking**: Integration with existing Criterion benchmarks

### Security Integration

- **Vulnerability Scanning**: Automated dependency security audits
- **Container Security**: Multi-stage Docker builds with non-root users
- **Supply Chain Security**: Dependabot integration with security labeling

### Multi-Architecture Support

- **Cross-Platform Builds**: Linux, Windows, macOS support
- **Container Images**: Multi-architecture Docker images (amd64, arm64)
- **Release Artifacts**: Platform-specific binary distributions

### Advanced Testing

- **SIMD Testing**: Specialized testing for SIMD operations with Miri
- **Memory Safety**: Valgrind integration for leak detection
- **Endurance Testing**: Long-running stress tests for stability verification

## Usage

### Running Performance Tests Locally

```bash
# Run performance regression tests
./scripts/check_performance.sh

# Create new baseline
./scripts/check_performance.sh --create-baseline

# Custom regression threshold
./scripts/check_performance.sh --max-regression 5.0
```

### Manual Workflow Triggers

All workflows support manual triggering through the GitHub Actions UI:

1. Navigate to Actions tab in GitHub
2. Select the desired workflow
3. Click "Run workflow"
4. Provide any required inputs

### Docker Usage

```bash
# Pull the latest image
docker pull ghcr.io/roeej/heimdall:latest

# Run the DNS server
docker run -p 1053:1053/udp ghcr.io/roeej/heimdall:latest

# Test the server
dig google.com @127.0.0.1 -p 1053
```

## Troubleshooting

### Performance Test Failures

If performance tests fail:

1. Check if the regression is expected (new features, optimizations)
2. Review the performance baseline age
3. Consider updating the baseline if changes are intentional
4. Investigate unexpected regressions in the code

### Build Failures

Common issues:

- **Dependency Conflicts**: Check Dependabot PRs for conflicts
- **Rust Version Issues**: Verify MSRV compatibility
- **Platform-Specific Bugs**: Check matrix build results

### Docker Issues

- **Registry Access**: Ensure `GITHUB_TOKEN` has package permissions
- **Multi-arch Builds**: Check buildx setup and platform support
- **Image Size**: Monitor image size growth over time

## Best Practices

### Performance Management

1. **Baseline Updates**: Update baselines after significant optimizations
2. **Regression Thresholds**: Keep thresholds tight (10-15%) to catch issues early
3. **Performance Reviews**: Include performance impact in all PR reviews

### Security Practices

1. **Dependency Updates**: Review and test Dependabot PRs promptly
2. **Vulnerability Response**: Address security audit failures immediately
3. **Container Security**: Regularly update base images and scan results

### Release Management

1. **Semantic Versioning**: Use proper version tags for releases
2. **Release Notes**: Maintain clear changelog and release notes
3. **Binary Testing**: Test release binaries before distribution

This CI/CD setup provides a robust foundation for maintaining code quality, performance, and security standards while supporting the advanced features and performance requirements of the Heimdall DNS server.