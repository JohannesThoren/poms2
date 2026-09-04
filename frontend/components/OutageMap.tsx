"use client";

import { useEffect, useRef } from "react";
import type L from "leaflet";
import type { Outage } from "@/lib/db";
import { providerName, STATUS_LABELS, formatTime } from "@/lib/format";

const STATUS_COLOR: Record<string, string> = {
  fault: "#e5484d",
  planned: "#f0a500",
  upcoming: "#4c9fe8",
  resolved: "#5b6b7a",
};

export function OutageMap({ outages }: { outages: Outage[] }) {
  const mapDivRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null);
  const markersRef = useRef<L.LayerGroup | null>(null);

  const located = outages.filter((o) => o.lat != null && o.lng != null);

  // Init map once.
  useEffect(() => {
    let cancelled = false;

    import("leaflet").then((L) => {
      if (cancelled || !mapDivRef.current || mapRef.current) return;

      const map = L.map(mapDivRef.current, {
        center: [62.5, 16.5], // roughly the middle of Sweden
        zoom: 5,
        zoomControl: true,
        attributionControl: true,
      });

      L.tileLayer(
        "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}",
        {
          attribution:
            'Tiles &copy; <a href="https://www.esri.com">Esri</a> &mdash; Esri, DeLorme, NAVTEQ',
          maxZoom: 16,
        }
      ).addTo(map);

      // Reference overlay: place names, borders, roads - the base layer
      // alone is just shaded terrain with no labels at all.
      L.tileLayer(
        "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Reference/MapServer/tile/{z}/{y}/{x}",
        {
          maxZoom: 16,
          pane: "overlayPane",
        }
      ).addTo(map);

      mapRef.current = map;
      markersRef.current = L.layerGroup().addTo(map);
    });

    return () => {
      cancelled = true;
      mapRef.current?.remove();
      mapRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Redraw markers whenever the (filtered) outage list changes.
  useEffect(() => {
    let cancelled = false;

    import("leaflet").then((L) => {
      if (cancelled || !markersRef.current) return;
      markersRef.current.clearLayers();

      for (const o of located) {
        const color = STATUS_COLOR[o.status] ?? STATUS_COLOR.resolved;
        const marker = L.circleMarker([o.lat as number, o.lng as number], {
          radius: 7,
          color,
          fillColor: color,
          // Approximate (geocoded from area name, not a real point from
          // the source) markers are shown hollow so it's clear at a
          // glance they're not precise.
          fillOpacity: o.approx ? 0.15 : 0.85,
          weight: o.approx ? 2 : 1.5,
          dashArray: o.approx ? "3,3" : undefined,
        });

        const customers =
          o.affected_customers != null ? `${o.affected_customers.toLocaleString("sv-SE")} kunder` : "";
        const approxNote = o.approx ? `<br/><span style="color:#888;font-size:11px">Ungefärlig position (ortnamn)</span>` : "";

        marker.bindPopup(
          `<div style="font-family:sans-serif;font-size:13px;min-width:160px">` +
            `<strong>${providerName(o.provider)}</strong><br/>` +
            `${o.area_label}<br/>` +
            `<span style="color:${color}">${STATUS_LABELS[o.status] ?? o.status}</span>` +
            (customers ? ` &middot; ${customers}` : "") +
            `<br/><span style="color:#888">Startade ${formatTime(o.started_at)}</span>` +
            approxNote +
            `</div>`
        );

        marker.addTo(markersRef.current!);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [located]);

  return (
    <div className="relative">
      <div ref={mapDivRef} className="h-[420px] w-full rounded-none border border-[var(--line)]" />
      {located.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center text-sm text-[var(--muted)] bg-[var(--bg)]/60 pointer-events-none">
          Ingen av de filtrerade händelserna har koordinater
        </div>
      )}
    </div>
  );
}
