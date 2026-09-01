# POMS2 — Power Outage Monitoring System

Aggregerar pågående och planerade strömavbrott från svenska nätägare, ett adapter-krypto per leverantör.

Detta är en ombyggnad från grunden efter att den ursprungliga koden gick förlorad. Samma grundarkitektur som tidigare:

```
crates/
  types/            delad schema (RawOutageEvent, Provider, OutageStatus)
  db/                Postgres-pool + migrationer
  adapter-sdk/       Adapter-trait + PostgresEventSink (skriver till staged_events)
  adapters/
    ellevio/          skrapar avbrottskarta.ellevio.se (kommun-nivå, inga koordinater)
services/
  ingestion/          dränerar staged_events, upsertar till outages, sopar bort inaktuella avbrott
```

**Dataflöde:** varje adapter pollar sin källa på ett fast intervall och skriver rådata till
`staged_events` (en tabell, en rad per observation). `ingestion`-tjänsten lyssnar på
Postgres `LISTEN/NOTIFY` och normaliserar det till den långlivade `outages`-tabellen -
all uppslags-/dedupliceringslogik bor där, inte i adaptrarna. En adapter som slutar
rapportera ett avbrott (utan att säga att det är löst) fångas av en periodisk
"staleness sweep" i ingestion.

## Köra lokalt

```
docker compose up --build
```

Startar Postgres, kör migrationerna, och startar Ellevio-adaptern + ingestion.

## Status

- [x] Grundarkitektur (types, db, adapter-sdk, ingestion)
- [x] Ellevio-adapter (kommun-nivå, aggregerade kundantal)
- [x] Vattenfall-adapter (`incidents.json`, riktiga koordinater via polygon-centroid)
- [x] Kraftringen-adapter (KML-feed, riktiga koordinater, saknar ortnamn - se modulkommentar)
- [x] Tekniska verken-adapter (`api.tekniskaverken.net/outage/v1/public/outages`, rikast källan hittills)
- [x] Öresundskraft-adapter (Tekla/GeoServer tile-system, koordinater joinade från separat endpoint)
- [ ] Jämtkraft (avbrottskartan var nere vid försök, 503 - försök igen senare)
- [ ] Frontend
