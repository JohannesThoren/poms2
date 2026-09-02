# POMS2 frontend

A dashboard reading directly from the shared Postgres `outages` table -
no separate API layer. Server components query Postgres via `lib/db.ts`
and render on every request (`export const dynamic = "force-dynamic"` in
`app/page.tsx`) so it always reflects the current state, no caching lag.

## Running locally

```
DATABASE_URL=postgres://poms:poms@localhost:5432/poms npm run dev
```

Needs the same Postgres the adapters/ingestion write to (see the root
README / docker-compose.yml).
