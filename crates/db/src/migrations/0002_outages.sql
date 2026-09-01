-- outages: the normalized, long-lived record the frontend and API read
-- from. One row per (provider, source_id) - the ingestion service upserts
-- into this table as staged_events come in, and marks rows resolved when
-- a provider stops reporting them (staleness sweep).

CREATE TABLE outages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    status TEXT NOT NULL,
    area_label TEXT NOT NULL,
    lat DOUBLE PRECISION,
    lng DOUBLE PRECISION,
    affected_customers INTEGER,
    reason TEXT,
    started_at TIMESTAMPTZ,
    estimated_end_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    first_observed_at TIMESTAMPTZ NOT NULL,
    last_observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (provider, source_id)
);

CREATE INDEX idx_outages_status ON outages (status) WHERE status <> 'resolved';
CREATE INDEX idx_outages_provider ON outages (provider);
