import Link from "next/link";
import { PROVIDER_NAMES, providerSourceUrl } from "@/lib/format";

export const metadata = {
  title: "Om POMS2",
};

export default function OmPage() {
  const providers = Object.keys(PROVIDER_NAMES).sort((a, b) => PROVIDER_NAMES[a].localeCompare(PROVIDER_NAMES[b], "sv"));

  return (
    <main className="flex-1 flex flex-col max-w-[720px] w-full mx-auto px-6">
      <header className="pt-10 pb-6 border-b border-[var(--line)]">
        <Link href="/" className="text-sm text-[var(--muted)] hover:text-[var(--text)]">
          ← Tillbaka till dashboarden
        </Link>
        <h1 className="text-xl font-medium text-[var(--text)] mt-4">Om POMS2</h1>
      </header>

      <section className="py-6 text-sm leading-relaxed text-[var(--text)] space-y-4">
        <p>
          POMS2 (Power Outage Monitoring System) samlar strömavbrottsinformation från svenska nätägares egna
          driftkartor på ett ställe. Sverige har över hundra nätägare, var och en med sin egen webbplats och sitt
          eget format - POMS2 hämtar den offentliga driftinformationen varje leverantör redan publicerar, normaliserar
          den till ett gemensamt format, och visar allt samlat i en karta och lista.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Hur det fungerar</h2>
        <p>
          En liten bakgrundsprocess (&quot;adapter&quot;) per nätägare läser samma öppna data som en vanlig besökare
          skulle se på leverantörens egen sida - ingen inloggning, ingen kringgång av tekniska skydd. Varje adapter
          pollar sin källa ungefär en gång i minuten. Om ett avbrott slutar rapporteras av källan markeras det som
          åtgärdat automatiskt.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Vad POMS2 inte är</h2>
        <p>
          POMS2 är <strong>inte en officiell källa</strong> och inte anslutet till någon nätägare. Vid ett verkligt
          strömavbrott, kontakta alltid din egen nätägare direkt - deras information är alltid mer aktuell och
          korrekt än en aggregerad kopia. Varje post i POMS2 länkar till leverantörens egen driftkarta som källa.
        </p>
        <p>
          Data kan vara fördröjd, ofullständig, eller fel om en källa ändrar sin webbplats utan att vi hunnit
          anpassa oss. Se{" "}
          <a
            href="https://github.com/JohannesThoren/poms2"
            target="_blank"
            rel="noopener noreferrer"
            className="underline hover:text-[var(--upcoming)]"
          >
            källkoden på GitHub
          </a>{" "}
          för fullständig teknisk dokumentation.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Nätägare</h2>
        <p>
          Är du en nätägare och har frågor om hur din data visas, vill bli borttagen, eller vill rätta något? Se{" "}
          <Link href="/for-natagare" className="underline hover:text-[var(--upcoming)]">
            sidan för nätägare
          </Link>
          .
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">
          Leverantörer som täcks just nu ({providers.length})
        </h2>
        <ul className="grid grid-cols-2 sm:grid-cols-3 gap-x-4 gap-y-1 text-[var(--muted)]">
          {providers.map((slug) => {
            const url = providerSourceUrl(slug);
            return (
              <li key={slug}>
                {url ? (
                  <a href={url} target="_blank" rel="noopener noreferrer" className="hover:text-[var(--text)]">
                    {PROVIDER_NAMES[slug]}
                  </a>
                ) : (
                  PROVIDER_NAMES[slug]
                )}
              </li>
            );
          })}
        </ul>
      </section>

      <footer className="py-6 border-t border-[var(--line)] text-xs text-[var(--muted)]">
        Byggt av Johannes Thorén ·{" "}
        <a href="mailto:johannes@lgjt.xyz" className="underline hover:text-[var(--text)]">
          johannes@lgjt.xyz
        </a>
      </footer>
    </main>
  );
}
