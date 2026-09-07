"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import type { Outage } from "@/lib/db";
import { providerName } from "@/lib/format";
import { OutageMap } from "@/components/OutageMap";
import { OutageSidebarList } from "@/components/OutageList";

/**
 * NOC ("network operations center") view: the map is the primary
 * surface, not an accessory below a table. A thin top bar carries the
 * headline numbers, a narrow sidebar carries the sortable/clickable
 * outage list, and everything else is map. List selection and map
 * selection are the same piece of state, so clicking either one drives
 * the other.
 */
export function NocView({ outages }: { outages: Outage[] }) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // "Active" here means already affecting someone right now - the same
  // definition the main dashboard uses for its "Aktuella avbrott"
  // section (fault, or planned work that has started). Not-yet-started
  // ("upcoming") outages don't belong on an operations map of what's
  // wrong *right now*.
  const active = useMemo(() => outages.filter((o) => o.status !== "upcoming"), [outages]);

  const totalCustomers = useMemo(
    () => active.reduce((sum, o) => sum + (o.affected_customers ?? 0), 0),
    [active]
  );

  const providerStats = useMemo(() => {
    const byProvider = new Map<string, { outageCount: number; customers: number }>();
    for (const o of active) {
      const entry = byProvider.get(o.provider) ?? { outageCount: 0, customers: 0 };
      entry.outageCount += 1;
      entry.customers += o.affected_customers ?? 0;
      byProvider.set(o.provider, entry);
    }
    return Array.from(byProvider.entries())
      .map(([provider, v]) => ({
        provider,
        outageCount: v.outageCount,
        customers: v.customers,
        perOutage: v.outageCount > 0 ? Math.round(v.customers / v.outageCount) : 0,
      }))
      .sort((a, b) => b.customers - a.customers);
  }, [active]);

  return (
    <div className="h-screen w-screen flex flex-col bg-[var(--bg)] text-[var(--text)] overflow-hidden">
      {/* Top bar */}
      <header className="shrink-0 border-b border-[var(--line)] bg-[var(--bg)] flex items-stretch">
        <div className="flex items-center gap-3 px-4 border-r border-[var(--line)] shrink-0">
          <Link href="/" className="text-sm text-[var(--muted)] hover:text-[var(--text)] underline">
            POMS2
          </Link>
          <span className="text-[var(--muted)]">·</span>
          <span className="text-sm text-[var(--text)]">NOC</span>
        </div>

        <div className="flex items-center px-4 border-r border-[var(--line)] shrink-0">
          <div
            className="font-mono text-2xl leading-none"
            style={{ color: totalCustomers > 0 ? "var(--fault)" : "var(--text)" }}
          >
            {totalCustomers.toLocaleString("sv-SE")}
          </div>
          <div className="text-xs text-[var(--muted)] ml-2 leading-tight max-w-[90px]">
            kunder påverkade totalt
          </div>
        </div>

        <div className="flex items-stretch overflow-x-auto">
          {providerStats.length === 0 && (
            <div className="flex items-center px-4 text-sm text-[var(--muted)]">Inga aktiva avbrott</div>
          )}
          {providerStats.map((p) => (
            <div
              key={p.provider}
              className="flex flex-col justify-center px-4 border-r border-[var(--line)] shrink-0 min-w-[140px]"
            >
              <div className="text-xs text-[var(--muted)] truncate">{providerName(p.provider)}</div>
              <div className="flex items-baseline gap-2">
                <span className="font-mono text-lg">{p.customers.toLocaleString("sv-SE")}</span>
                <span className="text-[11px] text-[var(--muted)]">kunder</span>
              </div>
              <div className="text-[11px] text-[var(--muted)] font-mono">
                {p.outageCount} avbrott · {p.perOutage.toLocaleString("sv-SE")}/avbrott
              </div>
            </div>
          ))}
        </div>
      </header>

      {/* Body: sidebar + map */}
      <div className="flex-1 flex min-h-0">
        <aside className="w-[300px] shrink-0 border-r border-[var(--line)] min-h-0">
          <OutageSidebarList outages={active} selectedId={selectedId} onSelect={setSelectedId} />
        </aside>
        <div className="flex-1 min-h-0">
          <OutageMap
            outages={active}
            selectedId={selectedId}
            onSelectId={setSelectedId}
            className="h-full w-full"
          />
        </div>
      </div>
    </div>
  );
}
