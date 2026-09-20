//! VänerEnergi adapter.
//!
//! Same SiteVision "archive portlet" technique as Sandviken Energi
//! (`/kundservice/driftinformation`), but a simpler feed: this page is
//! electricity-only (no shared water/heating/broadband category to filter
//! out - there's no "Kategori" segment in the title at all, just the
//! event title itself), and resolution status is baked directly into the
//! title text as a literal " - Avslutad" suffix rather than a separate
//! section.
//!
//! Only one relevant section exists on this page (marker div
//! `#Driftstorningarejatgardade`, "disruptions not yet resolved") - no
//! separate future/"planerade" section was found, so every entry here is
//! either currently active or already resolved.
//!
//! No numeric content id like Sandviken's `.8971.html` - the URL slug
//! itself (e.g. `2026-09-14-planerat-stromavbrott---avslutad`) is used as
//! the stable id instead, since it already embeds the date and is unique
//! per entry.

use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};

const URL: &str = "https://vanerenergi.se/kundservice/driftinformation";
const MARKER_ID: &str = "Driftstorningarejatgardade";

#[derive(Debug, PartialEq)]
struct Entry {
    id: String,
    date: Option<DateTime<Utc>>,
    title: String,
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Same technique as the Sandviken adapter: `ego_tree::NodeId` has no
/// `Ord`, so document order is found by walking the arena directly rather
/// than comparing ids.
fn parse_entries(html: &str) -> Vec<Entry> {
    let document = Html::parse_document(html);
    let ul_sel = Selector::parse("ul.sv-channel").unwrap();
    let li_sel = Selector::parse("li.sv-channel-item").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let time_sel = Selector::parse("time").unwrap();

    let mut past_marker = false;
    let mut target_ul = None;
    for node in document.tree.nodes() {
        let Some(element) = node.value().as_element() else { continue };
        if !past_marker {
            if element.attr("id") == Some(MARKER_ID) {
                past_marker = true;
            }
            continue;
        }
        if let Some(el_ref) = ElementRef::wrap(node) {
            if ul_sel.matches(&el_ref) {
                target_ul = Some(el_ref);
                break;
            }
        }
    }

    let Some(ul) = target_ul else { return Vec::new() };

    let mut entries = Vec::new();
    for li in ul.select(&li_sel) {
        let Some(a) = li.select(&a_sel).next() else { continue };
        let href = a.value().attr("href").unwrap_or("");
        let id = href.rsplit('/').next().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let title = text_of(&a);
        let date = li
            .select(&time_sel)
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        entries.push(Entry { id, date, title });
    }
    entries
}

fn to_event(entry: &Entry) -> Option<RawOutageEvent> {
    if entry.title.contains("Avslutad") {
        return None;
    }
    let status = if entry.title.to_lowercase().contains("planerat") {
        OutageStatus::Planned
    } else {
        OutageStatus::Fault
    };

    Some(RawOutageEvent {
        provider: Provider::Vanerenergi,
        source_id: entry.id.clone(),
        status,
        area_label: entry.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: None,
        started_at: entry.date,
        estimated_end_at: None,
        observed_at: Utc::now(),
    })
}

pub struct VanerenergiAdapter {
    client: reqwest::Client,
}

impl VanerenergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for VanerenergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for VanerenergiAdapter {
    fn name(&self) -> &'static str {
        "vanerenergi"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(URL).send().await?.text().await?;
        let entries = parse_entries(&body);
        Ok(entries.iter().filter_map(to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_suffix_is_skipped() {
        let entry = Entry { id: "1".into(), date: None, title: "Planerat strömavbrott - Avslutad".into() };
        assert!(to_event(&entry).is_none());
    }

    #[test]
    fn planerat_without_avslutad_is_planned() {
        let entry = Entry { id: "1".into(), date: None, title: "Planerat strömavbrott".into() };
        let event = to_event(&entry).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn non_planerat_is_fault() {
        let entry = Entry { id: "1".into(), date: None, title: "Akut driftstörning".into() };
        let event = to_event(&entry).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
    }

    #[test]
    fn real_fixture_parses_without_panicking() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let entries = parse_entries(html);
        assert!(!entries.is_empty(), "expected at least one entry in the real fixture");
        // Both real entries seen when this fixture was captured were
        // already resolved - confirms the filter engages on real markup,
        // not just hand-written structs.
        assert!(entries.iter().all(|e| e.title.contains("Avslutad")));
        for e in &entries {
            assert!(to_event(e).is_none());
        }
    }

    #[test]
    fn real_fixture_ids_are_unique_slugs() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let entries = parse_entries(html);
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        for id in &ids {
            assert!(id.starts_with("2026-") || id.starts_with("20"), "id was: {id}");
        }
    }
}
