use crate::errors::{SentinelError, SentinelResult};
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub auth_max_attempts: i64,
    pub auth_window_seconds: i64,
    pub api_keys: Vec<String>,
    pub server_host: String,
    pub server_port: u16,
}

impl Config {
    pub fn from_env() -> SentinelResult<Self> {
        dotenvy::dotenv().ok();

        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| {
                tracing::warn!("DATABASE_URL not set, using default");
                "postgres://sentinelops:sentinelops@localhost:5432/sentinelops".to_string()
            });

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

        tracing::info!(
            "Configuration loaded: {} API key(s), server {}:{}",
            api_keys.len(),
            server_host,
            server_port
        );

        Ok(Self {
            database_url,
            auth_max_attempts,
            auth_window_seconds,
            api_keys,
            server_host,
            server_port,
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

    #[test]
    fn test_config_missing_api_keys() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        env::set_var("API_KEYS", "");
        let result = Config::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SentinelError::ConfigError(_)));
    }

    #[test]
    fn test_config_valid() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        env::set_var("API_KEYS", "key1,key2,key3");
        env::set_var("DATABASE_URL", "postgres://test");
        
        let result = Config::from_env();
        assert!(result.is_ok());
        
        let config = result.unwrap();
        assert_eq!(config.api_keys.len(), 3);
        assert_eq!(config.api_keys[0], "key1");
    }
}
