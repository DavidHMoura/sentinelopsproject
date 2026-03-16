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

        // Enforce lowercase to match cert CN invariant
        let agent_id = agent_id.to_lowercase();

        let source_host = env::var("SOURCE_HOST")
            .unwrap_or_else(|_| {
                hostname::get()
                    .map(|h| h.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "unknown-host".to_string())
            });

        let batch_size: usize = env::var("BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);

        if batch_size < 1 {
            return Err("BATCH_SIZE must be >= 1".to_string());
        }

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
            batch_size,
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
                _                 => AgentMode::Hybrid,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    #[test]
    fn test_batch_size_zero_rejected() {
        let _g = env_lock().lock().unwrap();
        env::set_var("AGENT_ID", "test-agent");
        env::set_var("BATCH_SIZE", "0");
        let result = AgentConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("BATCH_SIZE"));
        env::remove_var("AGENT_ID");
        env::remove_var("BATCH_SIZE");
    }
}
