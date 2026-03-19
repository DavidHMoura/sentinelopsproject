use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::{Validate, ValidationError};

fn validate_ip(ip: &str) -> Result<(), ValidationError> {
    ip.parse::<std::net::IpAddr>()
        .map(|_| ())
        .map_err(|_| ValidationError::new("invalid_ip"))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EventIn {
    pub ts: DateTime<Utc>,
    #[validate(length(min = 1, max = 100))]
    pub event_type: String,
    #[validate(custom(function = "validate_ip"))]
    pub source_ip: String,
    #[validate(length(max = 255))]
    pub actor: Option<String>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Event {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub event_type: String,
    pub source_ip: String,
    pub actor: Option<String>,
    pub meta: serde_json::Value,
}

impl Event {
    pub fn from_input(input: EventIn) -> Self {
        Self {
            id: Uuid::new_v4(),
            ts: input.ts,
            event_type: input.event_type,
            source_ip: input.source_ip,
            actor: input.actor,
            meta: input.meta,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Alert {
    pub id: Uuid,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub fingerprint: String,
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn base_event(ip: &str) -> EventIn {
        EventIn {
            ts: Utc::now(),
            event_type: "auth.login.failed".to_string(),
            source_ip: ip.to_string(),
            actor: None,
            meta: serde_json::json!({}),
        }
    }

    #[test]
    fn valid_ipv4_accepted() {
        assert!(base_event("192.168.1.1").validate().is_ok());
    }

    #[test]
    fn valid_ipv6_accepted() {
        assert!(base_event("::1").validate().is_ok());
        assert!(base_event("2001:db8::1").validate().is_ok());
    }

    #[test]
    fn invalid_ip_rejected() {
        assert!(base_event("not_an_ip").validate().is_err());
        assert!(base_event("999.999.999.999").validate().is_err());
        assert!(base_event("1.2.3").validate().is_err());
        assert!(base_event("").validate().is_err());
    }
}
