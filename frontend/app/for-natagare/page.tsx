import Link from "next/link";

export const metadata = {
  title: "För nätägare - POMS2",
};

export default function ForNatagarePage() {
  return (
    <main className="flex-1 flex flex-col max-w-[720px] w-full mx-auto px-6">
      <header className="pt-10 pb-6 border-b border-[var(--line)]">
        <Link href="/" className="text-sm text-[var(--muted)] hover:text-[var(--text)]">
          ← Tillbaka till dashboarden
        </Link>
        <h1 className="text-xl font-medium text-[var(--text)] mt-4">Information till nätägare</h1>
      </header>

      <section className="py-6 text-sm leading-relaxed text-[var(--text)] space-y-4">
        <p>
          Om du representerar en nätägare vars driftinformation visas på POMS2 - den här sidan förklarar exakt vad vi
          gör, varför, och hur du kommer i kontakt med oss om något behöver ändras.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Vad vi hämtar</h2>
        <p>
          POMS2 läser samma offentliga driftinformation som vilken besökare som helst kan se på er webbplats eller
          driftkarta - ingen inloggning krävs, inget tekniskt skydd kringgås. Vi hämtar ungefär en gång i minuten och
          sparar bara det som redan är aktivt publicerat (pågående, planerade och nyligen åtgärdade avbrott). Se{" "}
          <a
            href="https://github.com/JohannesThoren/poms2/blob/main/docs/data-sources.md"
            target="_blank"
            rel="noopener noreferrer"
            className="underline hover:text-[var(--upcoming)]"
          >
            docs/data-sources.md
          </a>{" "}
          för exakt vilken endpoint vi läser från er sida, och{" "}
          <a
            href="https://github.com/JohannesThoren/poms2/blob/main/LEGAL-RISK-REVIEW.md"
            target="_blank"
            rel="noopener noreferrer"
            className="underline hover:text-[var(--upcoming)]"
          >
            LEGAL-RISK-REVIEW.md
          </a>{" "}
          för vår egen bedömning av det juridiska läget.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Vi respekterar robots.txt</h2>
        <p>
          Om er robots.txt disallowar den sökväg vi läser från - eller om ni lägger till en sådan regel efter att ha
          läst det här - slutar vi hämta därifrån. Det gäller oavsett vad som står i den här texten i övrigt: en
          robots.txt-regel väger tyngre än vår egen bekvämlighet.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Vad vi inte gör</h2>
        <ul className="list-disc list-inside space-y-1 text-[var(--muted)]">
          <li>Vi kringgår aldrig inloggning, captcha, betalväggar eller IP-blockeringar.</li>
          <li>Vi säljer inte er data vidare och delar den inte med tredje part utöver att visa den på POMS2.</li>
          <li>Vi utger oss aldrig för att vara er, eller för att vara en officiell källa för er driftinformation.</li>
          <li>Vi belastar inte era servrar mer än nödvändigt - en hämtning per adapter och minut, inget mer.</li>
        </ul>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Vad ni får tillbaka</h2>
        <p>
          Varje post som kommer från er visar en tydlig källänk till er egen driftkarta - vi gör inget anspråk på att
          vara källan till informationen. Om ni har en bättre, mer officiell endpoint ni hellre vill att vi använder
          (eller en riktig API-nyckel/samarbete), hör gärna av er - vi byter gärna till något ni själva kontrollerar.
        </p>

        <h2 className="text-[var(--muted)] text-xs uppercase tracking-wide pt-2">Kontakt</h2>
        <p>
          Vill ni bli helt borttagna, rätta felaktig information, eller bara veta mer - mejla{" "}
          <a href="mailto:johannes@lgjt.xyz?subject=POMS2%20-%20n%C3%A4t%C3%A4gare" className="underline hover:text-[var(--upcoming)]">
            johannes@lgjt.xyz
          </a>
          . Vi svarar på och åtgärdar borttagningsförfrågningar inom rimlig tid utan att ställa några krav i gengäld.
        </p>
      </section>

      <footer className="py-6 border-t border-[var(--line)] text-xs text-[var(--muted)]">
        <Link href="/om" className="underline hover:text-[var(--text)]">
          Om POMS2
        </Link>
      </footer>
    </main>
  );
}
