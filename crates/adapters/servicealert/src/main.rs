use poms_adapter_sdk::{run_poll_loop, PostgresEventSink};
use poms_adapter_servicealert::ServiceAlertAdapter;
use poms_types::Provider;
use std::time::Duration;

fn env_or_die(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set"))
}

fn parse_provider(name: &str) -> Provider {
    match name {
        "karlstad" => Provider::Karlstad,
        "eskilstuna_strangnas" => Provider::EskilstunaStrangnas,
        "tranas" => Provider::Tranas,
        "uddevalla" => Provider::Uddevalla,
        other => panic!("unknown SERVICEALERT_PROVIDER: {other}"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let provider_name = env_or_die("SERVICEALERT_PROVIDER");
    let customer_id = env_or_die("SERVICEALERT_CUSTOMER_ID");

    let provider = parse_provider(&provider_name);
    let adapter_name: &'static str = Box::leak(format!("servicealert-{provider_name}").into_boxed_str());

    let pool = poms_db::connect().await?;
    let sink = PostgresEventSink::new(pool);
    let adapter = ServiceAlertAdapter::new(provider, adapter_name, &customer_id);

    tracing::info!(provider = provider_name, "servicealert adapter starting");
    run_poll_loop(adapter, sink, Duration::from_secs(60)).await;
}
