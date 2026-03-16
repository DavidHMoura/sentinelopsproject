mod config;
mod collector;
mod client;

use std::sync::Arc;
use tokio::sync::mpsc;
use config::AgentMode;

// Proto types generated from proto/sentinel.proto at build time
pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cfg = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;

    tracing::info!(
        agent_id   = %cfg.agent_id,
        mode       = ?cfg.mode,
        host       = %cfg.source_host,
        "SentinelOps Agent starting"
    );

    match cfg.mode {
        AgentMode::Autonomous => {
            // Autonomous mode: sentinelops-agent does not run.
            // Deploy sentinelops-rust standalone instead.
            tracing::warn!("AGENT_MODE=autonomous — this binary should not be deployed in autonomous mode. Run sentinelops-rust directly.");
            return Ok(());
        }

        AgentMode::Subordinate | AgentMode::Hybrid => {
            let cfg = Arc::new(cfg);
            let (tx, rx) = mpsc::channel::<sentinel::SecurityEvent>(cfg.queue_capacity);

            // Spawn the Collector — produces events into the channel
            let collector_cfg = cfg.clone();
            tokio::spawn(async move {
                collector::run_collector(collector_cfg, tx).await;
            });

            // Run the ingest loop — consumes events and dispatches to Control Plane
            // This blocks until the channel closes (i.e., agent shuts down)
            client::run_ingest_loop(cfg, rx).await;
        }
    }

    Ok(())
}
