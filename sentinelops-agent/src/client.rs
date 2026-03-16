use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use anyhow::Context;

use crate::config::AgentConfig;
use crate::sentinel::{
    ingestion_service_client::IngestionServiceClient,
    SecurityEvent,
};

pub struct StreamingSession {
    client: IngestionServiceClient<Channel>,
}

impl StreamingSession {
    /// Connect to the Control Plane with mTLS.
    /// Reads cert files asynchronously — never blocks the executor.
    pub async fn connect(config: &AgentConfig) -> anyhow::Result<Self> {
        let ca_pem   = tokio::fs::read_to_string(&config.ca_cert_path)
            .await
            .with_context(|| format!("Failed to read CA cert: {}", config.ca_cert_path))?;
        let cert_pem = tokio::fs::read_to_string(&config.client_cert_path)
            .await
            .with_context(|| format!("Failed to read client cert: {}", config.client_cert_path))?;
        let key_pem  = tokio::fs::read_to_string(&config.client_key_path)
            .await
            .with_context(|| format!("Failed to read client key: {}", config.client_key_path))?;

        let tls = ClientTlsConfig::new()
            .domain_name("sentinelops-control.internal")
            .ca_certificate(Certificate::from_pem(&ca_pem))
            .identity(Identity::from_pem(&cert_pem, &key_pem));

        let channel = Channel::from_shared(config.control_plane_addr.clone())
            .context("Invalid control plane address")?
            .tls_config(tls)
            .context("TLS configuration failed")?
            .connect()
            .await
            .context("Failed to connect to Control Plane")?;

        Ok(Self {
            client: IngestionServiceClient::new(channel),
        })
    }

    /// Open a client-side streaming RPC.
    /// Returns a Sender — push SecurityEvent values to it.
    /// Dropping the Sender closes the stream; the server responds with StreamSummary.
    pub async fn open_stream(&mut self) -> anyhow::Result<mpsc::Sender<SecurityEvent>> {
        let (tx, rx) = mpsc::channel::<SecurityEvent>(1024);
        let stream   = ReceiverStream::new(rx);
        let mut client = self.client.clone();

        tokio::spawn(async move {
            match client.stream_events(stream).await {
                Ok(resp) => {
                    let s = resp.into_inner();
                    tracing::info!(
                        accepted = s.accepted_count,
                        rejected = s.rejected_count,
                        session  = %s.session_id,
                        "Stream session closed"
                    );
                }
                Err(e) => tracing::error!(error = %e, "gRPC stream error"),
            }
        });

        Ok(tx)
    }
}

/// Main ingest loop: connects to the Control Plane, opens a stream, and
/// drives events from the Collector into the stream in batches.
/// Reconnects automatically with exponential backoff on any failure.
pub async fn run_ingest_loop(
    config: Arc<AgentConfig>,
    mut event_rx: mpsc::Receiver<SecurityEvent>,
) {
    let mut backoff = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(60);

    loop {
        tracing::info!(addr = %config.control_plane_addr, "Connecting to Control Plane...");

        let mut session = match StreamingSession::connect(&config).await {
            Ok(s) => {
                backoff = Duration::from_secs(1); // reset on successful connect
                s
            }
            Err(e) => {
                tracing::error!(error = %e, backoff_secs = backoff.as_secs(), "Connection failed, retrying...");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let stream_tx = match session.open_stream().await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!(error = %e, "Failed to open stream");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let mut flush_tick = interval(Duration::from_millis(config.flush_interval_ms));
        let mut batch: Vec<SecurityEvent> = Vec::with_capacity(config.batch_size);

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            batch.push(event);
                            if batch.len() >= config.batch_size {
                                flush_batch(&stream_tx, &mut batch).await;
                            }
                        }
                        None => {
                            // Collector channel closed — agent is shutting down
                            if !batch.is_empty() {
                                flush_batch(&stream_tx, &mut batch).await;
                            }
                            tracing::info!("Ingest loop: collector channel closed, exiting");
                            return;
                        }
                    }
                }

                _ = flush_tick.tick() => {
                    if !batch.is_empty() {
                        flush_batch(&stream_tx, &mut batch).await;
                    }
                }
            }

            if stream_tx.is_closed() {
                tracing::warn!("Stream closed by server, reconnecting...");
                break; // break inner loop → reconnect in outer loop
            }
        }
    }
}

async fn flush_batch(tx: &mpsc::Sender<SecurityEvent>, batch: &mut Vec<SecurityEvent>) {
    for event in batch.drain(..) {
        if let Err(e) = tx.send(event).await {
            tracing::error!(error = %e, "Stream tx closed during flush");
            return;
        }
    }
}
