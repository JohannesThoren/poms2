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

Alla Rust-tjänster (adaptrar + ingestion) byggs från en enda delad `Dockerfile` i repo-roten - en gemensam byggsteg kompilerar hela workspacet en gång (med BuildKit cache-mounts för cargo-registret och target-mappen), och varje tjänst pekar bara på sitt eget `target:`-steg för att plocka ut sin binär. Det håller ombyggnadstiden nere jämfört med att varje adapter kompilerar om alla delade dependencies (tokio, sqlx, reqwest, ...) från grunden.

## Admin-sidan (`/admin`)

Ett status-/loggläge för drift, skyddat av inloggning mot **hostens egna Linux-användare och grupper** (samma mönster som docker-manager-projektet) - ingen separat lösenordsdatabas i appen.

Engångssetup på servern som kör `docker compose`:

```bash
sudo groupadd poms-admin
sudo usermod -aG poms-admin <ditt-användarnamn>
```

Skapa en `.env`-fil bredvid `docker-compose.yml` (committa den aldrig):

```
ADMIN_SESSION_SECRET=<kör: openssl rand -hex 32>
```

Logga sedan in på `/admin` med samma användarnamn/lösenord du loggar in på servern med. `frontend`-containern monterar hostens `/etc/passwd`, `/etc/shadow` och `/etc/group` read-only och verifierar via PAM - den har ingen egen kopia av lösenorden.

## Status

- [x] Grundarkitektur (types, db, adapter-sdk, ingestion)
- [x] Karta med polygon-stöd: Vattenfall, Kraftringen och Skellefteå Kraft ger ett riktigt avbrottsområde (inte bara en punkt) - lagras i `outages.polygon` (JSONB) och ritas ut på kartan vid inzoomning (zoom ≥ 9), utöver punktmarkören som alltid visas
- [x] Ellevio-adapter (**omskriven igen 2026-09-05**) - upptäckte att `/län/{slug}/idag` server-renderar en fullständig, live kommunnedbrytning för det länet (till skillnad från `/län/{slug}` utan `/idag`, som bara ger samma nationella fallback). Adaptern upptäcker nu länslistan dynamiskt från förstasidan och hämtar kommunnivå-data från varje läns egen sida - täcker alla ~76 kommuner i Ellevios nuvarande område (7 län: Dalarna, Gävleborg, Halland, Stockholm, Värmland, Västra Götaland, Örebro), självuppdaterande, ingen hårdkodad kommunlista längre. Oplanerade/Planerade är kundantal (bekräftat).
- [x] Vattenfall-adapter (`incidents.json`, riktiga koordinater via polygon-centroid)
- [x] Kraftringen-adapter (KML-feed, riktiga koordinater, saknar ortnamn - se modulkommentar)
- [x] Tekniska verken-adapter (`api.tekniskaverken.net/outage/v1/public/outages`, rikast källan hittills)
- [x] Öresundskraft-adapter (Tekla/GeoServer tile-system, koordinater joinade från separat endpoint)
- [x] Digpro "Outage Map"-familjen: en generisk adapter (env-var-konfigurerad) täcker Växjö Energi, Lerum Energi, Västerbergslagens Elnät, Partille Energi, Linde Energi - samma bakomliggande system som Kraftringen, se `crates/adapters/digpro/src/lib.rs` för hur det hittades
- [x] Tekla/GeoServer-familjen: en generisk adapter (env-var-konfigurerad) täcker Gävle Energi och Härjeåns Nät utöver Öresundskraft - samma bakomliggande system, se `crates/adapters/tekla/src/lib.rs`. Sundsvall Elnät och Mälarenergi bekräftat kompatibla men inte tillagda som tjänster än (Mälarenergi täcks redan via sin egen Next.js-API istället).
- [x] Umeå Energi-adapter (`uewebstorage.blob.core.windows.net/disturbance-data-prod/disturbances.json`, statisk Azure blob-JSON, ingen auth) - hittad via `.json`-sökning i sidans HTML istället för att gräva i Nuxt-bundlen. Täcker el/bredband/fjärrvärme/fjärrkyla i en och samma feed (`domain`-fält), bara el (`domain=0`) tas med. Riktig GeoJSON Point/Polygon-geometri, explicit `endDate` (används direkt för `Resolved` istället för att bara förlita sig på staleness sweep). **Overifierad statuslogik** - se varningen i `crates/adapters/umeaenergi/src/lib.rs` (inget publicerat facit för `state`/`type`, mappningen är en rimlig gissning baserad på stickprov).
- [x] HEM/Halmstads Energi och Miljö tillagd via Digpro-familjen (`dpwebmap.hem.se`, `cust=hsd`) - hittad som en vanlig länk i deras Next.js-sidas renderade HTML (`/avbrottsinformation/avbrottskarta`), ingen JS-bundle-grävning behövdes. KML-endpointen verifierad live (HTTP 200, riktig data).
- [x] **Korrigering (2026-09-07):** ovanstående var fel diagnos. Sandboxens utgående IP är en Google Cloud-adress i USA (`AS396982 Google LLC`, North Charleston) - flera av dessa system blockerar/svartlistar uppenbarligen datacenter-/utlands-IP:er, vilket ger exakt symptomen ovan (TLS klar men aldrig ett svar, eller ett generiskt 500/503) utan att servern faktiskt är nere. Bekräftat genom att Johannes testade samma URL:er i en vanlig webbläsare på svensk uppkoppling och fick HTTP 200 på flera av dem. Följande är alltså **riktiga, fungerande Digpro-instanser** trots att adaptern/sandboxen inte kan nå dem härifrån:
  - **HEMAB** (`dpwebmap.hemab.se`, `cust=hem`)
  - **BTEA** (`dpweb.btea.se`, `cust=bte`)
  - **Härryda Energi** (`instwebb.harrydaenergi.se`, `cust=hra`)
  - Lades till som digpro-adapter-tjänster i docker-compose ändå (samma mönster som Halmstad) - de kommer fungera när de körs från en riktig (svensk/icke-datacenter) server, vilket är hela poängen med att köra dem i produktion och inte i den här sandboxen.
  - **Jämtkraft** (`avbrottskarta.jamtkraft.se`) är fortfarande obekräftad plattform - vi vet bara att sidan är nåbar från en vanlig webbläsare, inte vilket system som ligger bakom. Behöver någon med svensk uppkoppling öppna sidan, öppna webbläsarens nätverksflik (F12 → Network) och se vilken URL som hämtar själva avbrottsdatan.
  - **Falu Elnät** (webmap.fev.se, `cust=fev`) och **Hedemora Energi** är inte omtestade än efter den här upptäckten - värda en ny koll från en icke-blockerad uppkoppling innan de skrivs av som trasiga.
- [ ] Nya kandidater hittade men inte verifierade än (ingen uppenbar öppen JSON/API i förstasidans HTML, kräver djupare grävning i deras JS-bundlar): Norrtälje Energi (WordPress), C4 Elnät, Sandviken Energi, Bjärke Energi, Falkenberg Energi (WordPress) - kollade WP REST-routes för Bjärke och Falkenberg, inga egna driftstörnings-endpoints registrerade, sannolikt manuellt uppdaterad text utan API
- [ ] Falbygdens Energi: länken från elsamverkan.se gav 404, sidan har troligen flyttat
- [x] Frontend (Next.js, `/frontend`) - dashboard som läser direkt från `outages`-tabellen, alltid färsk rendering (`dynamic = "force-dynamic"`), interaktivt statusfilter
- [ ] Fler Digpro-kandidater att verifiera när de kommer upp: Falu Elnät (`webmap.fev.se`, cust=fev), Härnösand Elnät (`dpwebmap.hemab.se`, cust=hem), Hedemora Energi (`webmap.hedemoraenergi.se`, cust=hed), Bergs Tingslags Elektriska (`dpweb.btea.se`, cust=bte), Härryda Energi (`instwebb.harrydaenergi.se`, cust=hra) - alla formler bekräftade, servrarna bara nere/felande just nu
- [x] Skellefteå Kraft-adapter (`driftinfo3-api.skekraft.se/api/disturbances`, eget system, hittat via app.config.js)
- [x] ServiceAlert-familjen (`se.sms-service.dk`): en generisk adapter täcker Karlstads El, Eskilstuna Strängnäs Energi, Tranås Energi, Höganäs Energi (Uddevalla bytte till Digpro). **Osäker statuslogik** - se varningen i `crates/adapters/servicealert/src/lib.rs` (ingen riktig statuskod i källan, bara ett visningsfönster + fritext; inte verifierad mot ett riktigt elavbrott än)
- [x] Eksjö Energi-adapter (server-renderad HTML, `eksjoenergi.se/driftinformation/`, ingen egen ID i källan så adaptern hashar titel+tid till ett stabilt id)
- [x] PiteEnergi-adapter (server-renderad HTML, tre tydliga sektioner Pågående/Planerade/Avklarade - en av de renaste källorna, riktiga ID:n och hushållsantal)
- [x] Karlshamn Energi-adapter (AJAX-endpoint `.../ajax/beredskapsalarm/get_data.php`). **Osäker statuslogik för el** - se varningen i `crates/adapters/karlshamn/src/lib.rs` (bara vatten hade aktiva poster när adaptern byggdes, el/status-mappningen är en rimlig gissning baserad på samma "ok/varning/akut"-konvention som andra kommunala sidor använder)
- [x] Telge Nät via Digpro (annan servlet-sökväg, saknar ".api." - adaptern stödjer nu båda varianterna via `DIGPRO_KML_URL`)
- [x] Mälarenergi-adapter (egen Next.js-API, `malarenergi.se/api/outages`) - de kör även Tekla/GeoServer OCH ServiceAlert parallellt, vi valde den renaste av de tre
- [x] Upplands Energi-adapter (`avbrott.upplandsenergi.se`, eget system). **Overifierat tidsformat** - se varningen i `crates/adapters/upplandsenergi/src/lib.rs` (interruptions.json var tomt hela tiden, start/end/est-fälten är gissade)
- [x] Västra Orusts Energitjänst-adapter (`voe.se/mirakel/news.json`, eget "Mirakel"-system) - riktiga UTC-tider, ingen DST-gissning, men saknar kundantal helt
- [ ] Skövde Energi: hittade en tredje integrationsstil (WordPress-plugin som proxar samma bakomliggande tjänst), men svaret är tomt just nu så fältnamnen är overifierade - bygg när skarp data finns
- [ ] Karlskoga Elnät: ingen API hittad vid snabb koll, kan behöva mer interaktion (scroll/klick) för att trigga kartladdning
- [ ] Kalmar Energi använder bara en manuellt uppdaterad Google My Maps-karta, ingen realtids-API - troligen inte värt att bygga adapter för
- [ ] E.ON blockerat av Cloudflare-botskydd - inget vi försöker kringgå
- [ ] Frontend
