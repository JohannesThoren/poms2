import { getActiveOutages, getRecentlyResolved } from "@/lib/db";
import { Dashboard } from "@/components/Dashboard";
import { AdBanner } from "@/components/AdSlot";
import { sideAdsConfigured, topAdConfigured } from "@/lib/ads";

export const dynamic = "force-dynamic";

export default async function Home() {
  const [outages, resolved] = await Promise.all([getActiveOutages(), getRecentlyResolved(10)]);
  const now = new Date();
  const showSideAds = sideAdsConfigured();

  return (
    <>
      {topAdConfigured() && (
        <div className="w-full flex justify-center py-2 border-b border-[var(--line)] bg-[var(--panel)]">
          <AdBanner position="top" />
        </div>
      )}

      <div className="flex-1 flex justify-center w-full">
        {showSideAds && (
          <aside className="hidden xl:flex w-[160px] shrink-0 justify-center pt-10">
            <AdBanner position="left" />
          </aside>
        )}

        <main className="flex-1 flex flex-col max-w-[1100px] w-full mx-auto px-6">
          <Dashboard outages={outages} resolved={resolved} />
          <footer className="py-4 border-t border-[var(--line)] text-xs text-[var(--muted)]">
            Uppdaterad {now.toLocaleTimeString("sv-SE")} · källor: Ellevio, Vattenfall, Kraftringen, Tekniska verken,
            Öresundskraft, Växjö, Lerum, Västerbergslagens, Partille
          </footer>
        </main>

        {showSideAds && (
          <aside className="hidden xl:flex w-[160px] shrink-0 justify-center pt-10">
            <AdBanner position="right" />
          </aside>
        )}
      </div>
    </>
  );
}
