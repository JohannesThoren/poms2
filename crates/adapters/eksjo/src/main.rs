use poms_adapter_eksjo::EksjoAdapter;
use poms_adapter_sdk::{run_poll_loop, PostgresEventSink};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);
    let adapter = EksjoAdapter::new();

    tracing::info!("eksjo adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
