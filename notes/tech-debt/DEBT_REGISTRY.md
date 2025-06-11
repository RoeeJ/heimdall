# Technical Debt Registry

**Purpose**: Track specific technical debt items with actionable remediation plans  
**Updated**: 2025-01-06  

## Item Format

Each debt item includes:
- **ID**: Unique identifier
- **Priority**: Critical/High/Medium
- **Category**: Type of technical debt
- **Impact**: Business/technical impact
- **Effort**: Estimated remediation time
- **Risk**: What happens if not fixed
- **Location**: Specific files/functions
- **Action Plan**: Concrete steps to resolve

---

## DEBT-001: Unsafe Unwrap Calls in Configuration

**Priority**: 🔴 Critical  
**Category**: Safety  
**Impact**: Runtime panics on invalid configuration  
**Effort**: 2 days  
**Risk**: Production server crashes on startup/config reload  

**Locations**:
- `src/config.rs:69-79,95` - Hard-coded address parsing
- `src/rate_limiter.rs:79,80,169,170,182,194` - NonZeroU32 creation
- `src/cluster_registry.rs:100,107,134,141,201` - Time/JSON operations

**Action Plan**:
1. Replace config defaults with `const` validated addresses
2. Add config validation function that returns `Result<DnsConfig, ConfigError>`
3. Use `expect()` with descriptive messages for truly safe operations
4. Add startup-time validation for all rate limiting parameters

**Acceptance Criteria**:
- Zero `.unwrap()` calls in config parsing
- Descriptive error messages for invalid configs
- Unit tests for all validation edge cases

---

## DEBT-002: Environment Variable Parsing Vulnerabilities

**Priority**: 🔴 Critical  
**Category**: Configuration/Security  
**Impact**: Silent config corruption, potential security issues  
**Effort**: 3 days  
**Risk**: Invalid configs accepted silently, server misconfiguration  

**Location**: `src/config.rs:102-231` (DnsConfig::from_env)

**Specific Issues**:
- `.parse().unwrap_or(true)` swallows parse errors for booleans
- No validation of numeric ranges (ports, timeouts, cache sizes)
- No conflict detection between related settings

**Action Plan**:
1. Create `ConfigValidator` trait with range checking
2. Implement `TryFrom<HashMap<String, String>>` for DnsConfig
3. Add warning logging for invalid environment variables
4. Validate cross-field dependencies (e.g., cache size vs available memory)

**Acceptance Criteria**:
- All env vars validated with helpful error messages
- Range checks for all numeric values
- Warning logs for invalid values with fallback behavior
- Integration tests for config validation edge cases

---

## DEBT-003: Inconsistent Error Handling

**Priority**: 🟡 High  
**Category**: Architecture  
**Impact**: Difficult debugging, inconsistent error responses  
**Effort**: 4 days  
**Risk**: Poor error diagnostics, maintenance complexity  

**Locations**:
- `src/server.rs` - Uses `Box<dyn Error>`
- `src/resolver.rs` - Uses custom `DnsError`
- `src/cache/` - Mixed error handling approaches

**Action Plan**:
1. Standardize on `thiserror` for all error types
2. Create error conversion traits between modules
3. Add error context preservation with `.with_context()`
4. Implement consistent error logging patterns

**Acceptance Criteria**:
- Single error handling pattern across codebase
- Error chains preserved with full context
- Consistent error response format
- Error handling style guide documented

---

## DEBT-004: DNS Parsing Code Duplication

**Priority**: 🟡 High  
**Category**: Maintainability  
**Impact**: Bug-prone maintenance, inconsistent behavior  
**Effort**: 3 days  
**Risk**: Bugs fixed in one place but not others  

**Location**: `src/dns/resource.rs` - Record field extraction methods

**Duplicated Patterns**:
- SOA/SRV/CAA field parsing all use similar split/parse logic
- Domain name parsing duplicated across modules
- Validation patterns repeated

**Action Plan**:
1. Create `RecordFieldParser` trait with generic implementations
2. Extract common domain parsing utilities to `dns::utils`
3. Consolidate validation logic into shared module
4. Add macro for repetitive field parsing patterns

**Acceptance Criteria**:
- Single implementation for each parsing pattern
- Shared test suite for all record types
- Consistent error handling across record parsers
- Documentation for extending with new record types

---

## DEBT-005: Excessive Memory Allocations

**Priority**: 🟡 High  
**Category**: Performance  
**Impact**: Higher memory usage, slower response times  
**Effort**: 5 days  
**Risk**: Performance degradation under load  

**High-Impact Locations**:
- `src/cache/mod.rs:47,88` - Domain normalization allocations
- `src/resolver.rs` - Multiple Arc::clone operations
- `src/dns/resource.rs:51,52,84` - String allocations in hot paths

**Action Plan**:
1. Implement `Cow<str>` for conditional string allocation
2. Create object pool for DNS packet buffers
3. Replace `.clone()` with `Arc::clone` where appropriate
4. Pre-allocate common buffer sizes
5. Profile allocation hotspots with memory profiler

**Acceptance Criteria**:
- 30% reduction in allocation rate under load
- Benchmark showing improved response times
- Memory usage stable under sustained load
- Object pooling implemented for DNS packets

---

## DEBT-006: Missing Configuration Validation

**Priority**: 🟡 High  
**Category**: Reliability  
**Impact**: Runtime failures, security vulnerabilities  
**Effort**: 2 days  
**Risk**: Invalid configs cause server instability  

**Missing Validations**:
- Port ranges (1-65535)
- TTL values within RFC limits
- Thread counts reasonable for system
- Cache sizes not exceeding available memory
- Timeout values positive and reasonable

**Action Plan**:
1. Implement `Validate` trait for all config structs
2. Add bounds checking for all numeric config values
3. Cross-validate related settings (e.g., timeouts vs retries)
4. Create config test generator for edge cases

**Acceptance Criteria**:
- All config values validated at load time
- Helpful error messages for invalid ranges
- Related settings validated together
- Config validation test coverage >95%

---

## DEBT-007: Test Coverage Gaps

**Priority**: 🟡 High  
**Category**: Quality Assurance  
**Impact**: Reduced confidence in reliability  
**Effort**: 4 days  
**Risk**: Bugs in production, difficult refactoring  

**Critical Missing Tests**:
- Configuration validation edge cases
- Redis connection failure scenarios
- DNS packet truncation handling
- Rate limiter accuracy under high load
- Error recovery in network failures

**Action Plan**:
1. Audit public API functions for test coverage
2. Add integration tests for failure scenarios
3. Create property-based tests for DNS parsing
4. Add chaos testing for network operations
5. Implement coverage reporting in CI

**Acceptance Criteria**:
- 90%+ line coverage on core modules
- All public functions have unit tests
- Integration tests for all failure paths
- Property tests for DNS parsing edge cases

---

## DEBT-008: Logging Inconsistencies

**Priority**: 🟡 Medium  
**Category**: Observability  
**Impact**: Difficult debugging, poor monitoring  
**Effort**: 2 days  
**Risk**: Production debugging difficulties  

**Issues**:
- `println!` statements in source files
- Missing request correlation IDs
- Inconsistent error message formatting
- Log levels not standardized

**Action Plan**:
1. Replace all `println!` with structured logging
2. Add request correlation ID middleware
3. Standardize log message format across modules
4. Add log level configuration per module

**Acceptance Criteria**:
- Zero `println!` statements in production code
- All requests have correlation IDs
- Consistent log message structure
- Configurable log levels per module

---

## DEBT-009: Circular Dependencies

**Priority**: 🟡 Medium  
**Category**: Architecture  
**Impact**: Difficult to maintain, fragile imports  
**Effort**: 3 days  
**Risk**: Compilation issues, hard to refactor  

**Location**: `src/dns/mod.rs:18` - Validation module dependency

**Action Plan**:
1. Map all module dependencies
2. Extract shared types to common module
3. Use dependency injection for cross-cutting concerns
4. Implement trait-based abstractions

**Acceptance Criteria**:
- Clean dependency graph with no cycles
- Shared types in dedicated modules
- Clear module boundaries
- Dependency documentation updated

---

## DEBT-010: Input Validation Gaps

**Priority**: 🟡 Medium  
**Category**: Security  
**Impact**: Potential security vulnerabilities  
**Effort**: 3 days  
**Risk**: Resource exhaustion, cache pollution  

**Missing Validations**:
- DNS name length limits
- Cache key size limits
- Request size bounds
- Character set validation for domain names

**Action Plan**:
1. Add input size limits at API boundaries
2. Implement domain name validation per RFC
3. Add cache key sanitization
4. Create fuzzing tests for input validation

**Acceptance Criteria**:
- All inputs validated at boundaries
- Protection against resource exhaustion
- Fuzzing tests pass with no crashes
- Security review completed

---

## Summary by Priority

### 🔴 Critical (Must Fix Next Sprint)
- DEBT-001: Unsafe Unwrap Calls (2 days)
- DEBT-002: Environment Variable Parsing (3 days)

### 🟡 High (Fix Within 2 Sprints)
- DEBT-003: Inconsistent Error Handling (4 days)
- DEBT-004: DNS Parsing Duplication (3 days)
- DEBT-005: Memory Allocations (5 days)
- DEBT-006: Configuration Validation (2 days)
- DEBT-007: Test Coverage Gaps (4 days)

### 🟡 Medium (Address Next Quarter)
- DEBT-008: Logging Inconsistencies (2 days)
- DEBT-009: Circular Dependencies (3 days)
- DEBT-010: Input Validation Gaps (3 days)

**Total Effort**: 31 days estimated for all items

## Progress Tracking

| Debt ID | Status | Assigned | Started | Completed | Notes |
|---------|--------|----------|---------|-----------|-------|
| DEBT-001 | Open | - | - | - | Highest priority |
| DEBT-002 | Open | - | - | - | Security critical |
| ... | ... | ... | ... | ... | ... |

## Review Schedule

- **Weekly**: Review progress on critical items
- **Monthly**: Reassess priorities based on new findings
- **Quarterly**: Comprehensive debt audit update