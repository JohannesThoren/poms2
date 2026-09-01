//! Kommuner Ellevio's public outage map covers, one page per kommun at
//! `avbrottskarta.ellevio.se/kommun/<slug>/idag`.
//!
//! Ellevio's own network spans Stockholm, Uppsala, Värmland and Halland
//! län, but not every kommun *within* those län is necessarily on
//! Ellevio's net (some pockets belong to smaller local nätägare). This
//! list is a first pass covering the kommuner known to be Ellevio
//! territory - treat gaps or wrong slugs as expected until verified
//! against real traffic; that's a quick fix (one entry) rather than a
//! redesign.

pub struct Kommun {
    pub name: &'static str,
    pub slug: &'static str,
}

pub const KOMMUNER: &[Kommun] = &[
    // Stockholms län
    Kommun { name: "Botkyrka", slug: "botkyrka" },
    Kommun { name: "Danderyd", slug: "danderyd" },
    Kommun { name: "Ekerö", slug: "ekero" },
    Kommun { name: "Haninge", slug: "haninge" },
    Kommun { name: "Huddinge", slug: "huddinge" },
    Kommun { name: "Järfälla", slug: "jarfalla" },
    Kommun { name: "Lidingö", slug: "lidingo" },
    Kommun { name: "Nacka", slug: "nacka" },
    Kommun { name: "Nykvarn", slug: "nykvarn" },
    Kommun { name: "Nynäshamn", slug: "nynashamn" },
    Kommun { name: "Salem", slug: "salem" },
    Kommun { name: "Sollentuna", slug: "sollentuna" },
    Kommun { name: "Solna", slug: "solna" },
    Kommun { name: "Stockholm", slug: "stockholm" },
    Kommun { name: "Sundbyberg", slug: "sundbyberg" },
    Kommun { name: "Södertälje", slug: "sodertalje" },
    Kommun { name: "Tyresö", slug: "tyreso" },
    Kommun { name: "Täby", slug: "taby" },
    Kommun { name: "Upplands-Bro", slug: "upplands-bro" },
    Kommun { name: "Upplands Väsby", slug: "upplands-vasby" },
    Kommun { name: "Vallentuna", slug: "vallentuna" },
    Kommun { name: "Vaxholm", slug: "vaxholm" },
    Kommun { name: "Värmdö", slug: "varmdo" },
    Kommun { name: "Österåker", slug: "osteraker" },
    // Uppsala län
    Kommun { name: "Enköping", slug: "enkoping" },
    Kommun { name: "Håbo", slug: "habo" },
    Kommun { name: "Knivsta", slug: "knivsta" },
    Kommun { name: "Uppsala", slug: "uppsala" },
    Kommun { name: "Älvkarleby", slug: "alvkarleby" },
    // Värmlands län
    Kommun { name: "Arvika", slug: "arvika" },
    Kommun { name: "Eda", slug: "eda" },
    Kommun { name: "Filipstad", slug: "filipstad" },
    Kommun { name: "Forshaga", slug: "forshaga" },
    Kommun { name: "Grums", slug: "grums" },
    Kommun { name: "Hagfors", slug: "hagfors" },
    Kommun { name: "Hammarö", slug: "hammaro" },
    Kommun { name: "Karlstad", slug: "karlstad" },
    Kommun { name: "Kil", slug: "kil" },
    Kommun { name: "Kristinehamn", slug: "kristinehamn" },
    Kommun { name: "Munkfors", slug: "munkfors" },
    Kommun { name: "Storfors", slug: "storfors" },
    Kommun { name: "Sunne", slug: "sunne" },
    Kommun { name: "Säffle", slug: "saffle" },
    Kommun { name: "Torsby", slug: "torsby" },
    Kommun { name: "Årjäng", slug: "arjang" },
    // Hallands län
    Kommun { name: "Falkenberg", slug: "falkenberg" },
    Kommun { name: "Halmstad", slug: "halmstad" },
    Kommun { name: "Hylte", slug: "hylte" },
    Kommun { name: "Kungsbacka", slug: "kungsbacka" },
    Kommun { name: "Laholm", slug: "laholm" },
    Kommun { name: "Varberg", slug: "varberg" },
];
