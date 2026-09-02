use poms_adapter_piteenergi::PiteEnergiAdapter;
use poms_adapter_sdk::{run_poll_loop, PostgresEventSink};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);
    let adapter = PiteEnergiAdapter::new();

    tracing::info!("piteenergi adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
