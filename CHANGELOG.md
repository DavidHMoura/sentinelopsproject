# Changelog

## [0.2.0] - 2026-03-08

### Added

#### Security
- API key authentication middleware
  - Validates X-API-Key header against configured keys
  - Supports multiple keys via comma-separated environment variable
  - Logs failed authentication attempts
  - Returns 401 Unauthorized for invalid/missing keys

- Rate limiting via actix-governor
  - 2 requests per second per IP
  - Burst capacity of 10 requests
  - Returns 429 Too Many Requests when exceeded

- Input validation with validator crate
  - EventIn struct validates event_type (1-100 chars)
  - Validates source_ip (7-45 chars)
  - Validates actor (max 255 chars)
  - Returns 400 Bad Request with validation errors

#### Error Handling
- Custom error types with thiserror
  - SentinelError enum for all application errors
  - Automatic conversion from sqlx::Error
  - HTTP response mapping via ResponseError trait
  - Type alias SentinelResult<T> for ergonomics

- Eliminated all .unwrap() calls
  - Config parsing uses Result with descriptive errors
  - Database operations propagate errors properly
  - No panic risks in production code

#### Logging
- Replaced log crate with tracing
  - Structured logging with key-value pairs
  - Contextual information (event_id, source_ip, etc.)
  - ENV-based log level configuration
  - Compatible with observability tools

#### Testing
- Unit tests for API key middleware
  - Valid key acceptance
  - Invalid key rejection
  - Missing key rejection

- Integration tests for detection logic
  - Brute-force threshold validation
  - Non-auth event filtering
  - Multiple IP tracking

- Model validation tests
  - Valid input acceptance
  - Invalid input rejection
  - Edge case handling

### Changed

- Updated Cargo.toml dependencies
  - Added: tracing, tracing-subscriber, thiserror, actix-governor, validator
  - Removed: log, env_logger

- Enhanced Config struct
  - Added api_keys, server_host, server_port fields
  - Returns SentinelResult instead of panicking
  - Validates API_KEYS environment variable
  - Provides descriptive error messages

- Improved database pool creation
  - Increased max_connections to 10
  - Added min_connections of 2
  - Returns SentinelResult for proper error handling

- Updated Event models
  - Added Validate derive to EventIn
  - Schema validation before processing

- Enhanced detection logic
  - Better structured logging
  - Improved error messages in alerts

- Refactored API handlers
  - Use SentinelResult for error propagation
  - Added query parameter support (limit)
  - Validate input before processing
  - Proper error responses

- Updated main.rs
  - Integrated authentication middleware
  - Added rate limiting
  - Run database migrations on startup
  - Better startup logging

### Fixed
- Configuration parsing errors now return descriptive messages instead of panicking
- Database errors propagate correctly through error type system
- Alert fingerprinting includes proper timestamp bucketing

## [0.1.0] - 2026-03-07

### Added
- Initial release
- Event ingestion API
- Brute-force detection
- Alert generation
- PostgreSQL storage
- Basic logging
