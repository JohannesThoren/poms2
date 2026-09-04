-- Every adapter poll writes a heartbeat row here regardless of whether it
-- found any outages - this is what actually answers "is this adapter
-- alive", since `outages` only ever gets rows when there's something to
-- report. Without this, an adapter that's working perfectly but simply
-- has nothing to report right now (or one that has genuinely never had a
-- reportable outage, like a newly added source) is indistinguishable from
-- one that's been silently broken since the container started.

CREATE TABLE adapter_heartbeat (
    provider TEXT PRIMARY KEY,
    last_poll_at TIMESTAMPTZ NOT NULL,
    last_success_at TIMESTAMPTZ,
    last_error TEXT
);
