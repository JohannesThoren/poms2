//! Shared scaffolding for provider adapters.
//!
//! An adapter's job is deliberately narrow: poll one grid operator's
//! source, normalize whatever it returns into `RawOutageEvent`s, and hand
//! them to a `PostgresEventSink`. All the upsert/staleness/dedup logic
//! lives in the ingestion service, not here - so a new adapter is just
//! "fetch + parse + call write_batch".

use anyhow::Context;
use poms_types::RawOutageEvent;
use sqlx::PgPool;

/// Writes normalized events into `staged_events`, transactionally.
///
/// One `write_batch` call = one poll cycle for one provider. Writing the
/// whole batch in a single transaction means a partial scrape failure never
/// leaves half a poll cycle staged.
pub struct PostgresEventSink {
    pool: PgPool,
}

impl PostgresEventSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn write_batch(&self, events: &[RawOutageEvent]) -> anyhow::Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to start transaction")?;

        for event in events {
            let provider = event.provider.as_str();
            let payload = serde_json::to_value(event).context("failed to serialize event")?;

            sqlx::query(
                "INSERT INTO staged_events (provider, payload) VALUES ($1, $2)",
            )
            .bind(provider)
            .bind(payload)
            .execute(&mut *tx)
            .await
            .context("failed to insert staged event")?;
        }

        tx.commit().await.context("failed to commit transaction")?;

        tracing::info!(count = events.len(), "staged events written");
        Ok(())
    }
}

/// Implemented by every provider crate (e.g. `poms-adapter-ellevio`).
///
/// `poll` is expected to fetch the provider's current outage list fresh
/// each call - the sink and ingestion service handle diffing against what
/// was seen before, so adapters stay stateless.
#[async_trait::async_trait]
pub trait Adapter {
    fn name(&self) -> &'static str;
    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>>;
}

/// Runs `adapter.poll()` on a fixed interval, writing each batch to `sink`.
/// Logs and continues on error rather than crashing the process - a single
/// bad poll (e.g. the source returned malformed HTML) shouldn't take the
/// whole adapter container down.
pub async fn run_poll_loop(
    adapter: impl Adapter,
    sink: PostgresEventSink,
    interval: std::time::Duration,
) -> ! {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        match adapter.poll().await {
            Ok(events) => {
                if let Err(err) = sink.write_batch(&events).await {
                    tracing::error!(adapter = adapter.name(), error = %err, "failed to write batch");
                }
            }
            Err(err) => {
                tracing::error!(adapter = adapter.name(), error = %err, "poll failed");
            }
        }
    }
}
