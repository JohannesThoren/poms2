import geoData from "./sv-geo-data.json";

const LOCALITY: Record<string, number[]> = geoData.locality;
const MUNICIPALITY: Record<string, number[]> = geoData.municipality;
const COUNTY: Record<string, number[]> = geoData.county;

/**
 * For providers whose whole coverage area is one small kommun, an outage
 * with an ungeocodable free-text label (a tiny local place not in our
 * locality table, or a real place name shared with a same-named but
 * unrelated town elsewhere in Sweden - "Hede" exists both near
 * Herrljunga and, much more prominently, in Härjedalen 500km north) is
 * still safely anchored to the right *kommun* by just using the
 * provider's own home municipality - far better than guessing a
 * same-named place in the wrong part of the country, and used before
 * that riskier single-word fallback below, not after.
 */
const PROVIDER_HOMETOWN: Record<string, string> = {
  herrljunga: "herrljunga",
  eksjo: "eksjö",
  pite: "piteå",
  karlshamn: "karlshamn",
  harryda: "härryda",
  btea: "berg",
  hemab: "härnösand",
  gotland: "gotland",
  norrtalje: "norrtälje",
  falbygden: "falköping",
  sandviken: "sandviken",
  vanerenergi: "vänersborg",
  kungalv: "kungälv",
  skurup: "skurup",
  ovikenergi: "örnsköldsvik",
  landskrona: "landskrona",
};

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
 * Some sources (e.g. Herrljunga Elektriska) give a whole free-text
 * sentence instead of a clean place list - "Strömavbrott och fiberfel
 * Hede samt Fröstorp med omnejd" - so beyond the simple comma/"och"
 * split, this also splits on more connector words/phrases ("samt",
 * "med omnejd", "korsningen", "vid", "i", ...) and, if that still finds
 * nothing, falls back to trying each individual capitalized word in the
 * label against the locality table alone (the most specific table, so a
 * false-positive word match is at least a real, small place rather than
 * an entire municipality or län).
 *
 * Source: SCB 2020 localities (via github.com/hej2010/svenska-orter),
 * WGS84. Each municipality/county maps to its single largest locality's
 * point (usually the seat) - a real approximation, not the true centroid
 * of the affected area, hence `approx: true` on the result so callers can
 * render it differently (e.g. a hollow marker) from a source-provided
 * exact coordinate.
 */
export function geocodeAreaLabel(label: string, provider?: string): { lat: number; lng: number } | null {
  const candidates = label
    .split(/[,/–-]|\bsamt\b|\bmed omnejd\b|\bkorsningen\b|\bkring\b|\brunt\b|\bvid\b|\bnära\b|\b[io]\s|(?:\bo\.?\s*|\boch\b)/i)
    .map((s) => normalize(s))
    .filter((s) => s.length > 1);

  for (const table of [LOCALITY, MUNICIPALITY, COUNTY]) {
    for (const candidate of candidates) {
      const hit = table[candidate];
      if (hit) return { lat: hit[0], lng: hit[1] };
    }
  }

  // Safer than guessing from an isolated word below - anchors to the
  // right kommun using the provider's own coverage area, before ever
  // risking a same-named-but-wrong-region match.
  const hometown = provider ? PROVIDER_HOMETOWN[provider] : undefined;
  if (hometown && MUNICIPALITY[hometown]) {
    const hit = MUNICIPALITY[hometown];
    return { lat: hit[0], lng: hit[1] };
  }

  // Last resort: a free-text sentence with no clean delimiters around its
  // place name(s) at all - try each capitalized word on its own against
  // the locality table only.
  const words = label.match(/\b[A-ZÅÄÖ][a-zåäöA-ZÅÄÖ]+\b/g) ?? [];
  for (const word of words) {
    const hit = LOCALITY[normalize(word)];
    if (hit) return { lat: hit[0], lng: hit[1] };
  }

  return null;
}
