//! Entry point for the generic Digpro adapter. Which company this
//! particular container instance polls is chosen entirely by env vars, so
//! the same image is deployed once per company in docker-compose with
//! different configuration - see the module docs in `lib.rs` for how this
//! endpoint was found and which companies are confirmed working.

use poms_adapter_digpro::DigproAdapter;
use poms_adapter_sdk::{run_poll_loop, PostgresEventSink};
use poms_types::Provider;
use std::time::Duration;

fn env_or_die(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set"))
}

fn parse_provider(name: &str) -> Provider {
    match name {
        "vaxjo" => Provider::Vaxjo,
        "lerum" => Provider::Lerum,
        "vasterbergslagens" => Provider::Vasterbergslagens,
        "partille" => Provider::Partille,
        other => panic!("unknown DIGPRO_PROVIDER: {other}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider_name = env_or_die("DIGPRO_PROVIDER");
    let base_url = env_or_die("DIGPRO_BASE_URL");
    let cust = env_or_die("DIGPRO_CUST");
    let app = std::env::var("DIGPRO_APP").unwrap_or_else(|_| "fpp".to_string());

    let provider = parse_provider(&provider_name);
    // Leaked deliberately: Adapter::name() needs a &'static str, and this
    // runs once at startup for the life of the process - not a real leak
    // in practice.
    let adapter_name: &'static str = Box::leak(format!("digpro-{provider_name}").into_boxed_str());

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);
    let adapter = DigproAdapter::new(provider, adapter_name, &base_url, &cust, &app);

    tracing::info!(provider = provider_name, base_url, cust, "digpro adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
