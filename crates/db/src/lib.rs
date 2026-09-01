use anyhow::Context;
use sqlx::postgres::{PgPool, PgPoolOptions};

/// Build a Postgres connection pool from `DATABASE_URL`.
///
/// Every service (adapters, ingestion, migrate job) shares this so pool
/// sizing and connect-timeout behavior stay consistent across the system.
pub async fn connect() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set (e.g. postgres://poms:poms@postgres:5432/poms)")?;

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .context("failed to connect to Postgres")
}

/// Run pending migrations. Safe to call from every service on startup, but
/// intended to be run once by the dedicated `migrate` one-shot container in
/// docker-compose before adapters/ingestion start.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./src/migrations")
        .run(pool)
        .await
        .context("failed to run migrations")?;
    Ok(())
}
