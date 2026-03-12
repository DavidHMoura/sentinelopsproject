use actix_web::{get, post, web, HttpResponse};
use crate::models::{EventIn, Event};
use crate::detection::DetectionEngine;
use crate::ingestor::AsyncIngestor;
use crate::config::Config;
use crate::errors::{SentinelResult, SentinelError};
use validator::Validate;
use sqlx::PgPool;

#[post("/events/ingest")]
pub async fn ingest_handler(
    payload: web::Json<EventIn>,
    engine: web::Data<DetectionEngine>,
    ingestor: web::Data<AsyncIngestor>,
    config: web::Data<Config>,
    pool: web::Data<PgPool>,
) -> SentinelResult<HttpResponse> {
    payload.validate().map_err(|e| {
        SentinelError::ValidationError(format!("Invalid data: {}", e))
    })?;

    let event = Event::from_input(payload.into_inner());

    if let Some(alert) = engine.evaluate(&config, &event).await? {
        crate::db::save_alert(&pool, &alert).await?;
        tracing::warn!(alert_id = %alert.id, "Security anomaly detected");
    }

    ingestor.submit(event).await;

    Ok(HttpResponse::Accepted().json(serde_json::json!({ "status": "accepted" })))
}

#[get("/events")]
pub async fn list_events(
    pool: web::Data<PgPool>,
) -> SentinelResult<HttpResponse> {
    let events = sqlx::query_as::<_, Event>("SELECT * FROM events ORDER BY ts DESC LIMIT 50")
        .fetch_all(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(events))
}