use chrono::Utc;
use sentinelops_rust::{config::Config, detection::DetectionEngine, models::Event};
use uuid::Uuid;

fn test_config() -> Config {
    Config {
        database_url: "postgres://testuser:testpass@localhost:5432/testdb".to_string(),
        db_max_connections: 20,
        db_min_connections: 2,
        auth_max_attempts: 3,
        auth_window_seconds: 300,
        port_scan_max_ports: 20,
        port_scan_window_seconds: 10,
        api_keys: vec!["test-key".to_string()],
        server_host: "127.0.0.1".to_string(),
        server_port: 8000,
        ingestor_queue_capacity: 1000,
        ingestor_batch_size: 100,
        ingestor_flush_ms: 3000,
    }
}

fn create_test_event(event_type: &str, source_ip: &str, actor: Option<String>) -> Event {
    Event {
        id: Uuid::new_v4(),
        ts: Utc::now(),
        event_type: event_type.to_string(),
        source_ip: source_ip.to_string(),
        actor,
        meta: serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_rt::test]
    async fn test_bruteforce_detection_threshold() {
        let config = test_config();
        let engine = DetectionEngine::new();
        let test_event = create_test_event(
            "auth.login.failed",
            "192.168.100.1",
            Some("test@example.com".to_string()),
        );

        for _ in 0..(config.auth_max_attempts - 1) {
            let result = engine.evaluate(&config, &test_event).await;
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }

        let result = engine.evaluate(&config, &test_event).await;
        assert!(result.is_ok());

        let alert = result.unwrap();
        assert!(alert.is_some());

        let alert = alert.unwrap();
        assert_eq!(alert.severity, "high");
        assert_eq!(alert.title, "Brute-force detected");
    }

    #[actix_rt::test]
    async fn test_non_auth_events_ignored() {
        let config = test_config();
        let engine = DetectionEngine::new();
        let event = create_test_event("network.scan", "192.168.100.2", None);

        let result = engine.evaluate(&config, &event).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
