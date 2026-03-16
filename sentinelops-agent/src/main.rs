mod config;
mod collector;
mod client;

pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("sentinelops-agent starting (stub — Task 8 will wire everything)");
    Ok(())
}
