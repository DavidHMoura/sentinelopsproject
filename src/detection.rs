use crate::models::{Alert, Event};
use crate::config::Config;
use crate::errors::SentinelResult;
use chrono::{Utc, DateTime, Duration};
use uuid::Uuid;
use dashmap::DashMap;
use std::sync::Arc;

pub struct DetectionEngine {
    state: Arc<DashMap<String, (u32, DateTime<Utc>, Option<DateTime<Utc>>)>>,
}

impl DetectionEngine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(DashMap::new()),
        }
    }

    pub async fn evaluate(&self, config: &Config, event: &Event) -> SentinelResult<Option<Alert>> {
        if event.event_type != "auth.login.failed" {
            return Ok(None);
        }

        let now = Utc::now();
        let actor_key = event.actor.as_deref().unwrap_or("unknown");
        let key = format!("{}:{}", event.source_ip, actor_key);
        let window = Duration::seconds(config.auth_window_seconds as i64);

        let mut should_alert = false;
        let count_out;

        {
            let mut entry = self.state.entry(key.clone()).or_insert((0, now, None));
            let (count, start_time, last_alert) = entry.value_mut();

            if now.signed_duration_since(*start_time) > window {
                *count = 1;
                *start_time = now;
            } else {
                *count += 1;
            }

            count_out = *count;

            let threshold_met = count_out >= config.auth_max_attempts as u32;
            let cooldown_ok = last_alert.map_or(true, |ts| {
                now.signed_duration_since(ts) > Duration::seconds(60)
            });

            if threshold_met && cooldown_ok {
                should_alert = true;
                *last_alert = Some(now);
            }
        }

        if !should_alert {
            return Ok(None);
        }

        Ok(Some(self.build_alert(event, count_out, config.auth_window_seconds as u64)))
    }

    fn build_alert(&self, event: &Event, attempts: u32, window: u64) -> Alert {
        let fingerprint = format!(
            "bf:{}:{}:{}",
            event.source_ip,
            event.actor.as_deref().unwrap_or("none"),
            Utc::now().format("%Y-%m-%d-%H")
        );

        Alert {
            id: Uuid::new_v4(),
            severity: "high".to_string(),
            status: "new".to_string(),
            title: "Brute-force detected".to_string(),
            description: format!(
                "Source {} reached {} attempts in {}s",
                event.source_ip, attempts, window
            ),
            fingerprint,
            evidence: serde_json::json!({
                "source_ip": event.source_ip,
                "actor": event.actor,
                "attempts": attempts,
                "engine": "l1_mem"
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
