-- staged_events: append-only landing zone every adapter writes into.
-- The ingestion service LISTENs on 'staged_event' and drains this table
-- with SELECT ... FOR UPDATE SKIP LOCKED, normalizing rows into `outages`.
-- Adapters never write to `outages` directly - this keeps every adapter
-- dead simple (just "insert what I saw") and keeps upsert/staleness logic
-- in exactly one place.

CREATE TABLE staged_events (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);

CREATE INDEX idx_staged_events_unprocessed
    ON staged_events (created_at)
    WHERE processed_at IS NULL;

CREATE OR REPLACE FUNCTION notify_staged_event() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('staged_event', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_notify_staged_event
    AFTER INSERT ON staged_events
    FOR EACH ROW
    EXECUTE FUNCTION notify_staged_event();
