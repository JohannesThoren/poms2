use poms_adapter_sdk::{run_poll_loop, PostgresEventSink};
use poms_adapter_tekla::TeklaAdapter;
use poms_types::Provider;
use std::time::Duration;

fn env_or_die(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set"))
}

fn env_f64_or_die(key: &str) -> f64 {
    env_or_die(key).parse().unwrap_or_else(|_| panic!("{key} must be a valid float"))
}

fn parse_provider(name: &str) -> Provider {
    match name {
        "oresundskraft" => Provider::Oresundskraft,
        "gavle" => Provider::Gavle,
        "harjeans" => Provider::Harjeans,
        other => panic!("unknown TEKLA_PROVIDER: {other}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider_name = env_or_die("TEKLA_PROVIDER");
    let base_url = env_or_die("TEKLA_BASE_URL");
    let lat_min = env_f64_or_die("TEKLA_LAT_MIN");
    let lat_max = env_f64_or_die("TEKLA_LAT_MAX");
    let lng_min = env_f64_or_die("TEKLA_LNG_MIN");
    let lng_max = env_f64_or_die("TEKLA_LNG_MAX");

    let provider = parse_provider(&provider_name);
    let adapter_name: &'static str = Box::leak(format!("tekla-{provider_name}").into_boxed_str());

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);
    let adapter = TeklaAdapter::new(provider, adapter_name, &base_url, lat_min, lat_max, lng_min, lng_max);

    tracing::info!(provider = provider_name, base_url, "tekla adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
