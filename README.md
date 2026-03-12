# SentinelOps Rust

High-performance security event ingestion, threat detection, and automated alerting system built in Rust.

## Version 0.2.0

This release adds production-ready security and reliability features:

- API Key Authentication
- Rate Limiting
- Robust Error Handling
- Input Validation
- Structured Logging with tracing
- Comprehensive Test Coverage

## Features

### Core Capabilities
- REST API for high-throughput event ingestion
- Real-time threat detection engine
- Alert deduplication via fingerprinting
- Async PostgreSQL operations

### Security (v0.2.0)
- Middleware-based API key authentication
- Rate limiting (2 req/s, burst 10)
- Input validation with schema enforcement
- Structured audit logging

## Architecture

```
Client -> Actix-web Server -> PostgreSQL
              |
              v
         Detection Engine
              |
              v
            Alerts
```

## Installation

### Prerequisites

- Rust 1.75+
- PostgreSQL 14+

### Database Setup

```sql
CREATE DATABASE sentinelops;
CREATE USER sentinelops WITH PASSWORD 'sentinelops';
GRANT ALL PRIVILEGES ON DATABASE sentinelops TO sentinelops;
```

### Configuration

```bash
cp .env.example .env
# Edit .env and set your API keys
```

### Run

```bash
cargo run
```

## API Usage

All requests require X-API-Key header:

```bash
curl -H "X-API-Key: your-key" http://localhost:8000/api/events
```

### Endpoints

**POST /api/events/ingest** - Ingest security event
```bash
curl -X POST http://localhost:8000/api/events/ingest \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-key" \
  -d '{
    "ts": "2026-03-08T22:00:00Z",
    "event_type": "auth.login.failed",
    "source_ip": "192.168.1.100",
    "actor": "user@example.com",
    "meta": {}
  }'
```

**GET /api/events?limit=50** - List recent events

**GET /api/alerts?limit=50** - List alerts

**GET /api/alerts/{id}** - Get specific alert

## Configuration

Environment variables (.env file):

| Variable | Description | Default |
|----------|-------------|---------|
| DATABASE_URL | PostgreSQL connection string | postgres://sentinelops:sentinelops@localhost:5432/sentinelops |
| API_KEYS | Comma-separated list of valid keys | Required |
| SERVER_HOST | Bind address | 127.0.0.1 |
| SERVER_PORT | Port number | 8000 |
| AUTH_MAX_ATTEMPTS | Brute-force threshold | 10 |
| AUTH_WINDOW_SECONDS | Detection time window | 300 |
| RUST_LOG | Log level | info |

## Detection Rules

### Brute-Force Detection

Monitors auth.login.failed events and generates alerts when attempts from a single IP exceed the configured threshold within the time window.

Alert includes:
- Attempt count
- Source IP
- Target actor (if available)
- Time window
- Event IDs for investigation

## Testing

```bash
# Unit tests
cargo test

# Integration tests (requires PostgreSQL)
cargo test --test detection_integration -- --ignored
```

## Project Structure

```
src/
├── main.rs           - Application entry point
├── api.rs            - REST endpoints
├── config.rs         - Configuration management
├── db.rs             - Database pool
├── detection.rs      - Threat detection logic
├── models.rs         - Data structures
├── errors.rs         - Error types
└── middleware/
    └── auth.rs       - API key authentication

tests/
└── detection_integration.rs

migrations/
└── 20260307_init.sql
```

## Technical Details

### Error Handling
Uses thiserror for type-safe error propagation. All errors are mapped to appropriate HTTP responses via ResponseError trait.

### Logging
Structured logging with tracing provides rich context for debugging and monitoring. Compatible with standard observability tools.

### Validation
Input validation with validator crate ensures data integrity before processing.

### Rate Limiting
Per-IP rate limiting via actix-governor prevents abuse and DoS attacks.

## Dependencies

- actix-web: Web framework
- sqlx: Async PostgreSQL client
- tracing: Structured logging
- thiserror: Error handling
- validator: Input validation
- actix-governor: Rate limiting

## License

MIT

## Contact

David Moura
Project: https://github.com/DavidHMoura/sentinelopsproject
