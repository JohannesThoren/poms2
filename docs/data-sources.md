# Vad källorna säger om automatiserad datainsamling

Genomgång av `robots.txt` (det enda maskinläsbara, konsekvent kollbara signalet) för varje källa POMS2 hämtar från, plus eventuella villkorstexter hittade på vägen. Se `LEGAL-RISK-REVIEW.md` för vad det här faktiskt betyder juridiskt. Kollat 2026-09-17.

**Metod:** `curl https://<domän>/robots.txt`. Ett API-endpoints egen subdomän saknar ofta en egen robots.txt (404) - det tolkas som "ingen explicit regel", inte som ett aktivt tillstånd.

## 🔴 Explicit nej till automatiserad åtkomst

| Leverantör | Domän | Fynd |
|---|---|---|
| **Upplands Energi** | avbrott.upplandsenergi.se | `Disallow: /` - förbjuder ALL automatiserad åtkomst till hela sajten, inklusive `/managed/interruptions.json` som vår adapter hämtar från. Det enda tydliga, otvetydiga "nej" vi hittat. |

## 🟡 Delvis begränsat (men inte den väg vi använder)

| Leverantör | Domän | Fynd |
|---|---|---|
| Ellevio | avbrottskarta.ellevio.se | `Disallow: /avbrottskartan/` och `/static/` - deras app-internals. Våra anrop går mot rot (`/`) och `/län/{slug}/idag`, utanför de disallowade sökvägarna. |

## 🟢 Inget hinder hittat

| Leverantör | Domän | robots.txt |
|---|---|---|
| Vattenfall | www.vattenfalleldistribution.se | Ingen Disallow, bara sitemap |
| PiteEnergi | www.piteenergi.se | Ingen Disallow (Yoast SEO-standard) |
| Eksjö Energi | eksjoenergi.se | Ingen Disallow för vårt path (bara `/wp/wp-admin/`) |
| Växjö Energi (VEAB) | www.veab.se | Ingen Disallow |
| Karlshamn Energi | www.karlshamnenergi.se | Ingen Disallow för vårt path (bara `/wp-admin/`) |
| Mälarenergi | www.malarenergi.se | `Allow: /` explicit |
| Umeå Energi | driftinfo.umeaenergi.se | `Allow: /` explicit |
| Västra Orusts Energitjänst | voe.se | Ingen Disallow för vårt path |
| HEM (Halmstad) | www.hem.se | Ingen Disallow för vårt path |

## ⚪ Ingen robots.txt hittad (API-subdomän utan egen policy)

Dessa är rena API-endpoints (KML/JSON) på en subdomän skild från företagets huvudsajt - ingen robots.txt där, vilket är normalt för en teknisk endpoint snarare än ett medvetet val att inte reglera:

Kraftringen (avbrott.kraftringen.se), Tekniska verken (api.tekniskaverken.net), Öresundskraft (driftinformation.oresundskraft.se, 302-redirect), Gävle Energi (avbrottskartan.gavleenergi.se), Skellefteå Kraft (driftinfo.skekraft.se), Härjeåns Nät (avbrottskarta.harjeans.se), Digpro-familjen (webmap.lindeenergi.se m.fl. - samma vendor-plattform för Växjö/Lerum/Västerbergslagens/Partille/Linde/Telge/Härryda/Uddevalla), ServiceAlert (se.sms-service.dk - dansk tredjepartsleverantör, serverar sin SPA-shell även på `/robots.txt`, ingen tydlig signal).

## Användarvillkor (ToS)

Ingen av leverantörernas publika användarvillkor vi läst igenom nämner automatiserad åtkomst till driftkartan specifikt - de handlar nästan uteslutande om elavtal/köpvillkor. Det betyder inte att det är fritt fram (se databasrätts-avsnittet i `LEGAL-RISK-REVIEW.md`), bara att det inte finns en explicit textklausul att bryta mot utöver robots.txt-fallet ovan.

## Google AdSense - policyfynd

Googles [Publisher Policies](https://support.google.com/adsense/answer/10502938) förbjuder annonser på sidor med "embedded or copied content from others **without additional commentary, curation, or otherwise adding value**". Exempel de nämner: "mirroring, framing, scraping, or rewriting content from other sources without adding value."

**Bedömning:** POMS2:s upplägg (30+ källor samlade i en vy, normaliserad status över olika underliggande system, karta, sök, historik) är ett genuint värdetillägg jämfört med att bara spegla en enskild källa rakt av - jämförbart med redan etablerade, AdSense-godkända tjänster som flygspårare (FlightAware-typ) och prisjämförelsesajter, som också bygger hela sin verksamhet på att aggregera andras realtidsdata. Policyn är formulerad med tanke på artikel-/textkopiering (blogginlägg, nyheter), inte strukturerad faktadata-aggregering - men en automatisk granskare förstår inte alltid den skillnaden.

**Rekommendation:** Om/när AdSense-ansökan görs, se till att sajten tydligt förklarar **vad den tillför** (en "Om POMS2"-sektion: "aggregerar X leverantörer, uppdateras var 60:e sekund, källänkar till varje leverantörs egen sida") snarare än att bara visa rådata utan kontext - det är både bättre för användaren och minskar risken att en granskare felklassar sajten som "scraped content".
