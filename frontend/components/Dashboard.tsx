"use client";

import { useMemo, useState } from "react";
import type { Outage } from "@/lib/db";
import { providerName, STATUS_LABELS, formatTime } from "@/lib/format";
import { OutageMap } from "@/components/OutageMap";

const FILTERABLE_STATUSES = ["fault", "planned", "upcoming"] as const;
type FilterableStatus = (typeof FILTERABLE_STATUSES)[number];

function StatusDot({ status }: { status: string }) {
  const color =
    status === "fault"
      ? "var(--fault)"
      : status === "planned"
        ? "var(--planned)"
        : status === "upcoming"
          ? "var(--upcoming)"
          : "var(--resolved)";
  return (
    <span
      className={`inline-block h-2 w-2 rounded-full ${status === "fault" ? "pulse" : ""}`}
      style={{ backgroundColor: color }}
    />
  );
}

function OutageTable({ outages }: { outages: Outage[] }) {
  if (outages.length === 0) {
    return <p className="text-sm text-[var(--muted)] py-8">Inga avbrott matchar det valda filtret.</p>;
  }
  return (
    <table className="w-full text-sm border-collapse">
      <thead>
        <tr className="text-left text-[var(--muted)] border-b border-[var(--line)]">
          <th className="font-normal py-2 pr-4 w-8"></th>
          <th className="font-normal py-2 pr-4">Leverantör</th>
          <th className="font-normal py-2 pr-4">Område</th>
          <th className="font-normal py-2 pr-4 text-right">Kunder</th>
          <th className="font-normal py-2 pr-4">Startade</th>
          <th className="font-normal py-2">Beräknat klart</th>
        </tr>
      </thead>
      <tbody>
        {outages.map((o) => (
          <tr key={o.id} className="border-b border-[var(--line)]/60 hover:bg-[var(--panel)]">
            <td className="py-2.5 pr-4">
              <StatusDot status={o.status} />
            </td>
            <td className="py-2.5 pr-4 text-[var(--text)]">{providerName(o.provider)}</td>
            <td className="py-2.5 pr-4 text-[var(--text)]">
              {o.area_label}
              <span className="block text-xs text-[var(--muted)]">
                {STATUS_LABELS[o.status]}
                {o.lat != null && o.lng != null && (
                  <span className="font-mono">
                    {" "}
                    · {o.lat.toFixed(4)}, {o.lng.toFixed(4)}
                  </span>
                )}
              </span>
            </td>
            <td className="py-2.5 pr-4 text-right font-mono">
              {o.affected_customers != null ? o.affected_customers.toLocaleString("sv-SE") : "—"}
            </td>
            <td className="py-2.5 pr-4 font-mono text-[var(--muted)]">{formatTime(o.started_at)}</td>
            <td className="py-2.5 font-mono text-[var(--muted)]">{formatTime(o.estimated_end_at)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function Dashboard({ outages, resolved }: { outages: Outage[]; resolved: Outage[] }) {
  const [visible, setVisible] = useState<Set<FilterableStatus>>(new Set(FILTERABLE_STATUSES));
  const [query, setQuery] = useState("");

  function toggle(status: FilterableStatus) {
    setVisible((prev) => {
      const next = new Set(prev);
      if (next.has(status)) {
        next.delete(status);
      } else {
        next.add(status);
      }
      return next;
    });
  }

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return outages.filter((o) => {
      if (!visible.has(o.status as FilterableStatus)) return false;
      if (!q) return true;
      return o.area_label.toLowerCase().includes(q) || providerName(o.provider).toLowerCase().includes(q);
    });
  }, [outages, visible, query]);

  // Current means already affecting someone - fault (unplanned) or planned
  // work that has actually started. Upcoming (not yet started) is a
  // distinct, separate thing: nobody's power is out for it yet, so it
  // gets its own section rather than being mixed into "current".
  const current = useMemo(() => filtered.filter((o) => o.status !== "upcoming"), [filtered]);
  const upcoming = useMemo(() => filtered.filter((o) => o.status === "upcoming"), [filtered]);

  const located = filtered.filter((o) => o.lat != null && o.lng != null);

  const currentlyWithoutPower = current.reduce((sum, o) => sum + (o.affected_customers ?? 0), 0);
  const upcomingCustomers = upcoming.reduce((sum, o) => sum + (o.affected_customers ?? 0), 0);

  const providerSummary = useMemo(() => {
    const byProvider = new Map<string, { active_count: number; total_customers: number }>();
    for (const o of current) {
      const entry = byProvider.get(o.provider) ?? { active_count: 0, total_customers: 0 };
      entry.active_count += 1;
      entry.total_customers += o.affected_customers ?? 0;
      byProvider.set(o.provider, entry);
    }
    return Array.from(byProvider.entries())
      .map(([provider, v]) => ({ provider, ...v }))
      .sort((a, b) => a.provider.localeCompare(b.provider));
  }, [current]);

  return (
    <>
      <header className="flex items-end justify-between pt-10 pb-6 border-b border-[var(--line)]">
        <div>
          <h1 className="text-[15px] font-medium text-[var(--text)]">POMS2</h1>
          <p className="text-sm text-[var(--muted)] mt-0.5">Driftläge elnät, Sverige</p>
        </div>
        <div className="text-right">
          <div
            className="font-mono text-4xl leading-none"
            style={{ color: currentlyWithoutPower > 0 ? "var(--fault)" : "var(--text)" }}
          >
            {currentlyWithoutPower.toLocaleString("sv-SE")}
          </div>
          <p className="text-sm text-[var(--muted)] mt-1">kunder utan ström just nu</p>
        </div>
      </header>

      <section className="flex items-center gap-2 py-4 border-b border-[var(--line)] flex-wrap">
        <span className="text-sm text-[var(--muted)] mr-2">Visa</span>
        {FILTERABLE_STATUSES.map((status) => {
          const active = visible.has(status);
          return (
            <button
              key={status}
              onClick={() => toggle(status)}
              className="flex items-center gap-2 px-3 py-1.5 text-sm border rounded-sm transition-colors"
              style={{
                borderColor: active ? "var(--line)" : "transparent",
                color: active ? "var(--text)" : "var(--muted)",
                backgroundColor: active ? "var(--panel)" : "transparent",
              }}
              aria-pressed={active}
            >
              <StatusDot status={status} />
              {STATUS_LABELS[status]}
            </button>
          );
        })}
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Sök ort eller leverantör…"
          className="ml-auto px-3 py-1.5 text-sm bg-[var(--panel)] border border-[var(--line)] text-[var(--text)] placeholder:text-[var(--muted)] focus:outline-none focus:border-[var(--upcoming)] w-full sm:w-64"
        />
      </section>

      <section className="flex flex-wrap border-b border-[var(--line)]">
        {providerSummary.length === 0 && (
          <div className="py-4 text-sm text-[var(--muted)]">Inga aktuella avbrott matchar filtret.</div>
        )}
        {providerSummary.map((p) => (
          <div
            key={p.provider}
            className="py-4 pr-8 mr-8 border-r border-[var(--line)] last:border-r-0 last:mr-0 last:pr-0"
          >
            <div className="text-sm text-[var(--text)]">{providerName(p.provider)}</div>
            <div className="font-mono text-xl mt-1">{p.active_count}</div>
            <div className="text-xs text-[var(--muted)]">{p.total_customers.toLocaleString("sv-SE")} kunder</div>
          </div>
        ))}
      </section>

      <section className="py-6 border-b border-[var(--line)]">
        <h2 className="text-sm text-[var(--muted)] mb-3">
          Karta ({located.length} av {filtered.length} har koordinater)
        </h2>
        <OutageMap outages={filtered} />
      </section>

      <section className="py-6 border-b border-[var(--line)]">
        <h2 className="text-sm text-[var(--muted)] mb-3">Aktuella avbrott ({current.length})</h2>
        <OutageTable outages={current} />
      </section>

      {visible.has("upcoming") && (
        <section className="py-6 border-b border-[var(--line)]">
          <h2 className="text-sm text-[var(--muted)] mb-3">
            Kommande avbrott ({upcoming.length})
            {upcomingCustomers > 0 && (
              <span className="ml-2 text-xs">
                · {upcomingCustomers.toLocaleString("sv-SE")} kunder berörs när de börjar
              </span>
            )}
          </h2>
          <OutageTable outages={upcoming} />
        </section>
      )}

      {resolved.length > 0 && (
        <section className="pb-10">
          <h2 className="text-sm text-[var(--muted)] mb-3">Senast åtgärdade</h2>
          <ul className="text-sm divide-y divide-[var(--line)]/60">
            {resolved.map((o) => (
              <li key={o.id} className="py-2 flex justify-between text-[var(--muted)]">
                <span>
                  {providerName(o.provider)} — {o.area_label}
                </span>
                <span className="font-mono">{formatTime(o.resolved_at)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}
