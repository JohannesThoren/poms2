import { Pool } from "pg";
import { geocodeAreaLabel } from "./geocode";

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
  /** True when `lat`/`lng` came from geocoding `area_label` after the
   * fact (see `lib/geocode.ts`), not from the source itself - the map
   * should render these less precisely than a source-provided point. */
  approx: boolean;
  affected_customers: number | null;
  reason: string | null;
  started_at: string | null;
  estimated_end_at: string | null;
  resolved_at: string | null;
  last_observed_at: string;
};

/** Fills in a best-effort lat/lng for any row missing one, from its
 * area_label - see `lib/geocode.ts`. Rows that already have real
 * coordinates are left untouched. */
function withGeocodedFallback<T extends { lat: number | null; lng: number | null; area_label: string }>(
  rows: T[]
): (T & { approx: boolean })[] {
  return rows.map((row) => {
    if (row.lat != null && row.lng != null) {
      return { ...row, approx: false };
    }
    const geocoded = geocodeAreaLabel(row.area_label);
    if (!geocoded) {
      return { ...row, approx: false };
    }
    return { ...row, lat: geocoded.lat, lng: geocoded.lng, approx: true };
  });
}

export async function getActiveOutages(): Promise<Outage[]> {
  const { rows } = await pool.query<Omit<Outage, "approx">>(
    `SELECT id, provider, source_id, status, area_label, lat, lng,
            affected_customers, reason, started_at, estimated_end_at,
            resolved_at, last_observed_at
     FROM outages
     WHERE status <> 'resolved'
     ORDER BY affected_customers DESC NULLS LAST`
  );
  return withGeocodedFallback(rows);
}

export async function getRecentlyResolved(limit = 15): Promise<Outage[]> {
  const { rows } = await pool.query<Omit<Outage, "approx">>(
    `SELECT id, provider, source_id, status, area_label, lat, lng,
            affected_customers, reason, started_at, estimated_end_at,
            resolved_at, last_observed_at
     FROM outages
     WHERE status = 'resolved'
     ORDER BY resolved_at DESC NULLS LAST
     LIMIT $1`,
    [limit]
  );
  return withGeocodedFallback(rows);
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
  last_poll_at: string | null;
  last_success_at: string | null;
  last_error: string | null;
  active_count: number;
  resolved_24h_count: number;
  total_events_seen: number;
};

/**
 * Per-provider health, keyed off `adapter_heartbeat` (written on every
 * poll tick regardless of outcome) rather than `outages` - the outages
 * table only ever gets rows when there's something to report, so an
 * adapter that's polling fine but genuinely has nothing to show would
 * otherwise look identical to one that's never run at all.
 * `active_count`/`resolved_24h_count`/`total_events_seen` still come from
 * `outages` since that's the only place those exist.
 */
export async function getProviderStatus(): Promise<ProviderStatus[]> {
  const { rows } = await pool.query(
    `SELECT h.provider,
            h.last_poll_at,
            h.last_success_at,
            h.last_error,
            COALESCE(o.active_count, 0)::int AS active_count,
            COALESCE(o.resolved_24h_count, 0)::int AS resolved_24h_count,
            COALESCE(o.total_events_seen, 0)::int AS total_events_seen
     FROM adapter_heartbeat h
     FULL OUTER JOIN (
        SELECT provider,
               count(*) FILTER (WHERE status <> 'resolved') AS active_count,
               count(*) FILTER (WHERE status = 'resolved' AND resolved_at > now() - interval '24 hours') AS resolved_24h_count,
               count(*) AS total_events_seen
        FROM outages
        GROUP BY provider
     ) o ON o.provider = h.provider
     ORDER BY COALESCE(h.provider, o.provider)`
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
