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

// Below this zoom, individual area outlines are too small to read and just
// clutter the overview - only draw them once someone's zoomed in close
// enough that a polygon actually adds detail over the point marker.
const POLYGON_MIN_ZOOM = 9;

export function OutageMap({
  outages,
  selectedId,
  onSelectId,
  className,
}: {
  outages: Outage[];
  selectedId?: string | null;
  onSelectId?: (id: string) => void;
  className?: string;
}) {
  const mapDivRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null);
  const markersRef = useRef<L.LayerGroup | null>(null);
  const polygonsRef = useRef<L.LayerGroup | null>(null);
  const markerById = useRef<Map<string, L.CircleMarker>>(new Map());
  // Kept in sync with the `outages`/`onSelectId` props so the zoomend and
  // marker click handlers (attached once, outside React's render cycle)
  // always see the current values without re-subscribing every render.
  const outagesRef = useRef<Outage[]>(outages);
  const onSelectIdRef = useRef(onSelectId);
  useEffect(() => {
    outagesRef.current = outages;
    onSelectIdRef.current = onSelectId;
  }, [outages, onSelectId]);

  const located = outages.filter((o) => o.lat != null && o.lng != null);

  function redrawPolygons(Lmod: typeof L) {
    const group = polygonsRef.current;
    const map = mapRef.current;
    if (!group || !map) return;
    group.clearLayers();

    if (map.getZoom() < POLYGON_MIN_ZOOM) return;

    for (const o of outagesRef.current) {
      if (!o.polygon || o.polygon.length < 3) continue;
      const color = STATUS_COLOR[o.status] ?? STATUS_COLOR.resolved;
      Lmod.polygon(o.polygon, {
        color,
        weight: 1.5,
        fillColor: color,
        fillOpacity: 0.2,
      }).addTo(group);
    }
  }

  // Init map once.
  useEffect(() => {
    let cancelled = false;

    import("leaflet").then((Lmod) => {
      if (cancelled || !mapDivRef.current || mapRef.current) return;

      const map = Lmod.map(mapDivRef.current, {
        center: [62.5, 16.5], // roughly the middle of Sweden
        zoom: 5,
        zoomControl: true,
        attributionControl: true,
      });

      Lmod
        .tileLayer("https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}", {
          attribution:
            'Tiles &copy; <a href="https://www.esri.com">Esri</a> &mdash; Esri, DeLorme, NAVTEQ',
          maxZoom: 16,
        })
        .addTo(map);

      // Reference overlay: place names, borders, roads - the base layer
      // alone is just shaded terrain with no labels at all.
      Lmod
        .tileLayer("https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Reference/MapServer/tile/{z}/{y}/{x}", {
          maxZoom: 16,
          pane: "overlayPane",
        })
        .addTo(map);

      mapRef.current = map;
      polygonsRef.current = Lmod.layerGroup().addTo(map);
      markersRef.current = Lmod.layerGroup().addTo(map);

      map.on("zoomend", () => redrawPolygons(Lmod));

      // In a flex/grid layout (the NOC view) the container may not have
      // its final height yet on the tick Leaflet measures it, which
      // leaves the map rendered too small until the next resize.
      const invalidate = () => map.invalidateSize();
      window.addEventListener("resize", invalidate);
      const t = setTimeout(invalidate, 0);
      (map as unknown as { _cleanupInvalidate?: () => void })._cleanupInvalidate = () => {
        window.removeEventListener("resize", invalidate);
        clearTimeout(t);
      };
    });

    return () => {
      cancelled = true;
      (mapRef.current as unknown as { _cleanupInvalidate?: () => void } | null)?._cleanupInvalidate?.();
      mapRef.current?.remove();
      mapRef.current = null;
    };
  }, []);

  // Redraw markers + polygons whenever the (filtered) outage list changes.
  useEffect(() => {
    let cancelled = false;

    import("leaflet").then((Lmod) => {
      if (cancelled || !markersRef.current) return;
      markersRef.current.clearLayers();
      markerById.current.clear();

      for (const o of located) {
        const color = STATUS_COLOR[o.status] ?? STATUS_COLOR.resolved;
        const isSelected = o.id === selectedId;
        const marker = Lmod.circleMarker([o.lat as number, o.lng as number], {
          radius: isSelected ? 10 : 7,
          color: isSelected ? "#ffffff" : color,
          fillColor: color,
          // Approximate (geocoded from area name, not a real point from
          // the source) markers are shown hollow so it's clear at a
          // glance they're not precise.
          fillOpacity: o.approx ? 0.15 : 0.85,
          weight: isSelected ? 3 : o.approx ? 2 : 1.5,
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

        marker.on("click", () => onSelectIdRef.current?.(o.id));

        marker.addTo(markersRef.current!);
        markerById.current.set(o.id, marker);
      }

      redrawPolygons(Lmod);
    });

    return () => {
      cancelled = true;
    };
  }, [located, selectedId]);

  // When something is selected from OUTSIDE the map (e.g. a click in the
  // outage list), pan/zoom to it and pop its marker open - this is what
  // makes the list and the map feel like one linked view instead of two
  // independent ones.
  useEffect(() => {
    if (!selectedId) return;
    const map = mapRef.current;
    const marker = markerById.current.get(selectedId);
    if (!map || !marker) return;
    const latLng = marker.getLatLng();
    const targetZoom = Math.max(map.getZoom(), 10);
    map.flyTo(latLng, targetZoom, { duration: 0.6 });
    marker.openPopup();
  }, [selectedId]);

  return (
    <div className="relative h-full">
      <div ref={mapDivRef} className={className ?? "h-[420px] w-full rounded-none border border-[var(--line)]"} />
      {located.length === 0 && (
        <div className="absolute inset-0 flex items-center justify-center text-sm text-[var(--muted)] bg-[var(--bg)]/60 pointer-events-none">
          Ingen av de filtrerade händelserna har koordinater
        </div>
      )}
    </div>
  );
}
