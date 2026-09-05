-- Some sources (Vattenfall, Kraftringen, Skellefteå Kraft) give the
-- affected area's outline, not just a point. Stored as JSONB - an array
-- of [lat, lng] pairs, or NULL when the source only gave a point (or
-- nothing at all). The point columns (lat/lng) stay as the primary map
-- marker location regardless; the polygon is an optional richer detail
-- shown once zoomed in.

ALTER TABLE outages ADD COLUMN polygon JSONB;
