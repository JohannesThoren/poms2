# Legal riskbedömning

**Detta är inte juridisk rådgivning.** Jag är inte jurist, och det här är en teknisk sammanställning av relevanta juridiska ramverk och konkreta fynd - inte en garanti för att något är okej eller inte. Om projektet växer (fler användare, kommersiellt syfte, annonsintäkter i praktiken) är det värt att faktiskt konsultera en jurist, särskilt med svensk/EU-inriktning.

## Vad POMS2 faktiskt gör

Pollar ~30 nätägares egna driftkarte-API:er (samma data en vanlig besökare skulle se i sin webbläsare) var 60:e sekund, normaliserar till ett gemensamt schema, och visar upp aggregerat på en egen sajt. Ingen personuppgiftsbehandling i sig (adresser/ortnamn på infrastrukturnivå, inte kopplat till individer). Se `docs/data-sources.md` för konkreta fynd per leverantör.

## De fyra relevanta rättsområdena (EU/Sverige)

### 1. Avtalsrätt - användarvillkor (ToS)

De flesta av dessa sajter har inga tydliga "no scraping"-klausuler i sina publika användarvillkor (de flesta nätägares webbplatser har generiska villkor om köp av el, inte om automatiserad åtkomst till driftkartan). Men **robots.txt är ett tydligt, maskinläsbart uttryck för ägarens vilja**, och EU-domstolen (Ryanair v. PR Aviation, 2015) har slagit fast att en webbplatsägare kan **avtalsmässigt förbjuda scraping även av data som inte har något eget upphovsrätts- eller databasskydd** - så länge villkoret är giltigt ingånget. robots.txt i sig är inte ett bindande avtal (ingen "click-wrap"-acceptans), men det är starkt bevis på att åtkomsten sker mot ägarens uttryckliga vilja, vilket väger tungt om det någonsin blir en tvist.

**Konkret fynd:** Upplands Energi har `Disallow: /` - de säger nej till all automatiserad åtkomst till hela sajten, inklusive den endpoint vi hämtar från. Det är det tydligaste dokumenterade "nej" vi hittat. Ellevio disallowar specifikt `/avbrottskartan/` (deras appmapp) och `/static/`, men våra faktiska anrop (rot-URL och `/län/{slug}/idag`) ligger utanför de disallowade sökvägarna.

### 2. EU:s sui generis-databasrätt (databasdirektivet 96/9/EG)

Skyddar en databasägare som gjort en **väsentlig investering** i att samla in, kontrollera eller presentera databasens innehåll - oavsett om innehållet i sig är upphovsrättsskyddat. Skyddet gäller mot **utdrag eller återanvändning av en väsentlig del** av databasen, eller systematisk utvinning av oväsentliga delar som sammantaget blir väsentlig.

Det här är den mest relevanta risken för oss: en nätägares driftkarte-databas (samlad, verifierad, presenterad data om avbrott) skulle kunna anses ha krävt en väsentlig investering, och vi hämtar i praktiken **hela** deras aktuella dataset kontinuerligt, inte ett litet stickprov. Om en nätägare hävdade databasrätt skulle argumentet att vi gör en "väsentlig" utvinning vara rimligt starkt, eftersom vi speglar praktiskt taget allt de publicerar.

Motargument som mildrar risken: (a) den underliggande **faktan** (var ett avbrott är, hur många kunder, när det började) är inte skyddsbar i sig - bara den **sammanställda databasen** som sådan; (b) vår användning konkurrerar inte med nätägarens egen tjänst (de säljer inte tillgång till sin driftkarta, den är gratis och allmänt tillgänglig); (c) inget rättsfall vi hittat gäller specifikt allmännyttig infrastrukturdata av den här typen (jämfört med flygpriser/produktkataloger där rättspraxis faktiskt finns).

### 3. Dataintrång / obehörig åtkomst (svensk brottsbalk 4 kap. 9c§, EU:s NIS/Cybercrime-direktiv)

Relevant bara om åtkomsten kräver att man **kringgår ett tekniskt skydd** (inloggning, CAPTCHA, betalvägg, IP-blockering man aktivt undviker). Vi har konsekvent **avstått** från att försöka kringgå sådant (E.ON:s Cloudflare-botskydd är ett uttryckligt exempel där vi backat, se `README.md`). Alla endpoints vi använder är öppna, okrypterade, kräver ingen autentisering - i praktiken samma data som en webbläsare hämtar. Det här är den lägsta risken av de fyra, förutsatt att vi håller fast vid principen att aldrig kringgå ett aktivt skydd.

### 4. GDPR

Låg relevans. Vi behandlar inga personuppgifter i vanlig mening - platsdata är knuten till infrastruktur (nätområden, orter, adresser i vissa fall) inte till identifierbara individer. Undantag att hålla koll på: om någon adapter någonsin skulle exponera exempelvis ett specifikt hushålls elmätarnummer eller liknande, skulle det kunna klassas som indirekt identifierande - inget vi gör idag gör det.

## Praktiska rekommendationer

1. **Respektera robots.txt konsekvent** - vi bör sluta polla Upplands Energi tills vidare (eller kontakta dem för tillstånd) givet deras explicita `Disallow: /`. Det är den enda leverantören med ett så tydligt uttalat nej.
2. **Fortsätt aldrig kringgå tekniska skydd** (redan vår praxis - E.ON är det tydligaste exemplet).
3. **Overväg att höra av dig till några nätägare** om projektet växer och får trafik - många skulle troligen tycka det är okej eller till och med bra (gratis extra distributionskanal för deras driftinfo), men ett mejl i förväg är billig riskreducering jämfört med ett krav-brev i efterhand.
4. **Undvik att marknadsföra POMS2 som "källan"** till avbrottsinformationen - vi är en aggregator, inte en primärkälla. Källänkarna vi nu lagt till i UI:t (till respektive nätägares egen driftkarta) är bra för detta: transparens om varifrån datan faktiskt kommer, och ger nätägaren trafik tillbaka istället för att bara "stjäla" deras data utan attribution.
5. Om annonsintäkter faktiskt blir betydande, är punkt 3 mer angeläget - "vi tjänar pengar på er data utan att fråga" är en helt annan konversation än "hobbyprojekt utan intäkter".

## Google AdSense-specifikt

Se `docs/data-sources.md`s sista avsnitt - kort sammanfattning: Googles policy om "Replicated content" förbjuder annonser på sidor som kopierar/scrapar innehåll **utan att tillföra värde**. Vår aggregering (30+ källor i en vy, normaliserad status, karta, sök) är ett genuint värdetillägg jämfört med att bara spegla en enskild källa - jämförbart med godkända tjänster som flygspårare och prisjämförelsesajter. Risken är inte obefintlig (en automatisk granskare kan felaktigt flagga "scraped content" utan att förstå aggregeringsvärdet), men betydligt lägre än för ren textkopiering. Se dokumentet för fler detaljer och en konkret rekommendation.
