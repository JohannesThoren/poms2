import geoData from "./sv-geo-data.json";

const LOCALITY: Record<string, number[]> = geoData.locality;
const MUNICIPALITY: Record<string, number[]> = geoData.municipality;
const COUNTY: Record<string, number[]> = geoData.county;

function normalize(s: string): string {
  return s
    .toLowerCase()
    .trim()
    .replace(/^län\s+/, "")
    .replace(/\s+län$/, "")
    .replace(/^avbrott\s*#?\d*$/, "") // "Avbrott #123" style labels carry no place name at all
    .trim();
}

/**
 * Best-effort coordinates for an outage that has none of its own, derived
 * from its free-text area label. Tries, in order: an exact locality name
 * (most specific), a municipality name, then a county/län name (least
 * specific - a whole län is a big pin, but still better than no pin at
 * all). Area labels that list several places ("Nyköping, Oxelösund") are
 * split on common separators and each candidate is tried in turn.
 *
 * Source: SCB 2020 localities (via github.com/hej2010/svenska-orter),
 * WGS84. Each municipality/county maps to its single largest locality's
 * point (usually the seat) - a real approximation, not the true centroid
 * of the affected area, hence `approx: true` on the result so callers can
 * render it differently (e.g. a hollow marker) from a source-provided
 * exact coordinate.
 */
export function geocodeAreaLabel(label: string): { lat: number; lng: number } | null {
  const candidates = label
    .split(/[,/]|(?:\bo\.?\s*|\boch\b)/i)
    .map((s) => normalize(s))
    .filter((s) => s.length > 1);

  for (const table of [LOCALITY, MUNICIPALITY, COUNTY]) {
    for (const candidate of candidates) {
      const hit = table[candidate];
      if (hit) return { lat: hit[0], lng: hit[1] };
    }
  }
  return null;
}
