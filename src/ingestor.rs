use tokio::sync::mpsc;
use crate::config::Config;
use crate::ml_features::EventFeatureVector;
use crate::models::Event;
use sqlx::PgPool;
use tokio::time::{interval, Duration};

pub struct AsyncIngestor {
    tx: mpsc::Sender<Event>,
}

impl AsyncIngestor {
    pub fn new(pool: PgPool, config: &Config) -> Self {
        let (tx, mut rx) = mpsc::channel::<Event>(config.ingestor_queue_capacity);
        let batch_size = config.ingestor_batch_size;
        let flush_ms = config.ingestor_flush_ms;

        tokio::spawn(async move {
            let mut batch: Vec<Event> = Vec::with_capacity(batch_size);
            let mut timer = interval(Duration::from_millis(flush_ms));

            loop {
                tokio::select! {
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(event) => {
                                batch.push(event);
                                if batch.len() >= batch_size {
                                    Self::flush(&pool, &mut batch).await;
                                }
                            }
                            // Channel closed: todos os senders caíram (shutdown).
                            // Flush o buffer restante antes de sair.
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
        let batch_len = batch.len();

        let mut tx = match pool.begin().await {
            Ok(t) => t,
            Err(e) => {
                // Não temos como persistir estes eventos — log e descarta.
                // Em produção, considere uma dead-letter queue.
                tracing::error!(
                    error = %e,
                    dropped = batch_len,
                    "Failed to begin transaction; dropping batch"
                );
                batch.clear();
                return;
            }
        };

        // Pre-computa feature vectors antes de consumir os eventos com drain.
        let events: Vec<Event> = batch.drain(..).collect();

        for event in &events {
            let fv = EventFeatureVector::from_event(event);

            let event_ok = sqlx::query(
                "INSERT INTO events (id, ts, event_type, source_ip, actor, meta)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(event.id)
            .bind(event.ts)
            .bind(&event.event_type)
            .bind(&event.source_ip)
            .bind(&event.actor)
            .bind(&event.meta)
            .execute(&mut *tx)
            .await;

            if let Err(e) = event_ok {
                tracing::error!(
                    error = %e,
                    event_id = %event.id,
                    batch_size = batch_len,
                    "INSERT failed — rolling back entire batch"
                );
                // Rollback explícito para não commitar dados parciais.
                if let Err(rb_err) = tx.rollback().await {
                    tracing::error!(error = %rb_err, "Rollback also failed");
                }
                return;
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
                tracing::error!(
                    error = %e,
                    event_id = %fv.event_id,
                    "ML feature INSERT failed — rolling back entire batch"
                );
                if let Err(rb_err) = tx.rollback().await {
                    tracing::error!(error = %rb_err, "Rollback also failed");
                }
                return;
            }
        }

        if let Err(e) = tx.commit().await {
            tracing::error!(error = %e, batch_size = batch_len, "Failed to commit batch — data lost");
        } else {
            tracing::debug!(count = batch_len, "Batch committed successfully");
        }
    }
}
