mod config;
mod collector;

pub mod sentinel {
    tonic::include_proto!("sentinel");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _config = config::AgentConfig::from_env()
        .map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("sentinelops-agent starting (stub)");
    Ok(())
}
