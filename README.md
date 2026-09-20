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

## Annonser (valfritt)

Dashboarden kan visa en topp-banner och två sido-banners (Google AdSense) - helt av som standard, syns inte alls förrän du konfigurerar det. Lägg till i din `.env`:

```
NEXT_PUBLIC_ADSENSE_CLIENT_ID=ca-pub-XXXXXXXXXXXXXXXX
NEXT_PUBLIC_AD_SLOT_TOP=<annons-enhets-id>
NEXT_PUBLIC_AD_SLOT_LEFT=<annons-enhets-id>
NEXT_PUBLIC_AD_SLOT_RIGHT=<annons-enhets-id>
```

Kräver ett godkänt AdSense-konto (skapa annonsenheter i deras gränssnitt, klistra in slot-ID:na ovan). `/ads.txt` genereras automatiskt från client-ID:t. Sido-bannerna visas bara på breda skärmar (≥ xl-brytpunkt) - på mobil/surfplatta får dashboarden all bredd. Bygg om frontend efter att ha ändrat dessa (`NEXT_PUBLIC_*`-variabler bakas in i klientbunten vid byggtillfället, inte körtid).

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
- [x] **Korrigering (2026-09-07):** ovanstående var fel diagnos. Sandboxens utgående IP är en Google Cloud-adress i USA (`AS396982 Google LLC`, North Charleston) - flera av dessa system blockerar/svartlistar uppenbarligen datacenter-/utlands-IP:er, vilket ger exakt symptomen ovan (TLS klar men aldrig ett svar, eller ett generiskt 500/503) utan att servern faktiskt är nere. Bekräftat genom att Johannes testade samma URL:er i en vanlig webbläsare på svensk uppkoppling och fick HTTP 200. **HEMAB** (`cust=hem`), **BTEA** (`cust=bte`) och **Härryda Energi** (`cust=hra`) är alltså riktiga, fungerande Digpro-instanser och tillagda som tjänster i docker-compose (samma mönster som Halmstad) - de fungerar när de körs från riktig (icke-datacenter) infrastruktur, vilket är hela poängen med att köra dem i produktion.
- [x] **Jämtkraft** tillagd via Tekla/GeoServer-familjen (`avbrottskarta.jamtkraft.se`) - identifierad genom att Johannes hittade `geoserver-api/content/StaticObjects` i webbläsarens nätverksflik, vars `"__type":"Area:#Tekla.Technology.GeoData"` avslöjade plattformen. `GetApplicationData`/`GetObjectsByTiles` (de faktiska endpoints adaptern använder) inte direkt verifierade live härifrån - sandboxens IP är blockerad på den domänen precis som HEMAB/BTEA/Härryda - men slutsatsen är säker: `content/outageTableData.json` gav identisk tom-schema-form som redan bekräftat fungerande Öresundskraft och Gävle Energi på samma plattform, så formeln är pålitlig.
- [x] **Gotlands Energi (GEAB)** - helt annorlunda plattform än alla andra: deras publika "Avbrottskarta" är en Esri ArcGIS Experience Builder-app som pekar direkt mot deras interna nätarbetsordersystem, exponerat som en öppen ArcGIS Feature Service (`services7.arcgis.com/.../Outages_view/FeatureServer/0`). Hittad genom att följa Experience Builder-appens item-ID → web map-item-ID → operationalLayers-URL. Statuslogiken bygger på `type_txt` ("Driftavbrott" = oplanerat, "Koppling" = planerat) snarare än det interna `state_txt`-arbetsflödesfältet (som är GEAB:s egen process, inte en ren avbrottstaxonomi). Koordinater i Web Mercator, konverterade till WGS84 i adaptern. Verifierat live: 5/5 poster gav rimlig statusfördelning och koordinater som landar korrekt på Gotland.
- [x] **Herrljunga Elektriska** - server-renderad WordPress-lista (`/storning/`, custom post type `servicestatus`), samma mönster som Eksjö/PiteEnergi/Karlshamn. Delar en och samma flöde med vattenavbrott utan egen kategori-fält - filtreras på nyckelord i titeln ("ström"/"elavbrott"/"elnät" vs "vatten"). Status avgörs av ett bokstavligt `[KLAR]`/`[KLART]`-prefix i titeln (båda stavningarna betyder samma sak, bara olika böjning) - **bugg hittad och fixad live**: första versionen missade `[KLART]`-varianten helt och skulle ha visat ett redan åtgärdat avbrott som pågående. Verifierat mot skarpt API efter fixen: exakt de 2 faktiskt aktiva elavbrotten kvar, det åtgärdade korrekt bortfiltrerat.
- [x] **Falbygdens Energi** tillagd via ServiceAlert-familjen - avslöjade en betydande designbrist i den generiska adaptern: Falbygden delar upp el i **två separata profiler** ("Elnät planerade avbrott" med profileName="Elnät", och "Elnät akuta avbrott" med profileName="Info" - inte "Elnät" alls!). Det gamla filtret (exakt profileTitle == "Elnät") hade tyst missat båda profilerna hos den här kunden för alltid. Bytt till substräng-matchning, vilket täcker alla kända kunder korrekt. Ny regressionstest med riktig fixture låser fast beteendet.
- [x] **Sandviken Energi** - SiteVision inbyggd "archive portlet" (inte en anpassad webapp-widget som Höganäs), server-renderad direkt i HTML:en. En delad lista för el/vatten/fjärrvärme/bredband utan eget kategori-fält förutom fritext i formatet "Ort - Kategori - Titel" - filtreras på Kategori == "Elnät". Två sektioner på samma sida identifierade via markör-divs (`#Pagaende`/`#Planerade`) - måste traversera `scraper`s underliggande `ego_tree` direkt eftersom `NodeId` saknar `Ord` för positionsjämförelse. Verifierat mot färsk skarp data (inte bara fixture): fångade dagens riktiga strömavbrott på Agavägen.
- [x] **VänerEnergi** - samma SiteVision archive-portlet-teknik som Sandviken, men enklare feed: rent elnät (ingen delad kategori att filtrera bort), och åtgärdad status är inbakad direkt i titeltexten som ett bokstavligt " - Avslutad"-suffix istället för en egen sektion. Inget numeriskt ID i URL:en - använder slug:en direkt som source_id.
- [x] **Kungälv Energi** - server-renderad SiteVision "script portlet" (`b3-rss-driftinformation`, en anpassad RSS-driven widget, inte den inbyggda archive-portleten). Sidan har också en iframe mot den blockerade delade `avbrottsinfo.svenskaenergigruppen.se`-plattformen, men adaptern läser Kungälvs egen server-renderade spegling av samma data direkt istället - kringgår blockeringen helt. Delad el/stadsnät-feed, filtreras på kategori-badge. Verifierat live: fångade ett riktigt planerat elnätsavbrott i Kungälv.
- [x] **Skurups Elverk** - ännu en SiteVision "webapp", men datan ligger direkt inbäddad som JSON i en `<script>`-tagg (`AppRegistry.registerInitialState(...)`) - ingen separat API-hämtning behövs alls, bara extrahera den självavgränsande JSON-blobben ur sidan. Inget kategori-fält (`tags` alltid tom) - filtreras på nyckelord i fritext-titeln. **Osäker statuslogik**: alla riktiga poster som fanns när adaptern byggdes var redan "Åtgärdat" (åtgärdade), så mappningen för en genuint pågående/oplanerad post är en rimlig gissning, inte bekräftad mot ett skarpt exempel.
- [x] **Övik Energi** - en tredje distinkt SiteVision-variant: en "predefinedsearch portlet" (sparad sökning som innehållslista), skild från både archive-portleten (Sandviken/VänerEnergi) och webapp-varianten (Skurup/Höganäs). Tre sektioner (Pågående/Planerade/Åtgärdade) på sidan, men varje post bär sin egen statusklass (`drift-status-{status}`) direkt - behöver inte hålla reda på vilken sektion en post kom från. **Osäker statuslogik**: bara `drift-status-planned` bekräftat mot ett skarpt exempel (sidan hade exakt en aktuell post totalt när adaptern byggdes) - "ended" är en rimlig gissning via analogi med Skurup, inte verifierad.
- [x] **Buggfix i Tekla-adaptern**: `GetApplicationData` delar upp avbrott per "scope"-nyckel (`p` = el, `h` = fjärrvärme, bekräftat genom att Gävle Energis `h`-scope innehöll riktiga områdesdata men 0 avbrott just när detta upptäcktes). Den gamla koden slog ihop *alla* scope-nycklar urskillningslöst - ofarligt hittills bara för att `h` råkat vara tom, men skulle ha rapporterat fjärrvärmeavbrott som elavbrott för Gävle/Härjeåns Nät så fort ett sånt inträffade. Fixat till att bara använda `p`. Öresundskrafts egna (separata) adapter hade aldrig buggen - dess `Scopes`-struct har bara ett hårdkodat `p`-fält.
- [ ] **Falu Elnät** och **Hedemora Energi** är inte omtestade från en icke-blockerad uppkoppling än - värda en ny koll innan de skrivs av som trasiga (`cust=fev` respektive-formel redan bekräftad korrekt).
- [ ] Nya kandidater hittade men inte verifierade än (ingen uppenbar öppen JSON/API i förstasidans HTML, kräver djupare grävning i deras JS-bundlar eller nätverksflik från en icke-blockerad uppkoppling): Norrtälje Energi (WordPress), C4 Elnät, Bjärke Energi, Falkenberg Energi (WordPress) - kollade WP REST-routes för Bjärke och Falkenberg, inga egna driftstörnings-endpoints registrerade, sannolikt manuellt uppdaterad text utan API. Obs: dessa testades innan IP-blockeringen upptäcktes, så "inget hittat" kan bero på samma blockering snarare än att det faktiskt saknas en API - värda en omkoll från en vanlig uppkoppling.
- [ ] VEAB (Växjö Energi, redan täckt via Digpro) har en andra, rikare sida på `www.veab.se/driftinformation/elnat/` med riktig kundräkningstabell ("Antal kunder"-kolumn) - potentiellt en kvalitetsuppgradering över nuvarande Digpro-baserade adapter, inte undersökt klart.
- [ ] **Ny delad plattform hittad men blockerad**: `avbrottsinfo.svenskaenergigruppen.se/{företagsslug}/` ("Svenska Energigruppen") - används av minst Ale El och Njudung Energi (Sävsjö/Vetlanda), troligen fler. Hela domänen gav 503 vid alla försök (rot, undermappar, gissade API-sökvägar) - samma mönster som andra domäner blockerade för sandboxens IP. Värd en omkoll från en icke-blockerad uppkoppling - kan vara ännu en Digpro/Tekla-liknande multi-tenant-vinst.
- [ ] **Ny riktig multi-tenant-plattform hittad, men ingen data att verifiera mot**: Embriq/Quant Insight (`powerapi.prod.hub.quant.embriq.no/outage/netowner/{id}/outage`) - ett norskt kraftbolagsverktyg med bekräftade svenska kunder. Två fungerande netowner-ID:n hittade: Boo Energi (`18100`) och Västervik Miljö & Energi (`38800`, länk: `quantinsight.net/outage/index.html#/38800` på deras egen sida). Formeln fungerar (giltig JSON, `{"outages":[...],"plannedOutages":[...]}`) men **båda var helt tomma vid test** - inget riktigt exempel på ett faktiskt avbrottsobjekt att verifiera fältnamn (koordinater, kundantal, tider) mot. Bygg när ett skarpt exempel finns, annars gissar vi blint på schema. Nacka Energi bekräftad Digpro-formel (`dp-prod.nackaenergi.se`, cust=nac) men blockerad för sandboxens IP, samma mönster som HEMAB/BTEA/Härryda.
- [ ] SEOM (Sollentuna Energi och Miljö) `seom.se/driftstatus/` - "Planerade avbrott"-sektionen är tillförlitligt server-renderad (Umbraco), men "Driftstörningar" (aktiva fel - den viktigaste kategorin) laddas bara i webbläsaren och saknas i en enkel HTTP-hämtning. Inget bekräftat Elnät-exempel tillgängligt heller - kategori-fältet verkade inkonsekvent (ibland ort, ibland kategori). Behöver antingen nätverksfångst för att hitta den dolda endpointen för aktiva fel, eller en riktig Elnät-post att verifiera mot. Luleå Energi också kollad - verkar inte publicera någon webbaserad avbrottslista alls (hänvisar till sin app istället).
- [ ] Vimmerby Energi Nät (VEMAB) `vemab.se/driftstorningar/aktiva-driftstorningar` - 503 vid flera separata försök, oklart om samma orsak som ovan eller en egen driftstörning på deras sida. Ser annars ut att ha en ren strukturerad sida ("Det finns inga registrerade driftstörningar" när tomt).
- [x] Frontend (Next.js, `/frontend`) - dashboard som läser direkt från `outages`-tabellen, alltid färsk rendering (`dynamic = "force-dynamic"`), interaktivt statusfilter
- [ ] Fler Digpro-kandidater att verifiera när de kommer upp: Falu Elnät (`webmap.fev.se`, cust=fev), Hedemora Energi (`webmap.hedemoraenergi.se`, cust=hed) - formler bekräftade, inte omtestade från en icke-blockerad uppkoppling än
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

## License

Copyright (c) 2026 Johannes Thorén. All rights reserved.

Licensed under the [LGJT License v1](LICENSE). Personal, non-commercial use
only. Anything else requires written permission: johannes@lgjt.xyz

