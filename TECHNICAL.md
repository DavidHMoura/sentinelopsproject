# Technical Documentation - v0.2.0 Improvements

## Overview

Version 0.2.0 introduces production-ready security, reliability, and observability features. This document explains the technical decisions, implementation details, and architectural changes.

## 1. Error Handling System

### Problem
Version 0.1.0 used `.unwrap()` extensively, which causes panics on errors. This is unacceptable in production systems.

### Solution
Implemented a type-safe error system using thiserror:

```rust
#[derive(Error, Debug)]
pub enum SentinelError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Authentication failed: {0}")]
    AuthError(String),
    
    #[error("Invalid input: {0}")]
    ValidationError(String),
    // ...
}
```

### Benefits
- Type-safe error propagation via Result
- Automatic conversion from sqlx::Error using #[from]
- Consistent HTTP response mapping via ResponseError trait
- No risk of panics in production

### Implementation Details
- Located in src/errors.rs
- Type alias `SentinelResult<T>` reduces boilerplate
- Each error variant maps to appropriate HTTP status code
- Error messages are serialized as JSON

## 2. Authentication Middleware

### Requirements
- Protect API endpoints from unauthorized access
- Support multiple API keys
- Log authentication failures
- Minimal performance overhead

### Implementation
Custom Actix middleware implementing Transform trait:

```rust
pub struct ApiKeyAuth {
    valid_keys: Rc<Vec<String>>,
}
```

### Key Design Decisions

**Why Rc instead of Arc?**
Actix workers are single-threaded, so Rc is sufficient and more efficient than Arc.

**Why middleware instead of per-handler checks?**
- Centralized authentication logic
- Applies to all routes automatically
- Separation of concerns
- Easier to test and maintain

**How it works:**
1. Extract X-API-Key header from request
2. Compare against configured valid keys
3. If valid, pass request to next service
4. If invalid, return 401 Unauthorized

### Configuration
API keys loaded from environment variable:
```bash
API_KEYS=key1,key2,key3
```

### Testing
Three test cases cover:
- Valid key acceptance
- Invalid key rejection  
- Missing key rejection

Located in src/middleware/auth.rs tests module.

## 3. Rate Limiting

### Purpose
Prevent abuse and DoS attacks by limiting request rate per IP.

### Implementation
Uses actix-governor crate with token bucket algorithm:

```rust
let governor_conf = GovernorConfigBuilder::default()
    .per_second(2)
    .burst_size(10)
    .finish()?;
```

### Configuration Rationale

**2 requests/second:**
- Allows legitimate usage patterns
- Prevents brute-force attacks
- Sufficient for typical SIEM ingestion

**Burst size 10:**
- Accommodates occasional traffic spikes
- Prevents blocking legitimate users
- Still effective against automated attacks

### Behavior
- Tracks requests per client IP
- Returns 429 Too Many Requests when exceeded
- Token bucket refills at configured rate

## 4. Input Validation

### Problem
Unvalidated input can lead to:
- SQL injection (mitigated by SQLx, but defense in depth)
- Buffer overflows in downstream systems
- Invalid data in database

### Solution
Schema validation using validator crate:

```rust
#[derive(Deserialize, Validate)]
pub struct EventIn {
    #[validate(length(min = 1, max = 100))]
    pub event_type: String,
    
    #[validate(length(min = 7, max = 45))]
    pub source_ip: String,
    // ...
}
```

### Validation Rules

**event_type:** 1-100 characters
- Prevents empty strings
- Limits excessive length

**source_ip:** 7-45 characters
- IPv4 minimum: 7 chars (0.0.0.0)
- IPv6 maximum: 45 chars

**actor:** 255 characters max
- Standard VARCHAR limit
- Prevents excessive memory usage

### Error Handling
Validation failures return 400 Bad Request with detailed error messages.

## 5. Structured Logging

### Migration: log -> tracing

**Why tracing?**
- Structured logging with key-value pairs
- Better performance
- Span/trace support for distributed systems
- Industry standard for Rust applications

### Example
```rust
tracing::warn!(
    source_ip = %event.source_ip,
    attempts = attempts,
    "Brute-force attack detected"
);
```

Outputs:
```
WARN source_ip=192.168.1.1 attempts=15 Brute-force attack detected
```

### Benefits
- Easier to parse by log aggregators
- Rich context without string formatting
- Filterable by severity and fields
- Compatible with OpenTelemetry

## 6. Configuration Management

### Improvements

**Error handling:**
```rust
// Before (v0.1.0)
.parse().unwrap()  // Panics on invalid input

// After (v0.2.0)
.parse().map_err(|e| {
    SentinelError::ConfigError(format!("Invalid: {}", e))
})?
```

**Validation:**
- API_KEYS is required, returns error if missing
- Validates at least one key is configured
- Parses and validates numeric values

**New fields:**
- api_keys: Vec<String>
- server_host: String
- server_port: u16

## 7. Testing Strategy

### Unit Tests
- Middleware: authentication logic
- Models: validation rules
- Config: parsing and validation

### Integration Tests
- Detection: database-backed brute-force logic
- Requires PostgreSQL for execution
- Marked with #[ignore] to skip in CI

### Running Tests
```bash
# Unit tests only
cargo test

# Including integration tests
cargo test -- --ignored
```

## 8. Database Improvements

### Connection Pool
```rust
PgPoolOptions::new()
    .max_connections(10)  // Increased from 5
    .min_connections(2)   // New: maintain minimum connections
```

**Rationale:**
- Higher max supports more concurrent requests
- Min connections reduce latency for first requests
- Better resource utilization

### Migrations
Auto-run on startup via:
```rust
sqlx::migrate!("./migrations")
    .run(&pool)
    .await?;
```

Ensures database schema is always up-to-date.

## 9. Architectural Decisions

### Module Organization
```
src/
├── errors.rs      - Centralized error types
├── middleware/    - Cross-cutting concerns
│   └── auth.rs
├── models.rs      - Data structures with validation
├── api.rs         - HTTP handlers
├── detection.rs   - Business logic
└── main.rs        - Application bootstrap
```

**Rationale:**
- Clear separation of concerns
- Easy to locate functionality
- Testable in isolation

### Dependency Choices

**thiserror over anyhow:**
- Library code benefits from specific error types
- Better API ergonomics
- Type-safe error handling

**tracing over log:**
- Modern standard
- Better observability
- Structured data support

**actix-governor:**
- Well-maintained
- Good integration with Actix
- Flexible configuration

**validator:**
- Declarative validation
- Comprehensive rule set
- Good error messages

## 10. Performance Considerations

### Overhead Analysis

**Authentication middleware:**
- HashMap lookup: O(n) where n = number of API keys
- Typically < 10 keys, negligible overhead
- Could use HashMap for O(1) if needed

**Rate limiting:**
- Token bucket: O(1) per request
- Minimal memory per IP
- Automatic cleanup of old entries

**Validation:**
- Length checks: O(1)
- Runs before database access
- Prevents wasted resources on invalid data

### Optimization Opportunities
- Use HashMap for API keys if count > 100
- Add connection pooling metrics
- Implement request ID tracing

## 11. Security Analysis

### Threat Model

**Prevented:**
- Unauthorized access (API keys)
- DoS attacks (rate limiting)
- SQL injection (parameterized queries)
- Invalid data injection (validation)

**Not covered (future work):**
- API key rotation
- Audit logging
- Request signing
- TLS/HTTPS (deployment concern)

### Best Practices Implemented
- Secrets in environment variables
- No sensitive data in logs
- Proper error messages (no stack traces to client)
- Defense in depth (multiple layers)

## 12. Upgrade Path from v0.1.0

### Breaking Changes
1. Config::from_env() now returns Result
2. API requires authentication header
3. Different logging format

### Migration Steps
1. Update Cargo.toml dependencies
2. Add API_KEYS to .env
3. Update code calling Config::from_env()
4. Replace log macros with tracing
5. Add validation to input types
6. Test with new authentication

### Backwards Compatibility
- Database schema unchanged
- API endpoints unchanged (except auth)
- Alert format unchanged

## Summary

Version 0.2.0 transforms SentinelOps from a prototype to a production-ready system through:

1. Type-safe error handling eliminating panics
2. Authentication protecting endpoints
3. Rate limiting preventing abuse
4. Input validation ensuring data integrity
5. Structured logging enabling observability
6. Comprehensive testing ensuring reliability

These improvements address the critical feedback while maintaining code clarity and performance.
