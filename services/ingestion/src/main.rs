//! Ingestion service.
//!
//! Every adapter writes raw, provider-shaped events into `staged_events`
//! and nothing else - all the "is this the same outage as last time",
//! "did this provider stop reporting an outage without saying so"
//! logic lives here, in one place, instead of being reimplemented per
//! adapter.
//!
//! Two independent loops run concurrently:
//! - `drain_loop`: wakes on `LISTEN staged_event` (with a periodic
//!   fallback tick in case a NOTIFY is ever missed), and upserts staged
//!   rows into `outages`.
//! - `staleness_loop`: every `STALENESS_SWEEP_INTERVAL`, resolves any
//!   outage that hasn't been re-observed in `STALENESS_THRESHOLD` - this
//!   is how we notice a fault has ended for providers (e.g. Ellevio) whose
//!   pages just stop mentioning it rather than marking it resolved.

use chrono::Utc;
use poms_types::{OutageStatus, RawOutageEvent};
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Row};
use std::time::Duration;

const FALLBACK_POLL_INTERVAL: Duration = Duration::from_secs(10);
const STALENESS_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// An outage not re-observed within this window is considered resolved.
/// Should be comfortably larger than any adapter's poll interval (they're
/// all 60s today) to tolerate one or two missed polls before declaring an
/// outage over.
const STALENESS_THRESHOLD: Duration = Duration::from_secs(5 * 60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let pool = poms_db::connect().await?;
    poms_db::migrate(&pool).await?;

    tracing::info!("ingestion service starting");

    let drain_pool = pool.clone();
    let staleness_pool = pool.clone();

    tokio::select! {
        res = drain_loop(drain_pool) => res,
        res = staleness_loop(staleness_pool) => res,
    }
}

async fn drain_loop(pool: PgPool) -> anyhow::Result<()> {
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen("staged_event").await?;

    loop {
        // Drain everything currently pending, then wait for either a
        // notification or the fallback tick before draining again. This
        // way one NOTIFY that arrives while we're mid-drain doesn't get
        // lost - we just pick up whatever's left on the next iteration.
        if let Err(err) = drain_once(&pool).await {
            tracing::error!(error = %err, "drain failed");
        }

        tokio::select! {
            notification = listener.recv() => {
                if let Err(err) = notification {
                    tracing::error!(error = %err, "listener error, reconnecting");
                    listener = PgListener::connect_with(&pool).await?;
                    listener.listen("staged_event").await?;
                }
            }
            _ = tokio::time::sleep(FALLBACK_POLL_INTERVAL) => {}
        }
    }
}

async fn drain_once(pool: &PgPool) -> anyhow::Result<()> {
    loop {
        let mut tx = pool.begin().await?;

        let rows = sqlx::query(
            "SELECT id, payload FROM staged_events \
             WHERE processed_at IS NULL \
             ORDER BY id \
             LIMIT 100 \
             FOR UPDATE SKIP LOCKED",
        )
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            tx.commit().await?;
            return Ok(());
        }

        for row in &rows {
            let id: i64 = row.get("id");
            let payload: serde_json::Value = row.get("payload");

            match serde_json::from_value::<RawOutageEvent>(payload) {
                Ok(event) => {
                    if let Err(err) = upsert_outage(&mut tx, &event).await {
                        tracing::error!(staged_event_id = id, error = %err, "failed to upsert outage");
                    }
                }
                Err(err) => {
                    tracing::error!(staged_event_id = id, error = %err, "failed to deserialize staged event");
                }
            }

            sqlx::query("UPDATE staged_events SET processed_at = now() WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        // Fewer than a full page means we've drained the backlog for now.
        if rows.len() < 100 {
            return Ok(());
        }
    }
}

async fn upsert_outage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &RawOutageEvent,
) -> anyhow::Result<()> {
    let status = status_str(event.status);
    let resolved_at = matches!(event.status, OutageStatus::Resolved).then(Utc::now);

    sqlx::query(
        "INSERT INTO outages (
            provider, source_id, status, area_label, lat, lng, polygon,
            affected_customers, reason, started_at, estimated_end_at,
            resolved_at, first_observed_at, last_observed_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
         ON CONFLICT (provider, source_id) DO UPDATE SET
            status = EXCLUDED.status,
            area_label = EXCLUDED.area_label,
            lat = EXCLUDED.lat,
            lng = EXCLUDED.lng,
            polygon = EXCLUDED.polygon,
            affected_customers = EXCLUDED.affected_customers,
            reason = EXCLUDED.reason,
            started_at = COALESCE(outages.started_at, EXCLUDED.started_at),
            estimated_end_at = EXCLUDED.estimated_end_at,
            resolved_at = EXCLUDED.resolved_at,
            last_observed_at = EXCLUDED.last_observed_at,
            updated_at = now()",
    )
    .bind(event.provider.as_str())
    .bind(&event.source_id)
    .bind(status)
    .bind(&event.area_label)
    .bind(event.lat)
    .bind(event.lng)
    .bind(serde_json::to_value(&event.polygon)?)
    .bind(event.affected_customers)
    .bind(&event.reason)
    .bind(event.started_at)
    .bind(event.estimated_end_at)
    .bind(resolved_at)
    .bind(event.observed_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn status_str(status: OutageStatus) -> &'static str {
    match status {
        OutageStatus::Fault => "fault",
        OutageStatus::Planned => "planned",
        OutageStatus::Upcoming => "upcoming",
        OutageStatus::Resolved => "resolved",
    }
}

async fn staleness_loop(pool: PgPool) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(STALENESS_SWEEP_INTERVAL);
    loop {
        ticker.tick().await;
        match sweep_stale_outages(&pool).await {
            Ok(count) if count > 0 => {
                tracing::info!(count, "resolved stale outages");
            }
            Ok(_) => {}
            Err(err) => {
                tracing::error!(error = %err, "staleness sweep failed");
            }
        }
    }
}

async fn sweep_stale_outages(pool: &PgPool) -> anyhow::Result<u64> {
    let threshold_secs = STALENESS_THRESHOLD.as_secs() as i64;
    let result = sqlx::query(
        "UPDATE outages
         SET status = 'resolved', resolved_at = now(), updated_at = now()
         WHERE status <> 'resolved'
           AND last_observed_at < now() - make_interval(secs => $1)",
    )
    .bind(threshold_secs as f64)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
