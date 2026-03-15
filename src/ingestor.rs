use tokio::sync::mpsc;
use crate::ml_features::EventFeatureVector;
use crate::models::Event;
use sqlx::PgPool;
use tokio::time::{interval, Duration};

pub struct AsyncIngestor {
    tx: mpsc::Sender<Event>,
}

impl AsyncIngestor {
    pub fn new(pool: PgPool) -> Self {
        let (tx, mut rx) = mpsc::channel::<Event>(10_000);

        tokio::spawn(async move {
            let mut batch: Vec<Event> = Vec::with_capacity(100);
            let mut timer = interval(Duration::from_secs(3));

            loop {
                tokio::select! {
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(event) => {
                                batch.push(event);
                                if batch.len() >= 100 {
                                    Self::flush(&pool, &mut batch).await;
                                }
                            }
                            // Channel closed: all senders dropped (server shutting down).
                            // Flush whatever is in the buffer before exiting.
                            None => {
                                if !batch.is_empty() {
                                    tracing::info!(count = batch.len(), "Graceful shutdown: flushing remaining events");
                                    Self::flush(&pool, &mut batch).await;
                                }
                                break;
                            }
                        }
                    }
                    _ = timer.tick() => {
                        if !batch.is_empty() {
                            Self::flush(&pool, &mut batch).await;
                        }
                    }
                }
            }

            tracing::info!("Ingestor worker shut down cleanly");
        });

        Self { tx }
    }

    pub async fn submit(&self, event: Event) {
        if let Err(e) = self.tx.send(event).await {
            tracing::error!(error = %e, "Failed to enqueue event: ingestion channel closed");
        }
    }

    async fn flush(pool: &PgPool, batch: &mut Vec<Event>) {
        let mut tx = match pool.begin().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "Failed to begin transaction; dropping batch of {} events", batch.len());
                batch.clear();
                return;
            }
        };

        for event in batch.drain(..) {
            // Build feature vector before event fields are consumed by bind().
            let fv = EventFeatureVector::from_event(&event);

            if let Err(e) = sqlx::query(
                "INSERT INTO events (id, ts, event_type, source_ip, actor, meta)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(event.id)
            .bind(event.ts)
            .bind(event.event_type)
            .bind(event.source_ip)
            .bind(event.actor)
            .bind(event.meta)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(error = %e, event_id = %fv.event_id, "Failed to insert event");
            }

            if let Err(e) = sqlx::query(
                "INSERT INTO ml_features_log \
                 (event_id, hour_of_day, is_weekend, event_type_id, has_actor, payload_size_bytes) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(fv.event_id)
            .bind(fv.hour_of_day as i16)
            .bind(fv.is_weekend)
            .bind(fv.event_type_id)
            .bind(fv.has_actor)
            .bind(fv.payload_size_bytes)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(error = %e, event_id = %fv.event_id, "Failed to insert ml feature vector");
            }
        }

        if let Err(e) = tx.commit().await {
            tracing::error!(error = %e, "Failed to commit batch");
        }
    }
}