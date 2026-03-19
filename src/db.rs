use sqlx::{postgres::PgPoolOptions, PgPool};
use crate::models::Alert;
use crate::errors::SentinelResult;

pub async fn create_pool(
    database_url: &str,
    max_connections: u32,
    min_connections: u32,
) -> SentinelResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn save_alert(pool: &PgPool, alert: &Alert) -> SentinelResult<()> {
    sqlx::query(
        "INSERT INTO alerts (id, severity, status, title, description, fingerprint, evidence, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(alert.id)
    .bind(&alert.severity)
    .bind(&alert.status)
    .bind(&alert.title)
    .bind(&alert.description)
    .bind(&alert.fingerprint)
    .bind(&alert.evidence)
    .bind(alert.created_at)
    .bind(alert.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}
