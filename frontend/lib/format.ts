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
  linde: "Linde Energi",
  gavle: "Gävle Energi",
  skekraft: "Skellefteå Kraft",
  karlstad: "Karlstads El",
  eskilstuna_strangnas: "Eskilstuna Strängnäs Energi",
  tranas: "Tranås Energi",
  uddevalla: "Uddevalla Energi",
  telge: "Telge Nät",
  malarenergi: "Mälarenergi",
  upplands_energi: "Upplands Energi",
  voe: "Västra Orusts Energitjänst",
  hoganas: "Höganäs Energi",
  eksjo: "Eksjö Energi",
  pite: "PiteEnergi",
  harjeans: "Härjeåns Nät",
  karlshamn: "Karlshamn Energi",
  umea: "Umeå Energi",
  gotland: "Gotlands Energi",
  herrljunga: "Herrljunga Elektriska",
  falbygden: "Falbygdens Energi",
  halmstad: "HEM (Halmstad)",
  hemab: "HEMAB (Härnösand)",
  btea: "Bergs Tingslags Elektriska",
  harryda: "Härryda Energi",
};

export function providerName(provider: string): string {
  return PROVIDER_NAMES[provider] ?? provider;
}

/** The utility's own public outage-map page, for a "källa"/source link -
 * lets anyone verify a number against where it actually came from,
 * without us claiming to be the source of truth ourselves. Points at the
 * human-facing page where one exists; falls back to the raw data endpoint
 * for the handful of sources with no separate human page. */
export const PROVIDER_SOURCE_URL: Record<string, string> = {
  ellevio: "https://avbrottskarta.ellevio.se",
  vattenfall: "https://www.vattenfalleldistribution.se/stromavbrott/pagaende-stromavbrott/",
  kraftringen: "https://avbrott.kraftringen.se",
  jamtkraft: "https://avbrottskarta.jamtkraft.se",
  tekniska_verken: "https://www.tekniskaverken.se/avbrott/",
  oresundskraft: "https://www.oresundskraft.se/avbrottsinformation/",
  vaxjo: "https://www.veab.se/driftinformation/",
  lerum: "https://www.lerumenergi.se/Avbrott",
  vasterbergslagens: "https://status.vbenergi.se/outagemap2/?cust=vbe&app=fpp",
  partille: "https://avbrottskarta.partilleenergi.se/outagemap2/?cust=pen&app=fpp",
  linde: "https://webmap.lindeenergi.se/outagemap2/?cust=lie&app=fpp",
  gavle: "https://avbrottskartan.gavleenergi.se",
  skekraft: "https://driftinfo.skekraft.se",
  karlstad: "https://www.karlstadselnat.se/driftinformation/",
  eskilstuna_strangnas: "https://www.eem.se/privat/driftinformation/",
  tranas: "https://www.tranasenergi.se/kundservice/driftinformation/",
  uddevalla: "https://uddevallaenergi.se/kundservice/driftinformation.html",
  telge: "https://www.telge.se/nat/avbrott/",
  malarenergi: "https://www.malarenergi.se/avbrott/",
  upplands_energi: "https://www.upplandsenergi.se/elavbrott",
  voe: "https://voe.se",
  hoganas: "https://www.hoganasenergi.se/driftinformation",
  eksjo: "https://eksjoenergi.se/driftinformation/",
  pite: "https://www.piteenergi.se/driftinformation/",
  harjeans: "https://avbrottskarta.harjeans.se",
  karlshamn: "https://www.karlshamnenergi.se/driftsinformation/",
  umea: "https://driftinfo.umeaenergi.se/",
  gotland: "https://experience.arcgis.com/experience/a58b3611518c4a1a93370b3d9035ad02/",
  herrljunga: "https://www.el.herrljunga.se/storning/",
  falbygden: "https://falbygdensenergi.se/driftinformation",
  halmstad: "https://www.hem.se/avbrottsinformation",
  hemab: "https://www.hemab.se/driftinformation.4.60121ec2161036d48346764e.html",
  btea: "https://www.btea.se/se-aktuella-avbrott",
  harryda: "https://www.harrydaenergi.se/drift/",
};

export function providerSourceUrl(provider: string): string | undefined {
  return PROVIDER_SOURCE_URL[provider];
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
