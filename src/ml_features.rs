use chrono::{Datelike, Timelike, Weekday};
use uuid::Uuid;
use crate::models::Event;

pub struct EventFeatureVector {
    pub event_id: Uuid,
    pub hour_of_day: u8,
    pub is_weekend: bool,
    pub event_type_id: i32,
    pub has_actor: bool,
    pub payload_size_bytes: i32,
}

impl EventFeatureVector {
    pub fn from_event(event: &Event) -> Self {
        let hour_of_day = event.ts.hour().min(23) as u8;
        let is_weekend = matches!(event.ts.weekday(), Weekday::Sat | Weekday::Sun);
        let event_type_id = djb2_hash(&event.event_type);
        let has_actor = event.actor.is_some();
        let payload_size_bytes = serde_json::to_vec(&event.meta)
            .map(|v| v.len().min(i32::MAX as usize) as i32)
            .unwrap_or(0);

        Self {
            event_id: event.id,
            hour_of_day,
            is_weekend,
            event_type_id,
            has_actor,
            payload_size_bytes,
        }
    }
}

/// Deterministic djb2 hash: stable across runs, no external deps.
/// Produces a consistent i32 ID for categorical event_type strings.
fn djb2_hash(s: &str) -> i32 {
    let mut hash: u32 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash as i32
}
