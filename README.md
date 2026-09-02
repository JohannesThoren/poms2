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
- [x] Digpro "Outage Map"-familjen: en generisk adapter (env-var-konfigurerad) täcker Växjö Energi, Lerum Energi, Västerbergslagens Elnät, Partille Energi, Linde Energi - samma bakomliggande system som Kraftringen, se `crates/adapters/digpro/src/lib.rs` för hur det hittades
- [x] Tekla/GeoServer-familjen: en generisk adapter (env-var-konfigurerad) täcker Gävle Energi utöver Öresundskraft - samma bakomliggande system, se `crates/adapters/tekla/src/lib.rs`. Sundsvall Elnät bekräftat kompatibel men inte tillagd som tjänst än.
- [x] Frontend (Next.js, `/frontend`) - dashboard som läser direkt från `outages`-tabellen, alltid färsk rendering (`dynamic = "force-dynamic"`), interaktivt statusfilter
- [ ] Jämtkraft (avbrottskartan var nere vid försök, 503 - försök igen senare)
- [ ] Fler Digpro-kandidater: Falu Elnät (500-fel just nu), Härnösand Elnät (503), Hedemora Energi (503), Bergs Tingslags Elektriska (ingen anslutning)
- [x] Skellefteå Kraft-adapter (`driftinfo3-api.skekraft.se/api/disturbances`, eget system, hittat via app.config.js)
- [x] ServiceAlert-familjen (`se.sms-service.dk`): en generisk adapter täcker Karlstads El, Eskilstuna Strängnäs Energi, Tranås Energi, Uddevalla Energi. **Osäker statuslogik** - se varningen i `crates/adapters/servicealert/src/lib.rs` (ingen riktig statuskod i källan, bara ett visningsfönster + fritext; inte verifierad mot ett riktigt elavbrott än)
- [x] Telge Nät via Digpro (annan servlet-sökväg, saknar ".api." - adaptern stödjer nu båda varianterna via `DIGPRO_KML_URL`)
- [x] Mälarenergi-adapter (egen Next.js-API, `malarenergi.se/api/outages`) - de kör även Tekla/GeoServer OCH ServiceAlert parallellt, vi valde den renaste av de tre
- [x] Upplands Energi-adapter (`avbrott.upplandsenergi.se`, eget system). **Overifierat tidsformat** - se varningen i `crates/adapters/upplandsenergi/src/lib.rs` (interruptions.json var tomt hela tiden, start/end/est-fälten är gissade)
- [x] Västra Orusts Energitjänst-adapter (`voe.se/mirakel/news.json`, eget "Mirakel"-system) - riktiga UTC-tider, ingen DST-gissning, men saknar kundantal helt
- [ ] Skövde Energi: hittade en tredje integrationsstil (WordPress-plugin som proxar samma bakomliggande tjänst), men svaret är tomt just nu så fältnamnen är overifierade - bygg när skarp data finns
- [ ] Karlskoga Elnät: ingen API hittad vid snabb koll, kan behöva mer interaktion (scroll/klick) för att trigga kartladdning
- [ ] Kalmar Energi använder bara en manuellt uppdaterad Google My Maps-karta, ingen realtids-API - troligen inte värt att bygga adapter för
- [ ] E.ON blockerat av Cloudflare-botskydd - inget vi försöker kringgå
- [ ] Frontend
