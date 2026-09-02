import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { verifySessionToken, COOKIE_NAME } from "@/lib/session";
import { getProviderStatus, getRecentActivity } from "@/lib/db";
import { providerName } from "@/lib/format";
import { LogoutButton } from "@/components/LogoutButton";

export const dynamic = "force-dynamic";

const KNOWN_PROVIDERS = [
  "ellevio",
  "vattenfall",
  "kraftringen",
  "jamtkraft",
  "tekniska_verken",
  "oresundskraft",
  "vaxjo",
  "lerum",
  "vasterbergslagens",
  "partille",
  "linde",
  "gavle",
  "skekraft",
  "karlstad",
  "eskilstuna_strangnas",
  "tranas",
  "uddevalla",
  "telge",
  "malarenergi",
  "upplands_energi",
  "voe",
  "hoganas",
  "eksjo",
  "pite",
  "harjeans",
];

const FRESH_MS = 5 * 60 * 1000;
const STALE_MS = 24 * 60 * 60 * 1000;

function health(lastObservedAt: string | null): { label: string; color: string } {
  if (!lastObservedAt) return { label: "Ingen data", color: "var(--resolved)" };
  const age = Date.now() - new Date(lastObservedAt).getTime();
  if (age < FRESH_MS) return { label: "OK", color: "var(--upcoming)" };
  if (age < STALE_MS) return { label: "Föråldrad", color: "var(--planned)" };
  return { label: "Inaktiv", color: "var(--fault)" };
}

function formatDateTime(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("sv-SE", { dateStyle: "short", timeStyle: "medium" });
}

export default async function AdminPage() {
  const cookieStore = await cookies();
  const username = verifySessionToken(cookieStore.get(COOKIE_NAME)?.value);
  if (!username) {
    redirect("/admin/login");
  }

  const [statusRows, activity] = await Promise.all([getProviderStatus(), getRecentActivity(200)]);
  const statusByProvider = new Map(statusRows.map((r) => [r.provider, r]));

  const rows = KNOWN_PROVIDERS.map((provider) => {
    const s = statusByProvider.get(provider);
    return {
      provider,
      last_observed_at: s?.last_observed_at ?? null,
      active_count: s?.active_count ?? 0,
      resolved_24h_count: s?.resolved_24h_count ?? 0,
      total_events_seen: s?.total_events_seen ?? 0,
    };
  }).sort((a, b) => {
    // Inaktiva/aldrig sedda leverantörer överst - de är det som behöver uppmärksamhet.
    const ha = health(a.last_observed_at).label;
    const hb = health(b.last_observed_at).label;
    if (ha === hb) return a.provider.localeCompare(b.provider);
    const order = ["Inaktiv", "Ingen data", "Föråldrad", "OK"];
    return order.indexOf(ha) - order.indexOf(hb);
  });

  return (
    <main className="flex-1 flex flex-col max-w-[1100px] w-full mx-auto px-6">
      <header className="flex items-end justify-between pt-10 pb-6 border-b border-[var(--line)]">
        <div>
          <h1 className="text-[15px] font-medium text-[var(--text)]">POMS2 admin</h1>
          <p className="text-sm text-[var(--muted)] mt-0.5">Inloggad som {username}</p>
        </div>
        <LogoutButton />
      </header>

      <section className="py-6 border-b border-[var(--line)]">
        <h2 className="text-sm text-[var(--muted)] mb-3">Status per leverantör</h2>
        <table className="w-full text-sm border-collapse">
          <thead>
            <tr className="text-left text-[var(--muted)] border-b border-[var(--line)]">
              <th className="font-normal py-2 pr-4">Leverantör</th>
              <th className="font-normal py-2 pr-4">Status</th>
              <th className="font-normal py-2 pr-4">Senast sedd</th>
              <th className="font-normal py-2 pr-4 text-right">Aktiva</th>
              <th className="font-normal py-2 pr-4 text-right">Åtgärdade (24h)</th>
              <th className="font-normal py-2 text-right">Totalt sedda</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const h = health(r.last_observed_at);
              return (
                <tr key={r.provider} className="border-b border-[var(--line)]/60">
                  <td className="py-2.5 pr-4 text-[var(--text)]">{providerName(r.provider)}</td>
                  <td className="py-2.5 pr-4">
                    <span className="inline-flex items-center gap-2">
                      <span className="inline-block h-2 w-2 rounded-full" style={{ backgroundColor: h.color }} />
                      {h.label}
                    </span>
                  </td>
                  <td className="py-2.5 pr-4 font-mono text-[var(--muted)]">{formatDateTime(r.last_observed_at)}</td>
                  <td className="py-2.5 pr-4 text-right font-mono">{r.active_count}</td>
                  <td className="py-2.5 pr-4 text-right font-mono text-[var(--muted)]">{r.resolved_24h_count}</td>
                  <td className="py-2.5 text-right font-mono text-[var(--muted)]">{r.total_events_seen}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      <section className="py-6 pb-10">
        <h2 className="text-sm text-[var(--muted)] mb-1">Senaste pollningsaktivitet</h2>
        <p className="text-xs text-[var(--muted)] mb-3">
          Varje rad är en batch en adapter skrev till <code>staged_events</code>. Det här är inte fullständiga
          applikationsloggar (adaptrarna loggar bara till stdout) - det här är vad som faktiskt finns kvar i databasen.
        </p>
        {activity.length === 0 ? (
          <p className="text-sm text-[var(--muted)] py-4">Ingen aktivitet registrerad än.</p>
        ) : (
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="text-left text-[var(--muted)] border-b border-[var(--line)]">
                <th className="font-normal py-2 pr-4">Leverantör</th>
                <th className="font-normal py-2 pr-4">Mottaget</th>
                <th className="font-normal py-2">Bearbetat</th>
              </tr>
            </thead>
            <tbody>
              {activity.map((a) => (
                <tr key={a.id} className="border-b border-[var(--line)]/60">
                  <td className="py-1.5 pr-4 text-[var(--text)]">{providerName(a.provider)}</td>
                  <td className="py-1.5 pr-4 font-mono text-[var(--muted)]">{formatDateTime(a.created_at)}</td>
                  <td className="py-1.5 font-mono text-[var(--muted)]">
                    {a.processed_at ? formatDateTime(a.processed_at) : "väntar…"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </main>
  );
}
