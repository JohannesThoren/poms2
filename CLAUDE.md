# CLAUDE.md

Kontext för Claude (eller andra assistenter) som jobbar i det här repot. Läs detta innan du börjar, och läs `README.md` för aktuell status över vilka leverantörer som är klara.

## Vad det här är

POMS2 (Power Outage Monitoring System) aggregerar pågående och planerade strömavbrott från svenska nätägare, en adapter per leverantör. Ägare: Johannes Thorén. Privat/proprietärt projekt — se `LICENSE` innan du delar kod, data eller URL-mönster utanför det här repot.

## Arkitektur

```
crates/
  types/            delad schema (RawOutageEvent, Provider, OutageStatus) — poms-types
  db/                Postgres-pool + migrationer (crates/db/src/migrations/)
  adapter-sdk/       Adapter-trait + PostgresEventSink + run_poll_loop — poms-adapter-sdk
  adapters/<namn>/   en crate per leverantör (eller leverantörsfamilj)
services/
  ingestion/          dränerar staged_events, upsertar till outages, staleness sweep
frontend/             Next.js dashboard, läser direkt från outages-tabellen
```

**Dataflöde:** en adapter pollar sin källa, normaliserar till `RawOutageEvent` (se `crates/types/src/lib.rs`), och skriver batchen transaktionellt till `staged_events` via `PostgresEventSink::write_batch`. `ingestion`-tjänsten lyssnar på Postgres `LISTEN/NOTIFY`, upsertar till den långlivade `outages`-tabellen och kör en periodisk staleness sweep som markerar avbrott som `resolved` om en adapter helt enkelt slutar rapportera dem. All uppslags-/dedupliceringslogik hör hemma i ingestion — **aldrig** i en adapter.

En adapter ska vara stateless: fetch + parse + `write_batch`. Den behöver inte komma ihåg vad den sett förut.

## Adapter-kontraktet

Implementera `Adapter`-traiten (`crates/adapter-sdk/src/lib.rs`):

```rust
#[async_trait::async_trait]
pub trait Adapter {
    fn name(&self) -> &'static str;             // måste matcha Provider::as_str()
    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>>;
}
```

- `name()` måste vara exakt samma slug som `Provider::as_str()` i `poms-types` (t.ex. `"vaxjo"`, inte `"digpro-vaxjo"`) — annars radar admin-sidans heartbeat-vy inte upp mot rätt `outages`-rader.
- Lägg till en ny variant i `Provider`-enumet (`crates/types/src/lib.rs`) för varje ny leverantör, även om den delar adapter-kod med en familj.
- `source_id` + `provider` måste tillsammans vara stabilt och unikt för samma verkliga avbrott mellan pollningar. Saknar källan ett naturligt id (t.ex. Jämtkraft): bygg ett deterministiskt (hash av plats + starttid), gissa inte ett löpnummer.
- `write_heartbeat` anropas en gång per tick av `run_poll_loop` — det är det enda hållbara livstecknet för en adapter, eftersom noll avbrott ger noll rader i `staged_events`. Bygger du en helt ny poll-loop manuellt (istället för `run_poll_loop`), glöm inte heartbeat.
- Lägg till den nya cargo-medlemmen i root-`Cargo.toml`s `[workspace] members` och som eget steg i `Dockerfile` (delat multi-stage-bygge — se root-Dockerfilen för mönstret, en `cargo build --release --workspace --bins` istället för separata builds per adapter).

## Kända leverantörsfamiljer — kolla dessa INNAN du bygger en helt ny adapter

Flera nätägare kör exakt samma bakomliggande plattform. En ny generisk `cust=`/env-var-konfiguration är nästan alltid rätt lösning istället för en ny crate:

- **Digpro "Outage Map"** (`crates/adapters/digpro/`): KML-feed, `cust=`-parameter identifierar tenant. Bekräftat: Kraftringen, Växjö/VEAB, Lerum, Västerbergslagens, Partille, Linde, Telge (annan servlet-path utan `.api.`, hanteras via `DIGPRO_KML_URL`-override). Kandidater att verifiera: Härnösand/HEMAB (`dpwebmap.hemab.se`, `cust=hem`), Falu Elnät (`webmap.fev.se`, `cust=fev`), Hedemora Energi (`webmap.hedemoraenergi.se`, `cust=hed`), Bergs Tingslags Elektriska (`dpweb.btea.se`, `cust=bte`), Härryda Energi (`instwebb.harrydaenergi.se`, `cust=hra`), Borlänge (`cust=bee`), Boden (`cust=ben`).
- **Tekla/GeoServer** (`crates/adapters/tekla/`): tile-baserat, koordinater joinas från separat endpoint. Bekräftat: Öresundskraft, Gävle Energi, Härjeåns Nät. Kompatibla men inte tillagda: Sundsvall Elnät, Mälarenergi (kör redan sin egen Next.js-API istället, se nedan).
- **ServiceAlert** (`crates/adapters/servicealert/`, `se.sms-service.dk`): Karlstads El, Eskilstuna Strängnäs, Tranås, Höganäs. **Kräver `#[serde(default, deserialize_with = ...)]`-hantering av `affectedAddressesCoordinates`** — fältet kommer ibland som explicit JSON `null` istället för `[]` eller utelämnat, vilket kraschar naiv deserialisering (se modulkommentaren, detta bet oss redan en gång på Karlstads El). Osäker statuslogik — ingen riktig statuskod i källan, bara ett textfönster.
- **Mälarenergi**: kör Tekla/GeoServer OCH ServiceAlert parallellt men vi använder deras egen renare Next.js-API (`malarenergi.se/api/outages`) istället.

Innan du skriver en ny adapter från scratch: kolla om leverantörens driftkarta-URL matchar någon av mönstren ovan (samma domänstruktur, samma `cust=`-parameter, samma JS-bundlenamn) — spar timmar.

## Research-metod för nya leverantörer

Källan för en i princip komplett lista över svenska nätägare med deras driftinformations-URL är `elsamverkan.se`s medlemsregister (`umbraco/surface/OrganizationList/GetOrganizationList`) — direkt-fetch är robots-blockerad, men sökmotorindexerade utdrag av sidan fungerar för att hitta `Hemsida`/`Störningssida`-par per bolag.

För att identifiera plattform bakom en okänd driftkarta: öppna sidan i en riktig webbläsare (Chrome-connectorn om tillgänglig, annars Playwright i sandboxen) och läs nätverksfliken efter XHR/fetch-anrop mot `/api/`, `.json`, `.kml`, eller GeoServer-tile-mönster — leta INTE efter mönster i den renderade HTML:en, moderna driftkartor är nästan alltid klientrenderade från en JSON/KML-endpoint.

## Bekräftat nere / lågprioriterat (kolla `README.md` för aktuellt datum)

- E.ON blockerat av Cloudflare-botskydd — försök inte kringgå.
- Kalmar Energi: bara en manuellt uppdaterad Google My Maps-karta, ingen realtids-API.
- Se `README.md` "Status"-avsnittet för fullständig och uppdaterad lista över vad som är klart, nere, eller overifierat — det är källan till sanning, inte den här filen.

## Köra lokalt

```bash
docker compose up --build
```

Kräver en `.env` bredvid `docker-compose.yml` med `ADMIN_SESSION_SECRET` (kör `openssl rand -hex 32`) — committa den aldrig.

## Konventioner

- Kommentarer och commit-meddelanden på svenska eller engelska, blanda inte inom samma fil.
- `anyhow::Result` + `.context(...)` i adaptrar/services för felkedjor som faktiskt går att felsöka i loggarna.
- `tracing`, inte `println!`.
- Skriv aldrig hemligheter (API-nycklar, `.env`, `ADMIN_SESSION_SECRET`) till repot.
