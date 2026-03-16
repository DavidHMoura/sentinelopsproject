# SentinelOps Dual-Stack Foundation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the dual-stack foundation: Cargo Workspace, shared Protobuf contract, sentinelops-agent skeleton (Rust/Tonic), and sentinelops-control skeleton (Java 21/Spring Boot) communicating over gRPC+mTLS.

**Architecture:** The existing `sentinelops-rust` repo becomes a Cargo Workspace root hosting two crates: itself (Aggregator/Edge Node) and the new `sentinelops-agent` (pure gRPC client). A Java 21 Spring Boot service acts as the Control Plane. All communication uses gRPC over mTLS; agent identity is enforced at both the TLS transport layer (cert CN) and the Protobuf payload layer (agent_id field).

**Tech Stack:** Rust 2021/Tokio/Tonic 0.12, Java 21/Spring Boot 3.3/grpc-server-spring-boot-starter 3.1, Protobuf 3, OpenSSL (dev PKI)

**Prerequisite:** `protoc` installed (`sudo apt install protobuf-compiler` or `brew install protobuf`).

---

## Chunk 1: Foundation — Schema, Workspace, Proto, Scripts

### Task 1: Database Schema Fix Migration

**Files:**
- Create: `migrations/20260316_fix_meta_column.sql`

This resolves the pre-existing `metadata`→`meta` column inconsistency documented in spec §1a. Must be applied before the workspace build.

- [ ] **Step 1.1: Create the corrective migration**

```sql
-- migrations/20260316_fix_meta_column.sql
-- Resolves: init created `metadata JSONB`, later migration added duplicate `meta JSONB`.
-- Fix: migrate data, enforce NOT NULL, drop redundant column.

-- Step A: backfill new column from old for all existing rows
UPDATE events SET meta = metadata WHERE meta IS NULL;

-- Step B: promote meta to NOT NULL with default (matches original metadata semantics)
ALTER TABLE events ALTER COLUMN meta SET NOT NULL;
ALTER TABLE events ALTER COLUMN meta SET DEFAULT '{}';

-- Step C: drop the now-redundant original column
ALTER TABLE events DROP COLUMN metadata;
```

- [ ] **Step 1.2: Apply migration and verify**

```bash
# Ensure DATABASE_URL is set in .env, then:
cargo sqlx migrate run
# Expected: "Applied 20260316_fix_meta_column"
```

```bash
# Verify final schema (psql):
psql $DATABASE_URL -c "\d events"
# Expected columns: id, ts, event_type, source_ip, actor, meta (NO metadata column)
```

- [ ] **Step 1.3: Commit**

```bash
git add migrations/20260316_fix_meta_column.sql
git commit -m "fix: migrate events.metadata → events.meta, drop redundant column"
```

---

### Task 2: Cargo Workspace Setup

**Files:**
- Modify: `Cargo.toml` (add `[workspace]` section above `[package]`)

- [ ] **Step 2.1: Add workspace declaration to root Cargo.toml**

Insert at the very top of `Cargo.toml`, before `[package]`:

```toml
[workspace]
members  = [".", "sentinelops-agent"]
resolver = "2"
```

The file should now start with:
```toml
[workspace]
members  = [".", "sentinelops-agent"]
resolver = "2"

[package]
name    = "sentinelops-rust"
version = "0.2.0"
edition = "2021"
# ... rest unchanged
```

- [ ] **Step 2.2: Verify existing crate still compiles**

```bash
cargo check -p sentinelops-rust
# Expected: Finished (no errors)
# NOTE: cargo metadata will fail here because sentinelops-agent/ does not exist yet.
# Full workspace verification is in Task 3, Step 3.6.
```

- [ ] **Step 2.3: Verify existing tests still pass**

```bash
cargo test -p sentinelops-rust
# Expected: all existing tests pass (config, middleware/auth tests)
```

- [ ] **Step 2.4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: convert to Cargo workspace (members: ., sentinelops-agent)"
```

---

### Task 3: Protobuf Contract + Agent Build Infrastructure

**Files:**
- Create: `proto/sentinel.proto`
- Create: `sentinelops-agent/Cargo.toml`
- Create: `sentinelops-agent/build.rs`
- Create: `sentinelops-agent/src/main.rs` (stub only — extended in Task 8)

- [ ] **Step 3.1: Create the proto directory and contract**

```bash
mkdir -p proto
```

Create `proto/sentinel.proto`:

```protobuf
syntax = "proto3";

package sentinel;

option java_multiple_files  = true;
option java_package         = "com.sentinelops.grpc";
option java_outer_classname = "SentinelProto";

// ─── Service ──────────────────────────────────────────────────────────────────

service IngestionService {
  // Unary: debug / critical events needing individual ACK
  rpc SendEvent (SecurityEvent) returns (EventResponse);

  // Client-side streaming: production mode
  // Agent opens ONE HTTP/2 stream, sends N events, server responds once.
  rpc StreamEvents (stream SecurityEvent) returns (StreamSummary);
}

// ─── Messages ─────────────────────────────────────────────────────────────────

message SecurityEvent {
  string event_id         = 1;  // UUID v4 lowercase
  string timestamp        = 2;  // RFC 3339 UTC
  string event_type       = 3;  // see canonical taxonomy in spec §3.2
  string source_ip        = 4;  // origin IP of the event (not the agent IP)
  bytes  meta_payload     = 5;  // payload_encoding declares the codec
  string agent_id         = 6;  // UUID v4 lowercase — MUST match cert CN
  string source_host      = 7;  // FQDN or hostname of host OS
  uint32 payload_encoding = 8;  // 0=RAW_JSON  1=ZSTD_JSON  2=ZSTD_MSGPACK
}

message EventResponse {
  bool   accepted = 1;
  string message  = 2;
  string event_id = 3;  // echo for client-side correlation
}

message StreamSummary {
  uint32 accepted_count = 1;
  uint32 rejected_count = 2;
  string session_id     = 3;  // gRPC session ID for audit trail
}
```

- [ ] **Step 3.2: Create sentinelops-agent/Cargo.toml**

```bash
mkdir -p sentinelops-agent/src
```

Create `sentinelops-agent/Cargo.toml`:

```toml
[package]
name    = "sentinelops-agent"
version = "0.1.0"
edition = "2021"

[dependencies]
tonic          = { version = "0.12", features = ["tls"] }
prost          = "0.13"
tokio          = { version = "1", features = ["full"] }
tokio-stream   = "0.1"
anyhow         = "1"
tracing        = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid           = { version = "1", features = ["v4"] }
chrono         = "0.4"
hostname       = "0.4"
dotenvy        = "0.15"

[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 3.3: Create sentinelops-agent/build.rs**

Use `CARGO_MANIFEST_DIR` to make the proto path absolute — safe regardless of the
working directory at build time (important for CI environments).

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("sentinelops-agent must be a sub-directory of the workspace root")
        .join("proto/sentinel.proto");
    tonic_build::compile_protos(proto_path)?;
    Ok(())
}
```

- [ ] **Step 3.4: Create stub main.rs so the crate compiles**

Create `sentinelops-agent/src/main.rs`:

```rust
// Proto types — generated from ../proto/sentinel.proto at build time
pub mod sentinel {
    tonic::include_proto!("sentinel");
}

fn main() {
    println!("sentinelops-agent stub — tasks 5-8 will wire the full agent");
}
```

- [ ] **Step 3.5: Verify proto compiles into Rust types**

```bash
cargo build -p sentinelops-agent
# Expected: compiles successfully; tonic-build generates sentinel.rs in $OUT_DIR
# If protoc not found: sudo apt install protobuf-compiler
```

- [ ] **Step 3.6: Verify workspace sees both crates**

```bash
cargo metadata --no-deps --format-version 1 | python3 -c "
import json,sys
m=json.load(sys.stdin)
print(sorted([p['name'] for p in m['packages']]))
"
# Expected: ['sentinelops-agent', 'sentinelops-rust']
```

- [ ] **Step 3.7: Commit**

```bash
git add proto/sentinel.proto sentinelops-agent/
git commit -m "feat: add proto/sentinel.proto and sentinelops-agent crate scaffold (tonic build)"
```

---

### Task 4: Dev PKI Bootstrap Script + gitignore

**Files:**
- Create: `scripts/gen-dev-certs.sh`
- Create: `certs/.gitkeep`
- Modify: `.gitignore` (add Java/Maven build artifacts)

- [ ] **Step 4.1: Create the PKI script**

```bash
mkdir -p scripts certs
```

Create `scripts/gen-dev-certs.sh`:

```bash
#!/usr/bin/env bash
# Generates the complete dev mTLS PKI for SentinelOps.
# Usage: ./scripts/gen-dev-certs.sh [AGENT_UUID]
# AGENT_UUID defaults to "agent-uuid-1234" if not provided.
#
# WARNING: For development ONLY. Never use these certs in production.
# Production: HashiCorp Vault PKI Engine or AWS Private CA.

set -euo pipefail

CERTS_DIR="certs"
AGENT_UUID="${1:-agent-uuid-1234}"
CA_DAYS=3650
CERT_DAYS=365

# Enforce lowercase UUID format (Zero Trust invariant: cert CN must equal AGENT_ID env var,
# both must be lowercase to avoid case-sensitivity mismatches in the Java interceptor)
AGENT_UUID="${AGENT_UUID,,}"

mkdir -p "$CERTS_DIR"
cd "$CERTS_DIR"

echo "==> [1/4] Generating development CA..."
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days "$CA_DAYS" -key ca.key -out ca.pem \
  -subj "/C=BR/O=SentinelOps Dev/CN=SentinelOps Dev CA"

echo "==> [2/4] Generating Control Plane server cert..."
openssl genrsa -out server.key 2048
openssl req -new -key server.key -out server.csr \
  -subj "/C=BR/O=SentinelOps/CN=sentinelops-control.internal"

# SAN is mandatory — modern TLS rejects certs without subjectAltName
cat > server-ext.cnf <<EOF
[req_ext]
subjectAltName=DNS:sentinelops-control.internal,DNS:localhost,IP:127.0.0.1
EOF

openssl x509 -req -days "$CERT_DAYS" \
  -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out server.pem -extfile server-ext.cnf -extensions req_ext

echo "==> [3/4] Generating Agent cert (CN=${AGENT_UUID})..."
# CRITICAL: CN must equal AGENT_ID env var. Java interceptor validates
# agent_id field in SecurityEvent against this CN (both must be lowercase).
openssl genrsa -out agent.key 2048
openssl req -new -key agent.key -out agent.csr \
  -subj "/C=BR/O=SentinelOps/CN=${AGENT_UUID}"
openssl x509 -req -days "$CERT_DAYS" \
  -in agent.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out agent.pem

echo "==> [4/4] Verifying trust chain..."
openssl verify -CAfile ca.pem server.pem && echo "  server.pem OK"
openssl verify -CAfile ca.pem agent.pem  && echo "  agent.pem  OK"

rm -f server.csr server-ext.cnf agent.csr ./*.srl

cat <<SUMMARY

Certificates written to ./${CERTS_DIR}/
  ca.pem         — trust anchor (distribute to both Rust agent and Java server)
  server.pem/key — Java Control Plane
  agent.pem/key  — Rust Agent (CN=${AGENT_UUID})

Zero Trust invariant:
  Set AGENT_ID=${AGENT_UUID} in sentinelops-agent/.env
  Java interceptor extracts CN from mTLS cert and validates against agent_id payload field.
  CN ≠ agent_id → PERMISSION_DENIED.

Production: use Vault PKI with 24h TTL + cert-manager rotation.
SUMMARY
```

- [ ] **Step 4.2: Make script executable**

```bash
chmod +x scripts/gen-dev-certs.sh
```

- [ ] **Step 4.3: Add Java/Maven build artifacts to .gitignore**

Append to `.gitignore`:

```gitignore
# Java / Maven
sentinelops-control/target/
sentinelops-control/.mvn/wrapper/maven-wrapper.jar
*.class

# Generated proto (Java side)
sentinelops-control/src/main/java/com/sentinelops/grpc/Sentinel*.java
```

Note: `*.pem` and `*.key` are already in `.gitignore`. Do NOT add a `certs/` directory
entry — it would also swallow `certs/.gitkeep` and prevent the directory from being
tracked in git.

- [ ] **Step 4.4: Create certs placeholder**

```bash
touch certs/.gitkeep
```

- [ ] **Step 4.5: Run the script and verify output**

```bash
./scripts/gen-dev-certs.sh agent-uuid-1234
# Expected: 4 steps complete, "server.pem OK" and "agent.pem OK"
ls certs/
# Expected: .gitkeep  agent.key  agent.pem  ca.key  ca.pem  server.key  server.pem
```

- [ ] **Step 4.6: Commit**

```bash
git add scripts/gen-dev-certs.sh certs/.gitkeep .gitignore
git commit -m "feat: add dev PKI script (mTLS CA + server + agent certs) and update .gitignore"
```

---

## Chunk 2: Rust Agent — sentinelops-agent crate

### Task 5: AgentConfig

**Files:**
- Create: `sentinelops-agent/src/config.rs`
- Modify: `sentinelops-agent/src/main.rs` (add `mod config`)

- [ ] **Step 5.1: Write failing tests first**

Create `sentinelops-agent/src/config.rs` with tests:

```rust
use std::env;

#[derive(Debug, Clone)]
pub enum AgentMode {
    Autonomous,
    Subordinate,
    Hybrid,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub agent_id:            String,
    pub source_host:         String,
    pub control_plane_addr:  String,
    pub ca_cert_path:        String,
    pub client_cert_path:    String,
    pub client_key_path:     String,
    pub batch_size:          usize,
    pub flush_interval_ms:   u64,
    pub queue_capacity:      usize,
    pub mode:                AgentMode,
}

impl AgentConfig {
    pub fn from_env() -> Result<Self, String> {
        let agent_id = env::var("AGENT_ID")
            .map_err(|_| "AGENT_ID environment variable is required".to_string())?;

        if agent_id.is_empty() {
            return Err("AGENT_ID must not be empty".to_string());
        }

        // Enforce lowercase to match cert CN invariant (spec §5.2)
        let agent_id = agent_id.to_lowercase();

        let source_host = env::var("SOURCE_HOST")
            .unwrap_or_else(|_| {
                hostname::get()
                    .map(|h| h.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "unknown-host".to_string())
            });

        Ok(Self {
            agent_id,
            source_host,
            control_plane_addr: env::var("CONTROL_PLANE_ADDR")
                .unwrap_or_else(|_| "https://127.0.0.1:9090".to_string()),
            ca_cert_path:       env::var("CA_CERT_PATH")
                .unwrap_or_else(|_| "certs/ca.pem".to_string()),
            client_cert_path:   env::var("CLIENT_CERT_PATH")
                .unwrap_or_else(|_| "certs/agent.pem".to_string()),
            client_key_path:    env::var("CLIENT_KEY_PATH")
                .unwrap_or_else(|_| "certs/agent.key".to_string()),
            batch_size:         env::var("BATCH_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500),
            flush_interval_ms:  env::var("FLUSH_INTERVAL_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1_000),
            queue_capacity:     env::var("AGENT_QUEUE_CAPACITY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000),
            mode: match env::var("AGENT_MODE").as_deref() {
                Ok("autonomous")  => AgentMode::Autonomous,
                Ok("subordinate") => AgentMode::Subordinate,
                _                 => AgentMode::Hybrid, // default
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-mutation tests — env is process-global
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_agent_id_required() {
        let _g = env_lock().lock().unwrap();
        env::remove_var("AGENT_ID");
        let result = AgentConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AGENT_ID"));
    }

    #[test]
    fn test_agent_id_normalised_to_lowercase() {
        let _g = env_lock().lock().unwrap();
        env::set_var("AGENT_ID", "AGENT-UUID-1234");
        let config = AgentConfig::from_env().unwrap();
        assert_eq!(config.agent_id, "agent-uuid-1234");
        env::remove_var("AGENT_ID");
    }

    #[test]
    fn test_defaults_applied() {
        let _g = env_lock().lock().unwrap();
        env::set_var("AGENT_ID", "test-agent");
        // Clear optional vars to test defaults
        for var in &["BATCH_SIZE", "FLUSH_INTERVAL_MS", "AGENT_QUEUE_CAPACITY", "AGENT_MODE"] {
            env::remove_var(var);
        }
        let config = AgentConfig::from_env().unwrap();
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.flush_interval_ms, 1_000);
        assert_eq!(config.queue_capacity, 10_000);
        assert!(matches!(config.mode, AgentMode::Hybrid));
        env::remove_var("AGENT_ID");
    }

    #[test]
    fn test_subordinate_mode_parsed() {
        let _g = env_lock().lock().unwrap();
        env::set_var("AGENT_ID", "test-agent");
        env::set_var("AGENT_MODE", "subordinate");
        let config = AgentConfig::from_env().unwrap();
        assert!(matches!(config.mode, AgentMode::Subordinate));
        env::remove_var("AGENT_ID");
        env::remove_var("AGENT_MODE");
    }
}
```

- [ ] **Step 5.2: Run tests — expect failure (config module not wired)**

```bash
cargo test -p sentinelops-agent 2>&1 | head -20
# Expected: error[E0583]: file not found for module `config`
# OR if main.rs is a stub with no mod config: tests simply don't run yet
```

- [ ] **Step 5.3: Add mod config to main.rs**

Replace `sentinelops-agent/src/main.rs` with:

```rust
mod config;

pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("sentinelops-agent starting (stub)");
    Ok(())
}
```

- [ ] **Step 5.4: Run tests — expect all pass**

```bash
cargo test -p sentinelops-agent config
# Expected: test_agent_id_required ... ok
#           test_agent_id_normalised_to_lowercase ... ok
#           test_defaults_applied ... ok
#           test_subordinate_mode_parsed ... ok
```

- [ ] **Step 5.5: Commit**

```bash
git add sentinelops-agent/src/config.rs sentinelops-agent/src/main.rs
git commit -m "feat(agent): AgentConfig with env parsing, lowercase enforcement, and tests"
```

---

### Task 6: Collector (Simulated OS Event Source)

**Files:**
- Create: `sentinelops-agent/src/collector.rs`
- Modify: `sentinelops-agent/src/main.rs` (add `mod collector`)

- [ ] **Step 6.1: Write the collector with inline tests**

Create `sentinelops-agent/src/collector.rs`:

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use uuid::Uuid;
use chrono::Utc;

use crate::config::AgentConfig;
use crate::sentinel::SecurityEvent;

/// Simulated OS event collector.
/// Replace the `interval`-based loop with eBPF probes or /proc polling
/// without changing the channel interface.
pub async fn run_collector(config: Arc<AgentConfig>, tx: mpsc::Sender<SecurityEvent>) {
    let mut tick = interval(Duration::from_millis(100)); // ~10 events/s

    loop {
        tick.tick().await;

        let event = SecurityEvent {
            event_id:         Uuid::new_v4().to_string(),
            timestamp:        Utc::now().to_rfc3339(),
            event_type:       "os.process.exec".to_string(),
            source_ip:        "127.0.0.1".to_string(),
            meta_payload:     br#"{"pid":1337,"cmd":"bash","args":[]}"#.to_vec(),
            agent_id:         config.agent_id.clone(),
            source_host:      config.source_host.clone(),
            payload_encoding: 0, // RAW_JSON
        };

        if tx.send(event).await.is_err() {
            tracing::warn!("Collector: ingest channel closed — shutting down collector");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use crate::config::{AgentConfig, AgentMode};

    fn test_config() -> Arc<AgentConfig> {
        Arc::new(AgentConfig {
            agent_id:           "test-agent-uuid".to_string(),
            source_host:        "test-host".to_string(),
            control_plane_addr: "https://127.0.0.1:9090".to_string(),
            ca_cert_path:       "certs/ca.pem".to_string(),
            client_cert_path:   "certs/agent.pem".to_string(),
            client_key_path:    "certs/agent.key".to_string(),
            batch_size:         500,
            flush_interval_ms:  1_000,
            queue_capacity:     10_000,
            mode:               AgentMode::Hybrid,
        })
    }

    #[tokio::test]
    async fn test_collector_emits_correctly_shaped_events() {
        let (tx, mut rx) = mpsc::channel::<SecurityEvent>(10);
        let config = test_config();

        // Run collector briefly, then drop tx clone to stop it
        let config_clone = config.clone();
        let handle = tokio::spawn(async move {
            run_collector(config_clone, tx).await;
        });

        // Receive one event and verify its shape
        let event = tokio::time::timeout(
            Duration::from_millis(500),
            rx.recv(),
        )
        .await
        .expect("timeout waiting for event")
        .expect("channel closed");

        assert_eq!(event.agent_id, "test-agent-uuid");
        assert_eq!(event.source_host, "test-host");
        assert_eq!(event.event_type, "os.process.exec");
        assert_eq!(event.payload_encoding, 0);
        assert!(!event.event_id.is_empty());
        assert!(!event.timestamp.is_empty());

        handle.abort(); // stop the collector task
    }

    #[tokio::test]
    async fn test_collector_stops_when_channel_closed() {
        let (tx, rx) = mpsc::channel::<SecurityEvent>(1);
        let config = test_config();

        // Drop the receiver — channel is now closed from sender perspective
        drop(rx);

        // Collector should exit cleanly (not hang or panic)
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            run_collector(config, tx),
        )
        .await;

        // Either the collector exits before timeout OR it's still running
        // (the 100ms tick may fire before the first send fails).
        // Either way, no panic is the assertion.
        let _ = result; // Ok(()) = exited, Err(Elapsed) = still running (also OK for this test)
    }

    #[tokio::test]
    async fn test_collector_back_pressures_when_channel_full() {
        // Capacity=1; do not read from rx — channel fills after the first send.
        // The collector MUST block on the second send (back-pressure), not drop or panic.
        let (tx, _rx) = mpsc::channel::<SecurityEvent>(1);
        let config = test_config();

        // If back-pressure is working, run_collector blocks on the second tx.send().await
        // and never returns within the timeout — Err(Elapsed) is the success condition.
        let result = tokio::time::timeout(
            Duration::from_millis(250),
            run_collector(config, tx),
        ).await;

        assert!(
            result.is_err(),
            "collector should block (back-pressure) on a full channel, not exit"
        );
    }
}
```

- [ ] **Step 6.2: Run tests — expect failure (mod collector not wired)**

```bash
cargo test -p sentinelops-agent collector 2>&1 | head -10
# Expected: error — module not declared in main.rs
```

- [ ] **Step 6.3: Add mod collector to main.rs**

Add `mod collector;` to `sentinelops-agent/src/main.rs`:

```rust
mod config;
mod collector;

pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("sentinelops-agent starting (stub)");
    Ok(())
}
```

- [ ] **Step 6.4: Run tests — expect all pass**

```bash
cargo test -p sentinelops-agent collector
# Expected:
#   test_collector_emits_correctly_shaped_events ... ok
#   test_collector_stops_when_channel_closed ... ok
#   test_collector_back_pressures_when_channel_full ... ok
```

- [ ] **Step 6.5: Commit**

```bash
git add sentinelops-agent/src/collector.rs sentinelops-agent/src/main.rs
git commit -m "feat(agent): simulated OS event collector with mpsc channel interface and tests"
```

---

### Task 7: StreamingSession (gRPC Client with mTLS)

**Files:**
- Create: `sentinelops-agent/src/client.rs`
- Modify: `sentinelops-agent/src/main.rs` (add `mod client`)

Note: mTLS is not exercised in unit tests (no real certs). The unit test validates the
ingest loop logic using a plaintext in-process Tonic server.

- [ ] **Step 7.1: Create client.rs**

Create `sentinelops-agent/src/client.rs`:

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use anyhow::Context;

use crate::config::AgentConfig;
use crate::sentinel::{
    ingestion_service_client::IngestionServiceClient,
    SecurityEvent,
};

pub struct StreamingSession {
    client: IngestionServiceClient<Channel>,
}

impl StreamingSession {
    /// Connect to the Control Plane with mTLS.
    /// Reads cert files asynchronously — never blocks the executor.
    pub async fn connect(config: &AgentConfig) -> anyhow::Result<Self> {
        let ca_pem   = tokio::fs::read_to_string(&config.ca_cert_path)
            .await
            .with_context(|| format!("Failed to read CA cert: {}", config.ca_cert_path))?;
        let cert_pem = tokio::fs::read_to_string(&config.client_cert_path)
            .await
            .with_context(|| format!("Failed to read client cert: {}", config.client_cert_path))?;
        let key_pem  = tokio::fs::read_to_string(&config.client_key_path)
            .await
            .with_context(|| format!("Failed to read client key: {}", config.client_key_path))?;

        let tls = ClientTlsConfig::new()
            .domain_name("sentinelops-control.internal")
            .ca_certificate(Certificate::from_pem(&ca_pem))
            .identity(Identity::from_pem(&cert_pem, &key_pem));

        let channel = Channel::from_shared(config.control_plane_addr.clone())
            .context("Invalid control plane address")?
            .tls_config(tls)
            .context("TLS configuration failed")?
            .connect()
            .await
            .context("Failed to connect to Control Plane")?;

        Ok(Self {
            client: IngestionServiceClient::new(channel),
        })
    }

    /// Open a client-side streaming RPC.
    /// Returns a Sender — push SecurityEvent values to it.
    /// Dropping the Sender closes the stream; the server responds with StreamSummary.
    pub async fn open_stream(&mut self) -> anyhow::Result<mpsc::Sender<SecurityEvent>> {
        let (tx, rx) = mpsc::channel::<SecurityEvent>(1024);
        let stream   = ReceiverStream::new(rx);
        let mut client = self.client.clone();

        tokio::spawn(async move {
            match client.stream_events(stream).await {
                Ok(resp) => {
                    let s = resp.into_inner();
                    tracing::info!(
                        accepted = s.accepted_count,
                        rejected = s.rejected_count,
                        session  = %s.session_id,
                        "Stream session closed"
                    );
                }
                Err(e) => tracing::error!(error = %e, "gRPC stream error"),
            }
        });

        Ok(tx)
    }
}

/// Main ingest loop: connects to the Control Plane, opens a stream, and
/// drives events from the Collector into the stream in batches.
/// Reconnects automatically with exponential backoff on any failure.
pub async fn run_ingest_loop(
    config: Arc<AgentConfig>,
    mut event_rx: mpsc::Receiver<SecurityEvent>,
) {
    let mut backoff = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(60);

    loop {
        tracing::info!(addr = %config.control_plane_addr, "Connecting to Control Plane...");

        let mut session = match StreamingSession::connect(&config).await {
            Ok(s) => {
                backoff = Duration::from_secs(1); // reset on successful connect
                s
            }
            Err(e) => {
                tracing::error!(error = %e, backoff_secs = backoff.as_secs(), "Connection failed, retrying...");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let stream_tx = match session.open_stream().await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open stream");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let mut flush_tick = interval(Duration::from_millis(config.flush_interval_ms));
        let mut batch: Vec<SecurityEvent> = Vec::with_capacity(config.batch_size);

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            batch.push(event);
                            if batch.len() >= config.batch_size {
                                flush_batch(&stream_tx, &mut batch).await;
                            }
                        }
                        None => {
                            // Collector channel closed — agent is shutting down
                            if !batch.is_empty() {
                                flush_batch(&stream_tx, &mut batch).await;
                            }
                            tracing::info!("Ingest loop: collector channel closed, exiting");
                            return;
                        }
                    }
                }

                _ = flush_tick.tick() => {
                    if !batch.is_empty() {
                        flush_batch(&stream_tx, &mut batch).await;
                    }
                }
            }

            if stream_tx.is_closed() {
                tracing::warn!("Stream closed by server, reconnecting...");
                break; // break inner loop → reconnect in outer loop
            }
        }
    }
}

async fn flush_batch(tx: &mpsc::Sender<SecurityEvent>, batch: &mut Vec<SecurityEvent>) {
    for event in batch.drain(..) {
        if let Err(e) = tx.send(event).await {
            tracing::error!(error = %e, "Stream tx closed during flush");
            return;
        }
    }
}
```

- [ ] **Step 7.2: Add mod client to main.rs**

```rust
mod config;
mod collector;
mod client;

pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("sentinelops-agent starting (stub — Task 8 will wire everything)");
    Ok(())
}
```

- [ ] **Step 7.3: Verify it compiles (no runtime test without real certs)**

```bash
cargo build -p sentinelops-agent
# Expected: compiles with no errors
```

- [ ] **Step 7.4: Commit**

```bash
git add sentinelops-agent/src/client.rs sentinelops-agent/src/main.rs
git commit -m "feat(agent): StreamingSession + run_ingest_loop with mTLS and exponential backoff"
```

---

### Task 8: main.rs — Full Agent Wiring

**Files:**
- Modify: `sentinelops-agent/src/main.rs`
- Create: `sentinelops-agent/.env.example`

- [ ] **Step 8.1: Wire all modules in main.rs**

Replace `sentinelops-agent/src/main.rs` with:

```rust
mod config;
mod collector;
mod client;

use std::sync::Arc;
use tokio::sync::mpsc;
use config::AgentMode;

// Proto types generated from proto/sentinel.proto at build time
pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cfg = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;

    tracing::info!(
        agent_id   = %cfg.agent_id,
        mode       = ?cfg.mode,
        host       = %cfg.source_host,
        "SentinelOps Agent starting"
    );

    match cfg.mode {
        AgentMode::Autonomous => {
            // Autonomous mode: sentinelops-agent does not run.
            // Deploy sentinelops-rust standalone instead.
            tracing::warn!("AGENT_MODE=autonomous — this binary should not be deployed in autonomous mode. Run sentinelops-rust directly.");
            return Ok(());
        }

        AgentMode::Subordinate | AgentMode::Hybrid => {
            let cfg = Arc::new(cfg);
            let (tx, rx) = mpsc::channel::<sentinel::SecurityEvent>(cfg.queue_capacity);

            // Spawn the Collector — produces events into the channel
            let collector_cfg = cfg.clone();
            tokio::spawn(async move {
                collector::run_collector(collector_cfg, tx).await;
            });

            // Run the ingest loop — consumes events and dispatches to Control Plane
            // This blocks until the channel closes (i.e., agent shuts down)
            client::run_ingest_loop(cfg, rx).await;
        }
    }

    Ok(())
}
```

- [ ] **Step 8.2: Create .env.example for the agent**

Create `sentinelops-agent/.env.example`:

```dotenv
# sentinelops-agent configuration
# Copy to .env and fill in values

# Required — MUST match CN in certs/agent.pem (lowercase UUID v4)
AGENT_ID=agent-uuid-1234

# Optional — defaults to system hostname
# SOURCE_HOST=my-server.internal

# gRPC Control Plane address
CONTROL_PLANE_ADDR=https://127.0.0.1:9090

# mTLS certificate paths (relative to working directory)
CA_CERT_PATH=certs/ca.pem
CLIENT_CERT_PATH=certs/agent.pem
CLIENT_KEY_PATH=certs/agent.key

# Batching / performance
BATCH_SIZE=500
FLUSH_INTERVAL_MS=1000
AGENT_QUEUE_CAPACITY=10000

# Mode: autonomous | subordinate | hybrid (default: hybrid)
AGENT_MODE=hybrid
```

- [ ] **Step 8.3: Verify full agent compiles**

`dotenvy` was already added to `Cargo.toml` in Task 3 Step 3.2 — no change needed here.

```bash
cargo build -p sentinelops-agent
# Expected: compiles successfully
```

- [ ] **Step 8.4: Run all agent tests**

```bash
cargo test -p sentinelops-agent
# Expected: all config and collector tests pass (back-pressure test included)
```

- [ ] **Step 8.5: Commit**

```bash
git add sentinelops-agent/src/main.rs sentinelops-agent/.env.example
git commit -m "feat(agent): wire main.rs — collector + ingest loop + AgentMode dispatch"
```

---

## Chunk 3: Java 21 Control Plane — sentinelops-control

### Task 9: Maven Project + Application Bootstrap

**Files:**
- Create: `sentinelops-control/pom.xml`
- Create: `sentinelops-control/src/main/java/com/sentinelops/SentinelOpsApplication.java`
- Create: `sentinelops-control/src/main/resources/application.yml`

- [ ] **Step 9.1: Create directory structure**

```bash
mkdir -p sentinelops-control/src/main/java/com/sentinelops/{config,grpc}
mkdir -p sentinelops-control/src/main/resources
mkdir -p sentinelops-control/src/test/java/com/sentinelops/grpc
```

- [ ] **Step 9.2: Create pom.xml**

Create `sentinelops-control/pom.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0
             https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>

  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.3.5</version>
    <relativePath/>
  </parent>

  <groupId>com.sentinelops</groupId>
  <artifactId>sentinelops-control</artifactId>
  <version>0.1.0-SNAPSHOT</version>
  <name>sentinelops-control</name>
  <description>SentinelOps Control Plane — Java 21 gRPC server</description>

  <properties>
    <java.version>21</java.version>
    <grpc.spring.boot.version>3.1.0.RELEASE</grpc.spring.boot.version>
  </properties>

  <dependencies>
    <!-- Spring Boot Web (actuator/health) -->
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-actuator</artifactId>
    </dependency>

    <!-- gRPC server + Spring Boot integration -->
    <dependency>
      <groupId>net.devh</groupId>
      <artifactId>grpc-server-spring-boot-starter</artifactId>
      <version>${grpc.spring.boot.version}</version>
    </dependency>

    <!-- Testing -->
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-test</artifactId>
      <scope>test</scope>
    </dependency>
    <dependency>
      <groupId>io.grpc</groupId>
      <artifactId>grpc-testing</artifactId>
      <version>1.63.0</version>
      <scope>test</scope>
    </dependency>
  </dependencies>

  <build>
    <!-- os-maven-plugin MUST be a build extension (not just a plugin) so that
         ${os.detected.classifier} is resolved before the protobuf-maven-plugin
         runs. Declaring it only as a <plugin> leaves the property unresolved. -->
    <extensions>
      <extension>
        <groupId>kr.motd.maven</groupId>
        <artifactId>os-maven-plugin</artifactId>
        <version>1.7.1</version>
      </extension>
    </extensions>

    <plugins>
      <plugin>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-maven-plugin</artifactId>
      </plugin>

      <!-- Compile proto → Java -->
      <plugin>
        <groupId>org.xolstice.maven.plugins</groupId>
        <artifactId>protobuf-maven-plugin</artifactId>
        <version>0.6.1</version>
        <configuration>
          <protocArtifact>com.google.protobuf:protoc:3.25.3:exe:${os.detected.classifier}</protocArtifact>
          <pluginId>grpc-java</pluginId>
          <pluginArtifact>io.grpc:protoc-gen-grpc-java:1.63.0:exe:${os.detected.classifier}</pluginArtifact>
          <protoSourceRoot>${project.basedir}/../proto</protoSourceRoot>
        </configuration>
        <executions>
          <execution>
            <goals>
              <goal>compile</goal>
              <goal>compile-custom</goal>
            </goals>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
```

- [ ] **Step 9.3: Create application entry point**

Create `sentinelops-control/src/main/java/com/sentinelops/SentinelOpsApplication.java`:

```java
package com.sentinelops;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class SentinelOpsApplication {
    public static void main(String[] args) {
        SpringApplication.run(SentinelOpsApplication.class, args);
    }
}
```

- [ ] **Step 9.4: Create application.yml**

Create `sentinelops-control/src/main/resources/application.yml`:

```yaml
spring:
  application:
    name: sentinelops-control
  threads:
    virtual:
      enabled: true   # Java 21 Virtual Threads (Project Loom) — Spring Boot ≥ 3.2 required

grpc:
  server:
    port: 9090
    security:
      enabled: true
      certificate-chain: classpath:certs/server.pem
      private-key:       classpath:certs/server.key
      trust-certificate-collection: classpath:certs/ca.pem
      client-auth: REQUIRE   # Enforce mTLS — no client cert = UNAUTHENTICATED

management:
  endpoints:
    web:
      exposure:
        include: health,info
  endpoint:
    health:
      show-details: always

logging:
  level:
    com.sentinelops: INFO
    io.grpc: WARN
```

- [ ] **Step 9.5: Compile the project**

```bash
cd sentinelops-control
mvn compile -q
# Expected: BUILD SUCCESS
# Proto files compiled to target/generated-sources/protobuf/
```

- [ ] **Step 9.6: Commit**

```bash
cd ..
git add sentinelops-control/
git commit -m "feat(control): Java 21 Spring Boot project scaffold with gRPC + proto compilation"
```

---

### Task 10: AgentIdentityInterceptor + Tests

**Files:**
- Create: `sentinelops-control/src/main/java/com/sentinelops/grpc/AgentIdentityInterceptor.java`
- Create: `sentinelops-control/src/test/java/com/sentinelops/grpc/AgentIdentityInterceptorTest.java`

- [ ] **Step 10.1: Write the failing test first**

Create `sentinelops-control/src/test/java/com/sentinelops/grpc/AgentIdentityInterceptorTest.java`:

```java
package com.sentinelops.grpc;

import io.grpc.*;
import org.junit.jupiter.api.Test;

import javax.net.ssl.SSLPeerUnverifiedException;
import javax.net.ssl.SSLSession;
import java.security.Principal;
import java.security.cert.Certificate;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class AgentIdentityInterceptorTest {

    private final AgentIdentityInterceptor interceptor = new AgentIdentityInterceptor();

    @Test
    void whenNoCert_callIsRejectedWithUnauthenticated() {
        @SuppressWarnings("unchecked")
        ServerCall<Object, Object> call = mock(ServerCall.class);
        Attributes attributes = Attributes.newBuilder()
            .set(Grpc.TRANSPORT_ATTR_SSL_SESSION, null)
            .build();
        when(call.getAttributes()).thenReturn(attributes);

        ServerCallHandler<Object, Object> next = mock(ServerCallHandler.class);

        interceptor.interceptCall(call, new Metadata(), next);

        verify(call).close(
            argThat(s -> s.getCode() == Status.Code.UNAUTHENTICATED),
            any(Metadata.class)
        );
        verify(next, never()).startCall(any(), any());
    }

    @Test
    void whenValidCert_cnIsNormalisedToLowercase() throws Exception {
        String mixedCaseCN = "Agent-UUID-1234"; // simulating a cert with mixed-case CN
        SSLSession sslSession = mockSslSession(mixedCaseCN);

        @SuppressWarnings("unchecked")
        ServerCall<Object, Object> call = mock(ServerCall.class);
        Attributes attributes = Attributes.newBuilder()
            .set(Grpc.TRANSPORT_ATTR_SSL_SESSION, sslSession)
            .build();
        when(call.getAttributes()).thenReturn(attributes);

        @SuppressWarnings("unchecked")
        ServerCallHandler<Object, Object> next = mock(ServerCallHandler.class);
        when(next.startCall(any(), any())).thenReturn(mock(ServerCall.Listener.class));

        // Capture the Context by wrapping the interceptor call
        final String[] capturedCN = {null};
        ServerCallHandler<Object, Object> capturingNext = (c, m) -> {
            capturedCN[0] = AgentIdentityInterceptor.CERT_CN.get();
            return mock(ServerCall.Listener.class);
        };

        interceptor.interceptCall(call, new Metadata(), capturingNext);

        assertEquals("agent-uuid-1234", capturedCN[0],
            "CN must be normalised to lowercase before storing in Context");
    }

    private SSLSession mockSslSession(String cn) throws SSLPeerUnverifiedException {
        SSLSession session = mock(SSLSession.class);
        Principal principal = mock(Principal.class);
        // X500Principal format: "CN=value,O=org,C=country"
        when(principal.getName()).thenReturn("CN=" + cn + ",O=SentinelOps,C=BR");

        java.security.cert.X509Certificate cert = mock(java.security.cert.X509Certificate.class);
        javax.security.auth.x500.X500Principal x500 = new javax.security.auth.x500.X500Principal("CN=" + cn);
        when(cert.getSubjectX500Principal()).thenReturn(x500);
        when(session.getPeerCertificates()).thenReturn(new Certificate[]{ cert });

        return session;
    }
}
```

- [ ] **Step 10.2: Run test — expect compilation failure (class doesn't exist)**

```bash
cd sentinelops-control
mvn test -pl . -Dtest=AgentIdentityInterceptorTest 2>&1 | tail -5
# Expected: COMPILATION ERROR — AgentIdentityInterceptor not found
```

- [ ] **Step 10.3: Create the interceptor**

Create `sentinelops-control/src/main/java/com/sentinelops/grpc/AgentIdentityInterceptor.java`:

```java
package com.sentinelops.grpc;

import io.grpc.*;
import net.devh.boot.grpc.server.interceptor.GrpcGlobalServerInterceptor;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import javax.net.ssl.SSLPeerUnverifiedException;
import javax.net.ssl.SSLSession;
import java.security.cert.X509Certificate;

/**
 * Zero Trust gRPC interceptor.
 *
 * Extracts the Common Name (CN) from the mTLS client certificate and stores it
 * in the gRPC Context so downstream service implementations can validate the
 * agent_id field in the SecurityEvent payload against the certified identity.
 *
 * Normalises CN to lowercase to match the AGENT_ID env var convention (spec §5.2).
 */
@GrpcGlobalServerInterceptor
public class AgentIdentityInterceptor implements ServerInterceptor {

    private static final Logger log = LoggerFactory.getLogger(AgentIdentityInterceptor.class);

    /** gRPC Context key carrying the verified cert CN (lowercase) for service access. */
    public static final Context.Key<String> CERT_CN = Context.key("cert-cn");

    @Override
    public <Q, R> ServerCall.Listener<Q> interceptCall(
        ServerCall<Q, R> call, Metadata headers, ServerCallHandler<Q, R> next
    ) {
        String cn = extractCN(call);

        if (cn == null) {
            log.warn("gRPC call received without client certificate — rejecting (Zero Trust)");
            call.close(
                Status.UNAUTHENTICATED.withDescription("mTLS client certificate is required"),
                headers
            );
            return new ServerCall.Listener<>() {};
        }

        // Normalise to lowercase: cert CNs are case-insensitive per RFC 5280,
        // but X500Principal.getName() returns the literal string. Normalising here
        // prevents a mixed-case cert CN from failing equality checks with AGENT_ID.
        Context ctx = Context.current().withValue(CERT_CN, cn.toLowerCase());
        return Contexts.interceptCall(ctx, call, headers, next);
    }

    private <Q, R> String extractCN(ServerCall<Q, R> call) {
        SSLSession ssl = call.getAttributes().get(Grpc.TRANSPORT_ATTR_SSL_SESSION);
        if (ssl == null) return null;

        try {
            X509Certificate cert = (X509Certificate) ssl.getPeerCertificates()[0];
            String dn = cert.getSubjectX500Principal().getName();
            // DN format: "CN=value,O=org,C=BR" — may contain escaped commas in values
            for (String part : dn.split(",")) {
                String trimmed = part.trim();
                if (trimmed.startsWith("CN=")) {
                    return trimmed.substring(3);
                }
            }
        } catch (SSLPeerUnverifiedException e) {
            log.error("Failed to verify peer certificate", e);
        } catch (ArrayIndexOutOfBoundsException e) {
            log.error("Peer certificate chain is empty", e);
        }

        return null;
    }
}
```

- [ ] **Step 10.4: Run tests — expect pass**

```bash
mvn test -Dtest=AgentIdentityInterceptorTest
# Expected:
#   Tests run: 2, Failures: 0, Errors: 0, Skipped: 0
```

- [ ] **Step 10.5: Commit**

```bash
cd ..
git add sentinelops-control/src/main/java/com/sentinelops/grpc/AgentIdentityInterceptor.java \
        sentinelops-control/src/test/java/com/sentinelops/grpc/AgentIdentityInterceptorTest.java
git commit -m "feat(control): AgentIdentityInterceptor — Zero Trust CN extraction + lowercase normalisation + tests"
```

---

### Task 11: IngestionServiceImpl + Tests

**Files:**
- Create: `sentinelops-control/src/main/java/com/sentinelops/grpc/IngestionServiceImpl.java`
- Create: `sentinelops-control/src/test/java/com/sentinelops/grpc/IngestionServiceImplTest.java`

- [ ] **Step 11.1: Write the failing tests**

Create `sentinelops-control/src/test/java/com/sentinelops/grpc/IngestionServiceImplTest.java`:

```java
package com.sentinelops.grpc;

import io.grpc.*;
import io.grpc.inprocess.InProcessChannelBuilder;
import io.grpc.inprocess.InProcessServerBuilder;
import io.grpc.stub.StreamObserver;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.*;

// NOTE: We manage the in-process server and channel lifecycle with @BeforeEach / @AfterEach
// rather than @Rule GrpcCleanupRule (which is JUnit 4 and silently ignored by JUnit 5).
class IngestionServiceImplTest {

    private io.grpc.Server inProcessServer;
    private ManagedChannel channel;
    private IngestionServiceGrpc.IngestionServiceStub asyncStub;
    private IngestionServiceGrpc.IngestionServiceBlockingStub blockingStub;

    /** Sets up an in-process gRPC server with CERT_CN injected via Context interceptor. */
    @BeforeEach
    void setUp() throws Exception {
        String serverName = InProcessServerBuilder.generateName();
        String testCN     = "test-agent-uuid";

        // Simulate the AgentIdentityInterceptor by injecting CERT_CN into context
        ServerInterceptor cnInjector = new ServerInterceptor() {
            @Override
            public <Q, R> ServerCall.Listener<Q> interceptCall(
                ServerCall<Q, R> call, Metadata headers, ServerCallHandler<Q, R> next
            ) {
                Context ctx = Context.current().withValue(AgentIdentityInterceptor.CERT_CN, testCN);
                return Contexts.interceptCall(ctx, call, headers, next);
            }
        };

        inProcessServer = InProcessServerBuilder.forName(serverName)
            .intercept(cnInjector)
            .addService(new IngestionServiceImpl())
            .build()
            .start();

        channel = InProcessChannelBuilder.forName(serverName).directExecutor().build();

        asyncStub    = IngestionServiceGrpc.newStub(channel);
        blockingStub = IngestionServiceGrpc.newBlockingStub(channel);
    }

    @AfterEach
    void tearDown() throws InterruptedException {
        channel.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
        inProcessServer.shutdownNow().awaitTermination(5, TimeUnit.SECONDS);
    }

    // ── Unary ─────────────────────────────────────────────────────────────────

    @Test
    void sendEvent_whenAgentIdMatchesCert_returnsAccepted() {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-001")
            .setAgentId("test-agent-uuid")   // matches injected CERT_CN
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.1")
            .build();

        EventResponse response = blockingStub.sendEvent(event);

        assertTrue(response.getAccepted());
        assertEquals("evt-001", response.getEventId());
    }

    @Test
    void sendEvent_whenAgentIdMismatch_returnsRejected() {
        SecurityEvent event = SecurityEvent.newBuilder()
            .setEventId("evt-002")
            .setAgentId("spoofed-agent-id")  // does NOT match CERT_CN
            .setEventType("auth.login.failed")
            .setSourceIp("10.0.0.1")
            .build();

        EventResponse response = blockingStub.sendEvent(event);

        assertFalse(response.getAccepted());
        assertTrue(response.getMessage().contains("agent_id mismatch"));
    }

    // ── Streaming ─────────────────────────────────────────────────────────────

    @Test
    void streamEvents_countsAcceptedAndRejected() throws InterruptedException {
        CountDownLatch done = new CountDownLatch(1);
        AtomicReference<StreamSummary> summaryRef = new AtomicReference<>();

        StreamObserver<SecurityEvent> requestObserver = asyncStub.streamEvents(
            new StreamObserver<StreamSummary>() {
                @Override public void onNext(StreamSummary s)      { summaryRef.set(s); }
                @Override public void onError(Throwable t)         { done.countDown(); }
                @Override public void onCompleted()                { done.countDown(); }
            }
        );

        // Send 2 valid events + 1 spoofed
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e1").setAgentId("test-agent-uuid").setEventType("network.scan").build());
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e2").setAgentId("test-agent-uuid").setEventType("auth.login.failed").build());
        requestObserver.onNext(SecurityEvent.newBuilder()
            .setEventId("e3").setAgentId("spoofed-agent").setEventType("auth.login.failed").build());
        requestObserver.onCompleted();

        assertTrue(done.await(3, TimeUnit.SECONDS), "Stream did not complete in time");

        StreamSummary summary = summaryRef.get();
        assertNotNull(summary);
        assertEquals(2, summary.getAcceptedCount());
        assertEquals(1, summary.getRejectedCount());
        assertFalse(summary.getSessionId().isEmpty());
    }
}
```

- [ ] **Step 11.2: Run test — expect compilation failure**

```bash
cd sentinelops-control
mvn test -Dtest=IngestionServiceImplTest 2>&1 | tail -5
# Expected: COMPILATION ERROR — IngestionServiceImpl not found
```

- [ ] **Step 11.3: Create the service implementation**

Create `sentinelops-control/src/main/java/com/sentinelops/grpc/IngestionServiceImpl.java`:

```java
package com.sentinelops.grpc;

import com.sentinelops.grpc.*;
import io.grpc.stub.StreamObserver;
import net.devh.boot.grpc.server.service.GrpcService;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.UUID;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * gRPC IngestionService implementation.
 *
 * Zero Trust contract: the agent_id field in every SecurityEvent MUST match the
 * cert CN stored in the gRPC Context by AgentIdentityInterceptor. Mismatches are
 * logged and counted as rejected, never silently passed.
 */
@GrpcService
public class IngestionServiceImpl extends IngestionServiceGrpc.IngestionServiceImplBase {

    private static final Logger log = LoggerFactory.getLogger(IngestionServiceImpl.class);

    // ── Unary ─────────────────────────────────────────────────────────────────

    @Override
    public void sendEvent(SecurityEvent req, StreamObserver<EventResponse> out) {
        String certCN = AgentIdentityInterceptor.CERT_CN.get();

        if (!req.getAgentId().equals(certCN)) {
            log.warn("Zero Trust violation [unary]: payload agent_id='{}' cert CN='{}'",
                     req.getAgentId(), certCN);
            out.onNext(EventResponse.newBuilder()
                .setAccepted(false)
                .setEventId(req.getEventId())
                .setMessage("agent_id mismatch — Zero Trust violation")
                .build());
            out.onCompleted();
            return;
        }

        log.info("[UNARY] type={} agent={} event={}", req.getEventType(), req.getAgentId(), req.getEventId());

        // TODO: publish to Kafka / persist to PostgreSQL

        out.onNext(EventResponse.newBuilder()
            .setAccepted(true)
            .setEventId(req.getEventId())
            .setMessage("ACK")
            .build());
        out.onCompleted();
    }

    // ── Client-side streaming ──────────────────────────────────────────────────

    @Override
    public StreamObserver<SecurityEvent> streamEvents(StreamObserver<StreamSummary> out) {
        String certCN    = AgentIdentityInterceptor.CERT_CN.get();
        String sessionId = UUID.randomUUID().toString();
        AtomicInteger accepted = new AtomicInteger(0);
        AtomicInteger rejected = new AtomicInteger(0);

        log.info("[STREAM] session={} agent(cert)={}", sessionId, certCN);

        return new StreamObserver<>() {

            @Override
            public void onNext(SecurityEvent event) {
                if (!event.getAgentId().equals(certCN)) {
                    log.warn("Zero Trust violation [stream] session={}: payload agent_id='{}' cert CN='{}'",
                             sessionId, event.getAgentId(), certCN);
                    rejected.incrementAndGet();
                    return;
                }

                // TODO: publish to Kafka / persist to PostgreSQL
                accepted.incrementAndGet();
            }

            @Override
            public void onError(Throwable t) {
                log.error("[STREAM] error session={} agent={}: {}", sessionId, certCN, t.getMessage());
            }

            @Override
            public void onCompleted() {
                log.info("[STREAM] closed session={} accepted={} rejected={}",
                         sessionId, accepted.get(), rejected.get());

                out.onNext(StreamSummary.newBuilder()
                    .setAcceptedCount(accepted.get())
                    .setRejectedCount(rejected.get())
                    .setSessionId(sessionId)
                    .build());
                out.onCompleted();
            }
        };
    }
}
```

- [ ] **Step 11.4: Run tests — expect all pass**

```bash
mvn test -Dtest=IngestionServiceImplTest
# Expected:
#   Tests run: 3, Failures: 0, Errors: 0, Skipped: 0
```

- [ ] **Step 11.5: Run full test suite**

```bash
mvn test
# Expected: all tests pass (interceptor + service tests)
```

- [ ] **Step 11.6: Commit**

```bash
cd ..
git add sentinelops-control/src/main/java/com/sentinelops/grpc/IngestionServiceImpl.java \
        sentinelops-control/src/test/java/com/sentinelops/grpc/IngestionServiceImplTest.java
git commit -m "feat(control): IngestionServiceImpl — unary + streaming with Zero Trust agent_id validation + tests"
```

---

### Task 12: GrpcServerConfig (Virtual Thread Executor)

**Files:**
- Create: `sentinelops-control/src/main/java/com/sentinelops/config/GrpcServerConfig.java`

- [ ] **Step 12.1: Create the Virtual Thread configurer**

Create `sentinelops-control/src/main/java/com/sentinelops/config/GrpcServerConfig.java`:

```java
package com.sentinelops.config;

import net.devh.boot.grpc.server.config.GrpcServerConfigurer;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;

import java.util.concurrent.Executors;

/**
 * Configures the gRPC server to dispatch each incoming call to a Java 21 Virtual Thread.
 *
 * Virtual Threads (Project Loom) allow blocking I/O (DB, Kafka) inside gRPC handlers
 * without consuming OS threads. This enables the server to handle tens of thousands
 * of concurrent agent connections on modest hardware.
 *
 * Requires: Java 21 and spring.threads.virtual.enabled=true in application.yml.
 */
@Configuration
public class GrpcServerConfig {

    @Bean
    public GrpcServerConfigurer virtualThreadExecutor() {
        return serverBuilder ->
            serverBuilder.executor(Executors.newVirtualThreadPerTaskExecutor());
    }
}
```

- [ ] **Step 12.2: Verify full build still passes**

```bash
cd sentinelops-control
mvn test
# Expected: BUILD SUCCESS — all tests pass
```

- [ ] **Step 12.3: Commit**

```bash
cd ..
git add sentinelops-control/src/main/java/com/sentinelops/config/GrpcServerConfig.java
git commit -m "feat(control): GrpcServerConfig — Virtual Thread executor for gRPC handlers (Java 21)"
```

---

## Post-Implementation Verification

- [ ] **Verify full workspace compiles**

```bash
cargo build --workspace
# Expected: both sentinelops-rust and sentinelops-agent compile
```

- [ ] **Verify all Rust tests pass**

```bash
cargo test --workspace
# Expected: all config, middleware/auth, collector tests pass
```

- [ ] **Verify Java project compiles and tests pass**

```bash
cd sentinelops-control && mvn test && cd ..
# Expected: BUILD SUCCESS
```

- [ ] **Generate dev certs and verify chain**

```bash
./scripts/gen-dev-certs.sh agent-uuid-1234
# Expected: "server.pem OK" and "agent.pem OK"
```

- [ ] **Final commit tag**

```bash
git tag v0.3.0-dual-stack-foundation
git log --oneline -10
```
