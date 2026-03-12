use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct EventIn {
    pub ts: DateTime<Utc>,
    #[validate(length(min = 1, max = 100))]
    pub event_type: String,
    #[validate(length(min = 7, max = 45))]
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