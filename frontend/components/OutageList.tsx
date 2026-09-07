"use client";

import { useMemo, useState } from "react";
import type { Outage } from "@/lib/db";
import { providerName, STATUS_LABELS, formatTime } from "@/lib/format";

export type SortKey = "provider" | "area_label" | "affected_customers" | "started_at" | "estimated_end_at";
type SortDir = "asc" | "desc";

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
      className={`inline-block h-2 w-2 rounded-full shrink-0 ${status === "fault" ? "pulse" : ""}`}
      style={{ backgroundColor: color }}
    />
  );
}

function sortValue(o: Outage, key: SortKey): string | number {
  switch (key) {
    case "provider":
      return providerName(o.provider);
    case "area_label":
      return o.area_label;
    case "affected_customers":
      return o.affected_customers ?? -1;
    case "started_at":
      return o.started_at ?? "";
    case "estimated_end_at":
      return o.estimated_end_at ?? "";
  }
}

function useSortedOutages(outages: Outage[], defaultKey: SortKey, defaultDir: SortDir) {
  const [sortKey, setSortKey] = useState<SortKey>(defaultKey);
  const [sortDir, setSortDir] = useState<SortDir>(defaultDir);

  function toggleSort(key: SortKey) {
    if (key === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      // Numbers/dates default to biggest/most-recent first; text defaults A→Ö.
      setSortDir(key === "area_label" || key === "provider" ? "asc" : "desc");
    }
  }

  const sorted = useMemo(() => {
    const copy = [...outages];
    copy.sort((a, b) => {
      const av = sortValue(a, sortKey);
      const bv = sortValue(b, sortKey);
      const cmp = typeof av === "number" && typeof bv === "number" ? av - bv : String(av).localeCompare(String(bv), "sv");
      return sortDir === "asc" ? cmp : -cmp;
    });
    return copy;
  }, [outages, sortKey, sortDir]);

  return { sorted, sortKey, sortDir, toggleSort };
}

function SortArrow({ active, dir }: { active: boolean; dir: SortDir }) {
  if (!active) return null;
  return <span className="ml-1 text-[10px] align-middle">{dir === "asc" ? "▲" : "▼"}</span>;
}

/**
 * Full table used on the main dashboard - all columns, comfortable
 * row height.
 */
export function OutageTable({
  outages,
  selectedId,
  onSelect,
}: {
  outages: Outage[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
}) {
  const { sorted, sortKey, sortDir, toggleSort } = useSortedOutages(outages, "affected_customers", "desc");

  if (outages.length === 0) {
    return <p className="text-sm text-[var(--muted)] py-8">Inga avbrott matchar det valda filtret.</p>;
  }

  const th = (key: SortKey, label: string, align: "left" | "right" = "left") => (
    <th
      className={`font-normal py-2 pr-4 cursor-pointer select-none hover:text-[var(--text)] ${align === "right" ? "text-right" : "text-left"}`}
      onClick={() => toggleSort(key)}
      aria-sort={sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
    >
      {label}
      <SortArrow active={sortKey === key} dir={sortDir} />
    </th>
  );

  return (
    <table className="w-full text-sm border-collapse">
      <thead>
        <tr className="text-left text-[var(--muted)] border-b border-[var(--line)]">
          <th className="font-normal py-2 pr-4 w-8"></th>
          {th("provider", "Leverantör")}
          {th("area_label", "Område")}
          {th("affected_customers", "Kunder", "right")}
          {th("started_at", "Startade")}
          {th("estimated_end_at", "Beräknat klart")}
        </tr>
      </thead>
      <tbody>
        {sorted.map((o) => (
          <tr
            key={o.id}
            onClick={() => onSelect?.(o.id)}
            className="border-b border-[var(--line)]/60 hover:bg-[var(--panel)] cursor-pointer transition-colors"
            style={selectedId === o.id ? { backgroundColor: "var(--panel)", boxShadow: "inset 2px 0 0 var(--upcoming)" } : undefined}
          >
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

/**
 * Narrow single-column list used in the NOC sidebar. Still sortable (via
 * a small header row of sort buttons instead of table columns, since
 * there's no room for a full header row) and clickable, kept in sync
 * with the map via `selectedId`/`onSelect`.
 */
export function OutageSidebarList({
  outages,
  selectedId,
  onSelect,
}: {
  outages: Outage[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
}) {
  const { sorted, sortKey, sortDir, toggleSort } = useSortedOutages(outages, "affected_customers", "desc");

  const sortButton = (key: SortKey, label: string) => (
    <button
      onClick={() => toggleSort(key)}
      className="text-xs px-2 py-1 rounded-sm border transition-colors"
      style={{
        borderColor: sortKey === key ? "var(--line)" : "transparent",
        color: sortKey === key ? "var(--text)" : "var(--muted)",
        backgroundColor: sortKey === key ? "var(--panel)" : "transparent",
      }}
    >
      {label}
      <SortArrow active={sortKey === key} dir={sortDir} />
    </button>
  );

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-1 px-3 py-2 border-b border-[var(--line)] shrink-0 flex-wrap">
        {sortButton("affected_customers", "Kunder")}
        {sortButton("started_at", "Tid")}
        {sortButton("provider", "Bolag")}
        {sortButton("area_label", "Område")}
      </div>
      <div className="flex-1 overflow-y-auto">
        {sorted.length === 0 && <p className="text-sm text-[var(--muted)] p-3">Inga aktiva avbrott.</p>}
        <ul className="divide-y divide-[var(--line)]/60">
          {sorted.map((o) => (
            <li
              key={o.id}
              onClick={() => onSelect?.(o.id)}
              className="px-3 py-2.5 cursor-pointer hover:bg-[var(--panel)] transition-colors text-sm"
              style={selectedId === o.id ? { backgroundColor: "var(--panel)", boxShadow: "inset 2px 0 0 var(--upcoming)" } : undefined}
            >
              <div className="flex items-center gap-2">
                <StatusDot status={o.status} />
                <span className="text-[var(--text)] truncate">{providerName(o.provider)}</span>
                <span className="ml-auto font-mono text-xs text-[var(--muted)]">
                  {o.affected_customers != null ? o.affected_customers.toLocaleString("sv-SE") : "—"}
                </span>
              </div>
              <div className="text-xs text-[var(--muted)] mt-0.5 pl-4 truncate">
                {o.area_label} · {formatTime(o.started_at)}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
