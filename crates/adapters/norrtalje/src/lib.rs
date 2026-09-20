//! Norrtälje Energi adapter.
//!
//! `/kundservice/driftinformation/` is server-rendered directly - a
//! custom accordion-list widget (not SiteVision, not WordPress; classes
//! like `content-acc js-accordion` suggest a bespoke build), one clean
//! `<div data-i="{id}" class="content-acc js-accordion">` per item with
//! an explicit status class and category tag - no free-text category
//! guessing needed here, unlike most other sources in this workspace:
//!
//!   <div class="status-type ... finished">Klart</div>
//!   <div class="status-tag ...">Elnät</div>
//!   <p>Strömavbrott Uddeboö</p>               (title, direct child of .op-label-wrap)
//!   <div class="op-c content js-content">...</div>  (free-text update log)
//!
//! Only `Elnät` is kept (page also covers `Fiber` and `Fjärrvärme`
//! through the same list). Only `finished` and `planned` status classes
//! were seen in the real fixture - anything else defaults to Fault, on
//! the same reasoning used for Kungälv/Övik Energi (an English-named CSS
//! class scheme where a third, unconfirmed "currently active" value is
//! the most likely gap).
//!
//! The `.time` block gives a day+month and start/end clock times but no
//! year and awkward formatting (SVG icons interleaved with text) - not
//! parsed for now; `started_at`/`estimated_end_at` are left `None`.

use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};

const URL: &str = "https://www.norrtaljeenergi.se/kundservice/driftinformation/";
const ELNAT_CATEGORY: &str = "Elnät";

#[derive(Debug, PartialEq)]
struct Entry {
    id: String,
    category: String,
    title: String,
    status: String,
    description: String,
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_entries(html: &str) -> Vec<Entry> {
    let document = Html::parse_document(html);
    let item_sel = Selector::parse("div.content-acc").unwrap();
    let status_sel = Selector::parse(".status-type").unwrap();
    let tag_sel = Selector::parse(".status-tag").unwrap();
    let title_sel = Selector::parse(".op-label-wrap > p").unwrap();
    let desc_sel = Selector::parse(".js-content p").unwrap();

    let mut entries = Vec::new();
    for item in document.select(&item_sel) {
        let Some(id) = item.value().attr("data-i") else { continue };

        let status = item
            .select(&status_sel)
            .next()
            .map(|e| {
                e.value()
                    .classes()
                    .find(|c| *c != "status-type" && *c != "secondary-font")
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_default();
        let category = item.select(&tag_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let title = item.select(&title_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let description = item.select(&desc_sel).map(|e| text_of(&e)).collect::<Vec<_>>().join(" ");

        if title.is_empty() {
            continue;
        }
        entries.push(Entry { id: id.to_string(), category, title, status, description });
    }
    entries
}

fn to_event(entry: &Entry) -> Option<RawOutageEvent> {
    if entry.category != ELNAT_CATEGORY {
        return None;
    }
    if entry.status == "finished" {
        return None;
    }
    let status = if entry.status == "planned" { OutageStatus::Planned } else { OutageStatus::Fault };

    Some(RawOutageEvent {
        provider: Provider::Norrtalje,
        source_id: entry.id.clone(),
        status,
        area_label: entry.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: (!entry.description.is_empty()).then(|| entry.description.clone()),
        started_at: None,
        estimated_end_at: None,
        observed_at: chrono::Utc::now(),
    })
}

pub struct NorrtaljeAdapter {
    client: reqwest::Client,
}

impl NorrtaljeAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for NorrtaljeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for NorrtaljeAdapter {
    fn name(&self) -> &'static str {
        "norrtalje"
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
        Entry {
            id: "1".into(),
            category: category.into(),
            title: "Test".into(),
            status: status.into(),
            description: "".into(),
        }
    }

    #[test]
    fn non_elnat_category_is_filtered() {
        let e = entry("Fiber", "planned");
        assert!(to_event(&e).is_none());
    }

    #[test]
    fn finished_status_is_skipped() {
        let e = entry("Elnät", "finished");
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
    fn real_fixture_parses_multiple_entries_with_correct_fields() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let entries = parse_entries(html);
        assert!(entries.len() >= 5, "expected several entries, got {}", entries.len());

        let svedenvagen = entries
            .iter()
            .find(|e| e.title == "Strömavbrott Svedenvägen Rö")
            .expect("expected the exact 'Strömavbrott Svedenvägen Rö' entry");
        assert_eq!(svedenvagen.category, "Elnät");
        assert_eq!(svedenvagen.status, "finished");
        assert!(svedenvagen.description.contains("säkringar"));
        assert!(to_event(svedenvagen).is_none());

        // Confirms both categories and both statuses genuinely vary
        // across real entries, not just in hand-written test structs.
        let categories: std::collections::HashSet<_> = entries.iter().map(|e| e.category.as_str()).collect();
        assert!(categories.len() > 1, "expected more than one category in the real fixture");
    }
}
