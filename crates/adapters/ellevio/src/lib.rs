mod kommuner;

use async_trait::async_trait;
use chrono::Utc;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{Html, Selector};
use std::collections::HashMap;

pub use kommuner::KOMMUNER;

const BASE_URL: &str = "https://avbrottskarta.ellevio.se";

/// Ellevio redesigned this site (2026-09): the per-kommun page used to be
/// server-rendered with the customer count directly in a fixed spot. Now
/// most kommun slugs don't resolve to their own page at all - they fall
/// back to a generic nationwide snapshot (`<h2>Aktuella strömavbrott just
/// nu</h2>` + a per-län table titled "Kunder berörda av strömavbrott just
/// nu" once hydrated). That table's two numbers - "Oplanerade" and
/// "Planerade" - are customer counts split by outage type, not incident
/// counts (confirmed: a single-kommun page like Kungsbacka's "37
/// oplanerade" exactly matches Hallands län's "37" in the nationwide
/// table, which only makes sense if both are customer counts and
/// Kungsbacka is the only affected place in that län right now).
///
/// This adapter handles both page shapes: kommun-specific pages are read
/// as before, and any kommun whose slug no longer resolves contributes to
/// a single set of per-län events instead - coarser than a kommun, but
/// far better than silently losing that whole region's data.
#[derive(Debug, PartialEq)]
enum PageResult {
    /// This kommun's own page rendered correctly.
    KommunSpecific { oplanerade: i32, planerade: i32 },
    /// The slug wasn't recognized - Ellevio served the generic nationwide
    /// per-län table instead.
    NationwideFallback(Vec<(String, i32, i32)>),
    /// Neither shape was found - the page changed again in some other way.
    Unrecognized,
}

fn extract_leading_number(s: &str) -> Option<i32> {
    let digits: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn parse_page(html: &str) -> PageResult {
    let document = Html::parse_document(html);
    let h2_sel = Selector::parse("#prerendered h2").unwrap();

    let Some(h2) = document.select(&h2_sel).next() else {
        return PageResult::Unrecognized;
    };
    let h2_text: String = h2.text().collect();

    if h2_text.contains("Aktuella strömavbrott i") {
        let container_sel = Selector::parse("#prerendered").unwrap();
        let Some(container) = document.select(&container_sel).next() else {
            return PageResult::Unrecognized;
        };
        let full_text: String = container.text().collect::<Vec<_>>().join(" ");

        if full_text.contains("Inga kunder berörda") {
            return PageResult::KommunSpecific { oplanerade: 0, planerade: 0 };
        }

        let div_sel = Selector::parse("#prerendered > div").unwrap();
        let mut oplanerade = 0;
        let mut planerade = 0;
        for div in document.select(&div_sel) {
            let text: String = div.text().collect();
            if text.contains("oplanerade") {
                oplanerade = extract_leading_number(&text).unwrap_or(0);
            } else if text.contains("planerade") {
                planerade = extract_leading_number(&text).unwrap_or(0);
            }
        }
        PageResult::KommunSpecific { oplanerade, planerade }
    } else if h2_text.contains("Aktuella strömavbrott just nu") {
        let row_sel = Selector::parse("table tbody tr").unwrap();
        let td_sel = Selector::parse("td").unwrap();
        let mut rows = Vec::new();

        for tr in document.select(&row_sel) {
            let tds: Vec<_> = tr.select(&td_sel).collect();
            if tds.len() != 3 {
                continue;
            }
            let lan: String = tds[0].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let oplanerade = extract_leading_number(&tds[1].text().collect::<String>()).unwrap_or(0);
            let planerade = extract_leading_number(&tds[2].text().collect::<String>()).unwrap_or(0);
            if !lan.is_empty() {
                rows.push((lan, oplanerade, planerade));
            }
        }
        PageResult::NationwideFallback(rows)
    } else {
        PageResult::Unrecognized
    }
}

fn status_for(oplanerade: i32, planerade: i32) -> Option<OutageStatus> {
    if oplanerade > 0 {
        Some(OutageStatus::Fault)
    } else if planerade > 0 {
        Some(OutageStatus::Planned)
    } else {
        None
    }
}

fn to_event(area_label: &str, source_id: &str, oplanerade: i32, planerade: i32) -> Option<RawOutageEvent> {
    let status = status_for(oplanerade, planerade)?;
    // Whichever bucket triggered the status is the customer count that
    // actually applies to it - "oplanerade"/"planerade" are customer
    // counts split by type, not incident counts (see module docs).
    let affected_customers = match status {
        OutageStatus::Fault => oplanerade,
        _ => planerade,
    };
    Some(RawOutageEvent {
        provider: Provider::Ellevio,
        source_id: source_id.to_string(),
        status,
        area_label: area_label.to_string(),
        lat: None,
        lng: None,
        affected_customers: Some(affected_customers),
        reason: None,
        started_at: None,
        estimated_end_at: None,
        observed_at: Utc::now(),
    })
}

pub struct EllevioAdapter {
    client: reqwest::Client,
}

impl EllevioAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn fetch_kommun(&self, kommun: &kommuner::Kommun) -> anyhow::Result<PageResult> {
        let url = format!("{BASE_URL}/kommun/{}/idag", kommun.slug);
        let body = self.client.get(&url).send().await?.text().await?;
        Ok(parse_page(&body))
    }
}

impl Default for EllevioAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for EllevioAdapter {
    fn name(&self) -> &'static str {
        "ellevio"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let mut events = Vec::new();
        // Collapses however many kommuner fall back to the nationwide
        // table into one entry per län - the table is identical on every
        // fallback page, so without this a busy län would otherwise
        // produce dozens of duplicate rows (harmless after upsert, but
        // wasteful).
        let mut lan_fallback: HashMap<String, (i32, i32)> = HashMap::new();

        for kommun in KOMMUNER {
            match self.fetch_kommun(kommun).await {
                Ok(PageResult::KommunSpecific { oplanerade, planerade }) => {
                    if let Some(event) = to_event(kommun.name, &kommun.name.to_lowercase(), oplanerade, planerade) {
                        events.push(event);
                    }
                }
                Ok(PageResult::NationwideFallback(rows)) => {
                    for (lan, oplanerade, planerade) in rows {
                        lan_fallback.insert(lan, (oplanerade, planerade));
                    }
                }
                Ok(PageResult::Unrecognized) => {
                    tracing::warn!(kommun = kommun.name, "unrecognized Ellevio page shape");
                }
                Err(err) => {
                    tracing::warn!(kommun = kommun.name, error = %err, "failed to fetch kommun page");
                }
            }
        }

        for (lan, (oplanerade, planerade)) in lan_fallback {
            let source_id = format!("lan-{}", lan.to_lowercase().replace(' ', "-"));
            if let Some(event) = to_event(&lan, &source_id, oplanerade, planerade) {
                events.push(event);
            }
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_case_kommun_specific_page_yields_no_event() {
        let html = include_str!("../tests/fixtures/karlstad_zero.html");
        let result = parse_page(html);
        assert_eq!(result, PageResult::KommunSpecific { oplanerade: 0, planerade: 0 });
        assert!(to_event("Karlstad", "karlstad", 0, 0).is_none());
    }

    #[test]
    fn active_kommun_specific_page_parses_counts() {
        let html = include_str!("../tests/fixtures/kungsbacka_active.html");
        let result = parse_page(html);
        assert_eq!(result, PageResult::KommunSpecific { oplanerade: 37, planerade: 0 });
    }

    #[test]
    fn active_kommun_produces_fault_event() {
        let event = to_event("Kungsbacka", "kungsbacka", 37, 0).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.affected_customers, Some(37));
    }

    #[test]
    fn planned_only_produces_planned_event() {
        let event = to_event("Test", "test", 0, 5).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn fallback_page_parses_all_lan_rows() {
        let html = include_str!("../tests/fixtures/knivsta_fallback.html");
        let result = parse_page(html);
        match result {
            PageResult::NationwideFallback(rows) => {
                assert_eq!(rows.len(), 7);
                let halland = rows.iter().find(|(name, _, _)| name.contains("Halland")).unwrap();
                assert_eq!(halland.1, 37);
                assert_eq!(halland.2, 0);
            }
            other => panic!("expected NationwideFallback, got {other:?}"),
        }
    }

    #[test]
    fn poll_deduplicates_fallback_lan_across_many_kommuner() {
        // Simulates what poll() does internally: many kommuner all
        // returning the same fallback table should collapse to one event
        // per län, not one per kommun.
        let html = include_str!("../tests/fixtures/knivsta_fallback.html");
        let mut lan_fallback: HashMap<String, (i32, i32)> = HashMap::new();
        for _ in 0..10 {
            if let PageResult::NationwideFallback(rows) = parse_page(html) {
                for (lan, op, pl) in rows {
                    lan_fallback.insert(lan, (op, pl));
                }
            }
        }
        assert_eq!(lan_fallback.len(), 7);
    }
}
