# 🚀 Quick Start Guide - SentinelOps Rust v0.2.0

Get SentinelOps running in 5 minutes!

## Prerequisites Check

```bash
# Check Rust installation
rustc --version  # Should be 1.75+

# Check PostgreSQL
psql --version   # Should be 14+
```

## Step 1: Database Setup (2 minutes)

```bash
# Start PostgreSQL (if not running)
sudo service postgresql start  # Linux
# OR
brew services start postgresql # macOS

# Create database
psql -U postgres -c "CREATE DATABASE sentinelops;"
psql -U postgres -c "CREATE USER sentinelops WITH PASSWORD 'sentinelops';"
psql -U postgres -c "GRANT ALL PRIVILEGES ON DATABASE sentinelops TO sentinelops;"
```

## Step 2: Configure Application (1 minute)

```bash
# Copy example config
cp .env.example .env

# Generate secure API key
openssl rand -hex 32

# Edit .env and paste the generated key
nano .env
```

Minimal `.env` configuration:
```bash
DATABASE_URL=postgres://sentinelops:sentinelops@localhost:5432/sentinelops
API_KEYS=YOUR_GENERATED_KEY_HERE
```

## Step 3: Run! (2 minutes)

```bash
# First run (downloads dependencies + compiles)
cargo run

# Wait for:
# 🚀 Starting SentinelOps Rust v0.2.0
# 🌐 Starting server on 127.0.0.1:8000
```

## Step 4: Test It! (1 minute)

```bash
# Test authentication (should fail with 401)
curl http://localhost:8000/api/events

# Test with API key (should succeed)
curl -H "X-API-Key: YOUR_GENERATED_KEY_HERE" \
     http://localhost:8000/api/events

# Ingest a test event
curl -X POST http://localhost:8000/api/events/ingest \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_GENERATED_KEY_HERE" \
  -d '{
    "ts": "2026-03-08T22:00:00Z",
    "event_type": "auth.login.failed",
    "source_ip": "192.168.1.100",
    "actor": "testuser@example.com",
    "meta": {}
  }'
```

## 🎉 Success!

You should see:
- ✅ Event created (HTTP 201)
- ✅ Event ID returned in JSON
- ✅ Server logs showing the ingestion

## Next Steps

1. **Read EVOLUTION.md** - Understand what changed and why
2. **Check logs** - See structured logging in action
3. **Run tests** - `cargo test`
4. **Trigger brute-force detection** - Send 15 failed login events

## Common Issues

### "connection refused" error
→ PostgreSQL not running: `sudo service postgresql start`

### "API_KEYS environment variable is required"
→ Check your `.env` file exists and has `API_KEYS` set

### Compilation errors
→ Update Rust: `rustup update`

## Development Mode

```bash
# Watch mode (auto-reload on changes)
cargo install cargo-watch
cargo watch -x run

# Run with debug logging
RUST_LOG=debug cargo run
```

## Testing Brute-Force Detection

```bash
# Send 15 failed login attempts from same IP
for i in {1..15}; do
  curl -X POST http://localhost:8000/api/events/ingest \
    -H "Content-Type: application/json" \
    -H "X-API-Key: YOUR_KEY" \
    -d "{
      \"ts\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
      \"event_type\": \"auth.login.failed\",
      \"source_ip\": \"192.168.1.100\",
      \"actor\": \"attacker@evil.com\",
      \"meta\": {}
    }"
  sleep 0.5
done

# Check alerts
curl -H "X-API-Key: YOUR_KEY" \
     http://localhost:8000/api/alerts
```

You should see a high-severity brute-force alert! 🎯

---

Need help? Check [README.md](./README.md) or [EVOLUTION.md](./EVOLUTION.md)
