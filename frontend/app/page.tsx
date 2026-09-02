import { getActiveOutages, getRecentlyResolved } from "@/lib/db";
import { Dashboard } from "@/components/Dashboard";

export const dynamic = "force-dynamic";

export default async function Home() {
  const [outages, resolved] = await Promise.all([getActiveOutages(), getRecentlyResolved(10)]);
  const now = new Date();

  return (
    <main className="flex-1 flex flex-col max-w-[1100px] w-full mx-auto px-6">
      <Dashboard outages={outages} resolved={resolved} />
      <footer className="py-4 border-t border-[var(--line)] text-xs text-[var(--muted)]">
        Uppdaterad {now.toLocaleTimeString("sv-SE")} · källor: Ellevio, Vattenfall, Kraftringen, Tekniska verken,
        Öresundskraft, Växjö, Lerum, Västerbergslagens, Partille
      </footer>
    </main>
  );
}
