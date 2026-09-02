export const PROVIDER_NAMES: Record<string, string> = {
  ellevio: "Ellevio",
  vattenfall: "Vattenfall",
  kraftringen: "Kraftringen",
  jamtkraft: "Jämtkraft",
  tekniska_verken: "Tekniska verken",
  oresundskraft: "Öresundskraft",
  vaxjo: "Växjö Energi",
  lerum: "Lerum Energi",
  vasterbergslagens: "Västerbergslagens Elnät",
  partille: "Partille Energi",
};

export function providerName(provider: string): string {
  return PROVIDER_NAMES[provider] ?? provider;
}

export const STATUS_LABELS: Record<string, string> = {
  fault: "Pågående fel",
  planned: "Planerat avbrott",
  upcoming: "Kommande",
  resolved: "Åtgärdat",
};

export const STATUS_STYLES: Record<string, string> = {
  fault: "bg-red-500/15 text-red-400 border-red-500/30",
  planned: "bg-amber-500/15 text-amber-400 border-amber-500/30",
  upcoming: "bg-sky-500/15 text-sky-400 border-sky-500/30",
  resolved: "bg-zinc-500/15 text-zinc-400 border-zinc-500/30",
};

export function formatTime(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("sv-SE", {
    dateStyle: "short",
    timeStyle: "short",
  });
}
