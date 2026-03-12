use tokio::sync::mpsc;
use crate::models::Event;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub struct AsyncIngestor {
    tx: mpsc::Sender<Event>,
}

impl AsyncIngestor {
    pub fn new(pool: PgPool) -> Self {
        let (tx, mut rx) = mpsc::channel::<Event>(10000);
        let pool = Arc::new(pool);

        tokio::spawn(async move {
            let mut batch = Vec::with_capacity(100);
            let mut timer = interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    Some(event) = rx.recv() => {
                        batch.push(event);
                        if batch.len() >= 100 {
                            Self::flush(&pool, &mut batch).await;
                        }
                    }
                    _ = timer.tick() => {
                        if !batch.is_empty() {
                            Self::flush(&pool, &mut batch).await;
                        }
                    }
                }
            }
        });

        Self { tx }
    }

    pub async fn submit(&self, event: Event) {
        if let Err(e) = self.tx.send(event).await {
            tracing::error!("Ingestion buffer full: {}", e);
        }
    }

    async fn flush(pool: &PgPool, batch: &mut Vec<Event>) {
        let mut tx = match pool.begin().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Failed to begin transaction: {}", e);
                return;
            }
        };

        for event in batch.drain(..) {
            let _ = sqlx::query(
                "INSERT INTO events (id, ts, event_type, source_ip, actor, meta) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(event.id)
            .bind(event.ts)
            .bind(event.event_type)
            .bind(event.source_ip)
            .bind(event.actor)
            .bind(event.meta)
            .execute(&mut *tx)
            .await;
        }

        if let Err(e) = tx.commit().await {
            tracing::error!("Failed to commit batch: {}", e);
        }
    }
}