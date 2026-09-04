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
        "linde" => Provider::Linde,
        "telge" => Provider::Telge,
        "uddevalla" => Provider::Uddevalla,
        other => panic!("unknown DIGPRO_PROVIDER: {other}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider_name = env_or_die("DIGPRO_PROVIDER");
    let provider = parse_provider(&provider_name);
    let adapter_name: &'static str = Box::leak(provider_name.clone().into_boxed_str());

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);

    // Two ways to configure a deployment: the usual base_url + cust (+
    // optional app) that reconstructs the standard path, or a full
    // DIGPRO_KML_URL for deployments (e.g. Telge Nät) that use a
    // differently-shaped servlet path.
    let adapter = if let Ok(kml_url) = std::env::var("DIGPRO_KML_URL") {
        DigproAdapter::from_url(provider, adapter_name, kml_url)
    } else {
        let base_url = env_or_die("DIGPRO_BASE_URL");
        let cust = env_or_die("DIGPRO_CUST");
        let app = std::env::var("DIGPRO_APP").unwrap_or_else(|_| "fpp".to_string());
        DigproAdapter::new(provider, adapter_name, &base_url, &cust, &app)
    };

    tracing::info!(provider = provider_name, "digpro adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
