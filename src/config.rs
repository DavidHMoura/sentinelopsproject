use crate::errors::{SentinelError, SentinelResult};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub auth_max_attempts: i64,
    pub auth_window_seconds: i64,
    pub port_scan_max_ports: i64,
    pub port_scan_window_seconds: i64,
    pub api_keys: Vec<String>,
    pub server_host: String,
    pub server_port: u16,
    pub ingestor_queue_capacity: usize,
    pub ingestor_batch_size: usize,
    pub ingestor_flush_ms: u64,
}

impl Config {
    pub fn from_env() -> SentinelResult<Self> {
        // DATABASE_URL é obrigatório — não há fallback com credenciais hardcoded.
        let database_url = env::var("DATABASE_URL").map_err(|_| {
            SentinelError::ConfigError(
                "DATABASE_URL environment variable is required".to_string(),
            )
        })?;

        if database_url.is_empty() {
            return Err(SentinelError::ConfigError(
                "DATABASE_URL must not be empty".to_string(),
            ));
        }

        let db_max_connections = env::var("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "20".to_string())
            .parse::<u32>()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid DB_MAX_CONNECTIONS: {}", e))
            })?;

        let db_min_connections = env::var("DB_MIN_CONNECTIONS")
            .unwrap_or_else(|_| "2".to_string())
            .parse::<u32>()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid DB_MIN_CONNECTIONS: {}", e))
            })?;

        if db_min_connections > db_max_connections {
            return Err(SentinelError::ConfigError(
                "DB_MIN_CONNECTIONS must be <= DB_MAX_CONNECTIONS".to_string(),
            ));
        }

        let auth_max_attempts = env::var("AUTH_MAX_ATTEMPTS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid AUTH_MAX_ATTEMPTS: {}", e))
            })?;

        let auth_window_seconds = env::var("AUTH_WINDOW_SECONDS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid AUTH_WINDOW_SECONDS: {}", e))
            })?;

        let port_scan_max_ports = env::var("PORT_SCAN_MAX_PORTS")
            .unwrap_or_else(|_| "20".to_string())
            .parse()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid PORT_SCAN_MAX_PORTS: {}", e))
            })?;

        let port_scan_window_seconds = env::var("PORT_SCAN_WINDOW_SECONDS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .map_err(|e| {
                SentinelError::ConfigError(format!("Invalid PORT_SCAN_WINDOW_SECONDS: {}", e))
            })?;

        let api_keys_str = env::var("API_KEYS").map_err(|_| {
            SentinelError::ConfigError(
                "API_KEYS environment variable is required".to_string()
            )
        })?;

        let api_keys: Vec<String> = api_keys_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if api_keys.is_empty() {
            return Err(SentinelError::ConfigError(
                "At least one API key must be configured".to_string(),
            ));
        }

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8000".to_string())
            .parse()
            .map_err(|e| SentinelError::ConfigError(format!("Invalid SERVER_PORT: {}", e)))?;

        let ingestor_queue_capacity = env::var("INGESTOR_QUEUE_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);

        let ingestor_batch_size = env::var("INGESTOR_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let ingestor_flush_ms = env::var("INGESTOR_FLUSH_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3_000);

        tracing::info!(
            "Configuration loaded: {} API key(s), server {}:{}, pool {}-{}",
            api_keys.len(),
            server_host,
            server_port,
            db_min_connections,
            db_max_connections,
        );

        Ok(Self {
            database_url,
            db_max_connections,
            db_min_connections,
            auth_max_attempts,
            auth_window_seconds,
            port_scan_max_ports,
            port_scan_window_seconds,
            api_keys,
            server_host,
            server_port,
            ingestor_queue_capacity,
            ingestor_batch_size,
            ingestor_flush_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Adquire o lock, recuperando de lock envenenado por pânico em outros testes.
    macro_rules! lock_env {
        () => {
            env_lock().lock().unwrap_or_else(|p| p.into_inner())
        };
    }

    fn set_required_vars() {
        env::set_var("DATABASE_URL", "postgres://test:test@localhost/testdb");
        env::set_var("API_KEYS", "test-key");
    }

    fn clear_all_test_vars() {
        for var in &[
            "DATABASE_URL", "API_KEYS", "DB_MAX_CONNECTIONS", "DB_MIN_CONNECTIONS",
            "INGESTOR_QUEUE_CAPACITY", "INGESTOR_BATCH_SIZE", "INGESTOR_FLUSH_MS",
        ] {
            env::remove_var(var);
        }
    }

    #[test]
    fn test_database_url_required() {
        let _guard = lock_env!();
        clear_all_test_vars();
        env::set_var("API_KEYS", "key1");

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SentinelError::ConfigError(_)));
        assert!(err.to_string().contains("DATABASE_URL"));
        clear_all_test_vars();
    }

    #[test]
    fn test_config_missing_api_keys() {
        let _guard = lock_env!();
        clear_all_test_vars();
        env::set_var("DATABASE_URL", "postgres://test:test@localhost/testdb");
        env::set_var("API_KEYS", "");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SentinelError::ConfigError(_)));
        clear_all_test_vars();
    }

    #[test]
    fn test_config_valid() {
        let _guard = lock_env!();
        clear_all_test_vars();
        set_required_vars();
        env::set_var("API_KEYS", "key1,key2,key3");

        let result = Config::from_env();
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.api_keys.len(), 3);
        assert_eq!(config.api_keys[0], "key1");
        assert_eq!(config.port_scan_max_ports, 20);
        assert_eq!(config.db_max_connections, 20);
        assert_eq!(config.ingestor_batch_size, 100);
        clear_all_test_vars();
    }

    #[test]
    fn test_pool_min_greater_than_max_rejected() {
        let _guard = lock_env!();
        clear_all_test_vars();
        set_required_vars();
        env::set_var("DB_MIN_CONNECTIONS", "10");
        env::set_var("DB_MAX_CONNECTIONS", "5");

        let result = Config::from_env();
        assert!(result.is_err());
        clear_all_test_vars();
    }
}
