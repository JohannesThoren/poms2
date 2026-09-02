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
