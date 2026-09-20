//! Övik Energi adapter.
//!
//! `/driftinformation` is server-rendered (SiteVision "predefinedsearch
//! portlet" - a saved-search content listing, a third distinct SiteVision
//! pattern in this workspace alongside Sandviken/VänerEnergi's built-in
//! "archive portlet" and Skurup/Höganäs's custom "webapp"). Three such
//! portlets sit on the page (Pågående/Planerade/Åtgärdade), but rather
//! than track which section an item came from, each `<li>` already
//! carries its own status via a CSS class - `drift-status-{status}` - so
//! this adapter just parses every `li.sv-search-hit` on the whole page
//! and reads that class directly, regardless of which section it's in.
//!
//! One shared feed covers el/fiber/fjärrvärme/fjärrkyla - the category is
//! the first segment of the heading text, "{Kategori}: {Titel}" (e.g.
//! "Fibernät: Planerad avbrottsperiod...") - only "Elnät" is kept.
//!
//! Only `drift-status-planned` was seen in a real, live example when this
//! was written (the page had exactly one current item). `ended` is a
//! reasonable guess by analogy with Skurup's `"Åtgärdat"` (skipped -
//! nothing to resolve if never seen active), and anything else defaults
//! to Fault - neither is confirmed against a real unplanned/ended example.

use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};

const URL: &str = "https://www.ovikenergi.se/driftinformation";
const ELNAT_CATEGORY: &str = "Elnät";

#[derive(Debug, PartialEq)]
struct Entry {
    id: String,
    category: String,
    title: String,
    status: String,
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_entries(html: &str) -> Vec<Entry> {
    let document = Html::parse_document(html);
    let li_sel = Selector::parse("li.sv-search-hit").unwrap();
    let a_sel = Selector::parse("h2 a").unwrap();
    let status_sel = Selector::parse("span.drift-status").unwrap();

    let mut entries = Vec::new();
    for li in document.select(&li_sel) {
        let Some(a) = li.select(&a_sel).next() else { continue };
        let href = a.value().attr("href").unwrap_or("").to_string();
        if href.is_empty() {
            continue;
        }
        let heading = text_of(&a);
        let (category, title) = heading.split_once(": ").unwrap_or(("", heading.as_str()));

        let status = li
            .select(&status_sel)
            .next()
            .and_then(|s| {
                s.value()
                    .classes()
                    .find(|c| c.starts_with("drift-status-"))
                    .map(|c| c.trim_start_matches("drift-status-").to_string())
            })
            .unwrap_or_default();

        entries.push(Entry { id: href, category: category.to_string(), title: title.to_string(), status });
    }
    entries
}

fn to_event(entry: &Entry) -> Option<RawOutageEvent> {
    if entry.category != ELNAT_CATEGORY {
        return None;
    }
    if entry.status == "ended" {
        return None;
    }
    let status = if entry.status == "planned" { OutageStatus::Planned } else { OutageStatus::Fault };

    Some(RawOutageEvent {
        provider: Provider::Ovikenergi,
        source_id: entry.id.clone(),
        status,
        area_label: entry.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: None,
        started_at: None,
        estimated_end_at: None,
        observed_at: chrono::Utc::now(),
    })
}

pub struct OvikenergiAdapter {
    client: reqwest::Client,
}

impl OvikenergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for OvikenergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for OvikenergiAdapter {
    fn name(&self) -> &'static str {
        "ovikenergi"
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

    fn entry(category: &str, status: &str) -> Entry {
        Entry { id: "/x".into(), category: category.into(), title: "Test".into(), status: status.into() }
    }

    #[test]
    fn non_elnat_category_is_filtered() {
        let e = entry("Fibernät", "planned");
        assert!(to_event(&e).is_none());
    }

    #[test]
    fn ended_status_is_skipped() {
        let e = entry("Elnät", "ended");
        assert!(to_event(&e).is_none());
    }

    #[test]
    fn planned_status_is_planned() {
        let e = entry("Elnät", "planned");
        let event = to_event(&e).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn unrecognized_status_defaults_to_fault() {
        let e = entry("Elnät", "active");
        let event = to_event(&e).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
    }

    #[test]
    fn real_fixture_parses_the_live_entry() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let entries = parse_entries(html);
        assert!(!entries.is_empty(), "expected at least one entry in the real fixture");
        let fiber = entries.iter().find(|e| e.category == "Fibernät").expect("expected the real Fibernät entry");
        assert_eq!(fiber.status, "planned");
        assert!(fiber.title.contains("Örnsköldsviks"));
        // The one real entry is Fibernät, not Elnät - confirms the
        // category filter actually engages on real markup.
        assert!(to_event(fiber).is_none());
    }
}
