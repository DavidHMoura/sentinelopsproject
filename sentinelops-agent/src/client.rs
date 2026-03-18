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
    stream_task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for StreamingSession {
    fn drop(&mut self) {
        if let Some(handle) = self.stream_task.take() {
            handle.abort();
        }
    }
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
            .domain_name(&config.tls_server_name)
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
            stream_task: None,
        })
    }

    /// Open a client-side streaming RPC.
    /// Returns a Sender — push SecurityEvent values to it.
    /// Dropping the Sender closes the stream; the server responds with StreamSummary.
    /// The spawned gRPC task handle is stored in `self.stream_task` and will be
    /// aborted automatically when `StreamingSession` is dropped.
    pub async fn open_stream(&mut self) -> anyhow::Result<mpsc::Sender<SecurityEvent>> {
        let (tx, rx) = mpsc::channel::<SecurityEvent>(1024);
        let stream   = ReceiverStream::new(rx);
        let mut client = self.client.clone();

        let handle = tokio::spawn(async move {
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

        self.stream_task = Some(handle);
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
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, backoff_secs = backoff.as_secs(), "Connection failed, retrying...");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let stream_tx = match session.open_stream().await {
            Ok(tx) => {
                backoff = Duration::from_secs(1); // reset only after both connect + stream are up
                tx
            }
            Err(e) => {
                tracing::error!(error = %e, backoff_secs = backoff.as_secs(), "Failed to open stream, retrying...");
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
                // Critical 2: log events that will be dropped before breaking
                if !batch.is_empty() {
                    tracing::warn!(dropped = batch.len(), "Dropping buffered events — stream closed by server");
                    batch.clear();
                }
                // Critical 1: apply backoff before reconnecting (avoid busy-loop)
                tracing::warn!(backoff_secs = backoff.as_secs(), "Stream closed by server, reconnecting after backoff...");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                break; // break inner loop → reconnect in outer loop
            }
        }
    }
}

async fn flush_batch(tx: &mpsc::Sender<SecurityEvent>, batch: &mut Vec<SecurityEvent>) {
    let total = batch.len();
    let mut sent = 0usize;
    for event in batch.drain(..) {
        if let Err(e) = tx.send(event).await {
            tracing::error!(error = %e, dropped = total - sent, "Stream tx closed during flush");
            return;
        }
        sent += 1;
    }
}
