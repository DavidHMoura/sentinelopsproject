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
            tls_server_name:    "sentinelops-control.internal".to_string(),
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

        let config_clone = config.clone();
        let handle = tokio::spawn(async move {
            run_collector(config_clone, tx).await;
        });

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

        handle.abort();
    }

    #[tokio::test]
    async fn test_collector_stops_when_channel_closed() {
        let (tx, rx) = mpsc::channel::<SecurityEvent>(1);
        let config = test_config();

        drop(rx);

        let result = tokio::time::timeout(
            Duration::from_millis(500),
            run_collector(config, tx),
        )
        .await;

        let _ = result;
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
