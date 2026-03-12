use crate::models::{Alert, Event};
use crate::config::Config;
use crate::errors::SentinelResult;
use chrono::{Utc, DateTime, Duration};
use uuid::Uuid;
use dashmap::DashMap;
use std::sync::Arc;
use std::collections::HashSet;

pub struct DetectionEngine {
    bf_state: Arc<DashMap<String, (u32, DateTime<Utc>, Option<DateTime<Utc>>)>>,
    ps_state: Arc<DashMap<String, (HashSet<u16>, DateTime<Utc>, Option<DateTime<Utc>>)>>,
}

impl DetectionEngine {
    pub fn new() -> Self {
        Self {
            bf_state: Arc::new(DashMap::new()),
            ps_state: Arc::new(DashMap::new()),
        }
    }

    pub async fn evaluate(&self, config: &Config, event: &Event) -> SentinelResult<Option<Alert>> {
        match event.event_type.as_str() {
            "auth.login.failed" => self.eval_bruteforce(config, event).await,
            "network.scan" => self.eval_portscan(config, event).await,
            _ => Ok(None),
        }
    }

    async fn eval_bruteforce(&self, config: &Config, event: &Event) -> SentinelResult<Option<Alert>> {
        let now = Utc::now();
        let actor_key = event.actor.as_deref().unwrap_or("unknown");
        let key = format!("{}:{}", event.source_ip, actor_key);
        let window = Duration::seconds(config.auth_window_seconds as i64);

        let mut should_alert = false;
        let count_out;

        {
            let mut entry = self.bf_state.entry(key.clone()).or_insert((0, now, None));
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

        Ok(Some(self.build_alert(
            event, 
            "Brute-force detected", 
            format!("Source {} reached {} attempts in {}s", event.source_ip, count_out, window.num_seconds()),
            "high",
            serde_json::json!({ "attempts": count_out })
        )))
    }

    async fn eval_portscan(&self, config: &Config, event: &Event) -> SentinelResult<Option<Alert>> {
        let target_port = match event.meta.get("target_port").and_then(|p| p.as_u64()) {
            Some(p) => p as u16,
            None => return Ok(None), 
        };

        let now = Utc::now();
        let key = event.source_ip.clone(); 
        
        // Consumindo do config dinâmico
        let ps_window = Duration::seconds(config.port_scan_window_seconds); 
        let ps_threshold = config.port_scan_max_ports as usize; 

        let mut should_alert = false;
        let ports_count;

        {
            let mut entry = self.ps_state.entry(key).or_insert_with(|| (HashSet::new(), now, None));
            let (ports, start_time, last_alert) = entry.value_mut();

            if now.signed_duration_since(*start_time) > ps_window {
                ports.clear();
                *start_time = now;
            }
            
            ports.insert(target_port);
            ports_count = ports.len();

            let threshold_met = ports_count >= ps_threshold;
            let cooldown_ok = last_alert.map_or(true, |ts| {
                now.signed_duration_since(ts) > Duration::seconds(120)
            });

            if threshold_met && cooldown_ok {
                should_alert = true;
                *last_alert = Some(now);
            }
        }

        if !should_alert {
            return Ok(None);
        }

        Ok(Some(self.build_alert(
            event, 
            "Horizontal Port Scan Detected", 
            format!("Source IP {} scanned {} distinct ports in {}s", event.source_ip, ports_count, ps_window.num_seconds()),
            "critical",
            serde_json::json!({ "distinct_ports_hit": ports_count })
        )))
    }
    
    fn build_alert(&self, event: &Event, title: &str, description: String, severity: &str, extra_evidence: serde_json::Value) -> Alert {
        let fingerprint = format!(
            "detect:{}:{}:{}",
            event.event_type,
            event.source_ip,
            Utc::now().format("%Y-%m-%d-%H")
        );

        let mut evidence = serde_json::json!({
            "source_ip": event.source_ip,
            "actor": event.actor,
            "engine": "l1_mem"
        });

        if let Some(obj) = evidence.as_object_mut() {
            if let Some(extra) = extra_evidence.as_object() {
                for (k, v) in extra {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        Alert {
            id: Uuid::new_v4(),
            severity: severity.to_string(),
            status: "new".to_string(),
            title: title.to_string(),
            description,
            fingerprint,
            evidence,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}