import { Pool } from "pg";

declare global {
  // eslint-disable-next-line no-var
  var pgPool: Pool | undefined;
}

// Reuse the pool across hot reloads / server component invocations instead
// of opening a fresh connection per request.
export const pool =
  global.pgPool ??
  new Pool({
    connectionString: process.env.DATABASE_URL,
    max: 5,
  });

if (process.env.NODE_ENV !== "production") {
  global.pgPool = pool;
}

export type Outage = {
  id: string;
  provider: string;
  source_id: string;
  status: "fault" | "planned" | "upcoming" | "resolved";
  area_label: string;
  lat: number | null;
  lng: number | null;
  affected_customers: number | null;
  reason: string | null;
  started_at: string | null;
  estimated_end_at: string | null;
  resolved_at: string | null;
  last_observed_at: string;
};

export async function getActiveOutages(): Promise<Outage[]> {
  const { rows } = await pool.query<Outage>(
    `SELECT id, provider, source_id, status, area_label, lat, lng,
            affected_customers, reason, started_at, estimated_end_at,
            resolved_at, last_observed_at
     FROM outages
     WHERE status <> 'resolved'
     ORDER BY affected_customers DESC NULLS LAST`
  );
  return rows;
}

export async function getRecentlyResolved(limit = 15): Promise<Outage[]> {
  const { rows } = await pool.query<Outage>(
    `SELECT id, provider, source_id, status, area_label, lat, lng,
            affected_customers, reason, started_at, estimated_end_at,
            resolved_at, last_observed_at
     FROM outages
     WHERE status = 'resolved'
     ORDER BY resolved_at DESC NULLS LAST
     LIMIT $1`,
    [limit]
  );
  return rows;
}

export async function getProviderSummary(): Promise<
  { provider: string; active_count: number; total_customers: number }[]
> {
  const { rows } = await pool.query(
    `SELECT provider,
            count(*) FILTER (WHERE status <> 'resolved')::int AS active_count,
            COALESCE(sum(affected_customers) FILTER (WHERE status <> 'resolved'), 0)::int AS total_customers
     FROM outages
     GROUP BY provider
     ORDER BY provider`
  );
  return rows;
}

export type ProviderStatus = {
  provider: string;
  last_observed_at: string | null;
  active_count: number;
  resolved_24h_count: number;
  total_events_seen: number;
};

/**
 * Per-provider health: when each adapter last actually wrote something we
 * ingested, how many outages are currently active for it, and how many it
 * resolved in the last 24h. `last_observed_at` is the closest thing we
 * have to "last successful poll" - an adapter that's silently failing
 * (network error, site changed shape) will fall behind here even though
 * its container is still running.
 */
export async function getProviderStatus(): Promise<ProviderStatus[]> {
  const { rows } = await pool.query(
    `SELECT provider,
            max(last_observed_at) AS last_observed_at,
            count(*) FILTER (WHERE status <> 'resolved')::int AS active_count,
            count(*) FILTER (WHERE status = 'resolved' AND resolved_at > now() - interval '24 hours')::int AS resolved_24h_count,
            count(*)::int AS total_events_seen
     FROM outages
     GROUP BY provider
     ORDER BY provider`
  );
  return rows;
}

export type StagedEventLogRow = {
  id: string;
  provider: string;
  created_at: string;
  processed_at: string | null;
};

/**
 * Recent staged_events rows as a simple activity log - every adapter poll
 * that produced at least one event shows up here, with `processed_at`
 * showing whether ingestion has caught up yet. This isn't a full
 * application log (adapters only write to stdout otherwise), just the
 * durable trace of polling activity that's actually in the database.
 */
export async function getRecentActivity(limit = 200): Promise<StagedEventLogRow[]> {
  const { rows } = await pool.query(
    `SELECT id, provider, created_at, processed_at
     FROM staged_events
     ORDER BY id DESC
     LIMIT $1`,
    [limit]
  );
  return rows;
}
