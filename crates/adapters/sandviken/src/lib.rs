//! Sandviken Energi adapter.
//!
//! `/kundservice/driftinfo.94.html` is server-rendered directly (SiteVision
//! CMS's built-in "archive portlet" - a native content listing feature,
//! not a custom widget calling out to a separate API like the SiteVision
//! sites we've seen elsewhere, e.g. Höganäs). One page holds two relevant
//! sections, each its own archive-portlet list, identified by a marker
//! div right before it:
//!
//!   <div id="Pagaende"><!-- Pågående --></div>
//!   ...<ul class="sv-channel ...">...</ul>   (currently active)
//!   <div id="Planerade"><!-- Planerade --></div>
//!   ...<ul class="sv-channel ...">...</ul>   (scheduled, not yet started)
//!
//! A third "Åtgärdade arbeten" (resolved) section exists but lives on a
//! separate page entirely - not fetched, since our own staleness sweep
//! already handles marking things resolved once a source stops reporting
//! them as active.
//!
//! Each `<li>` covers electricity, water, district heating and broadband
//! all in one shared feed - the only category marker is embedded in the
//! free-text summary line itself: "{Ort} - {Kategori} - {Titel}", e.g.
//! "Sandviken - Elnät - Strömavbrott". Only `Kategori == "Elnät"` is kept.
//! Within the "Pågående" list (whose own heading admits it can hold
//! *either* a fault *or* ongoing planned work), the `Titel` segment is
//! used to tell them apart: "Strömavbrott" (unplanned) vs anything
//! mentioning "underhåll" (planned maintenance) - defaulting to Fault
//! when neither keyword matches, since this list is presented as
//! currently affecting customers either way.

use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};

const URL: &str = "https://sandvikenenergi.se/kundservice/driftinfo.94.html";
const ELNAT_CATEGORY: &str = "Elnät";

#[derive(Debug, PartialEq)]
struct Entry {
    id: String,
    date: Option<DateTime<Utc>>,
    place: String,
    category: String,
    title: String,
    detail: String,
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_entries_in(ul: &ElementRef) -> Vec<Entry> {
    let li_sel = Selector::parse("li.sv-channel-item").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let time_sel = Selector::parse("time").unwrap();
    let summary_sel = Selector::parse("span.env-text-summary-01").unwrap();
    let body_sel = Selector::parse("span.env-text-body-01").unwrap();

    let mut entries = Vec::new();
    for li in ul.select(&li_sel) {
        let href = li.select(&a_sel).next().and_then(|a| a.value().attr("href")).unwrap_or("");
        // The numeric id SiteVision appends to every content page's URL
        // (".../foo.8971.html") - stable and unique, unlike the free text.
        let id = href
            .rsplit('.')
            .nth(1)
            .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }

        let date = li
            .select(&time_sel)
            .next()
            .and_then(|t| t.value().attr("datetime"))
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // First non-empty "summary" span is "{Ort} - {Kategori} - {Titel}"
        // - a second, empty one (just a <br> spacer) is common and must
        // be skipped rather than treated as the real value.
        let summary = li
            .select(&summary_sel)
            .map(|el| text_of(&el))
            .find(|t| !t.trim().is_empty())
            .unwrap_or_default();
        let mut parts = summary.splitn(3, " - ").map(|s| s.trim().to_string());
        let place = parts.next().unwrap_or_default();
        let category = parts.next().unwrap_or_default();
        let title = parts.next().unwrap_or_default();

        // Remaining body spans are free-text detail (street/area); the
        // <time> element also carries this same CSS class, but selecting
        // `span.env-text-body-01` specifically excludes it - only the
        // spacer-only ones (just a <br>) need filtering here.
        let detail = li
            .select(&body_sel)
            .map(|el| text_of(&el))
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        entries.push(Entry { id, date, place, category, title, detail });
    }
    entries
}

/// Finds the marker div with this id, then the next `<ul class="sv-channel...">`
/// after it in document order, and parses its entries. `ego_tree::NodeId`
/// has no `Ord` impl, so this walks the underlying arena directly (which
/// is itself in document/parse order) rather than trying to compare ids.
fn parse_section(document: &Html, marker_id: &str) -> Vec<Entry> {
    let ul_sel = Selector::parse("ul.sv-channel").unwrap();

    let mut past_marker = false;
    for node in document.tree.nodes() {
        let Some(element) = node.value().as_element() else { continue };
        if !past_marker {
            if element.attr("id") == Some(marker_id) {
                past_marker = true;
            }
            continue;
        }
        let Some(el_ref) = ElementRef::wrap(node) else { continue };
        if ul_sel.matches(&el_ref) {
            return parse_entries_in(&el_ref);
        }
    }
    Vec::new()
}

fn to_event(entry: &Entry, upcoming: bool) -> Option<RawOutageEvent> {
    if entry.category != ELNAT_CATEGORY {
        return None;
    }
    let status = if upcoming {
        OutageStatus::Upcoming
    } else if entry.title.to_lowercase().contains("underhåll") {
        OutageStatus::Planned
    } else {
        OutageStatus::Fault
    };

    let area_label = if entry.detail.is_empty() {
        format!("{} - {}", entry.place, entry.title)
    } else {
        format!("{} - {} ({})", entry.place, entry.title, entry.detail)
    };

    Some(RawOutageEvent {
        provider: Provider::Sandviken,
        source_id: entry.id.clone(),
        status,
        area_label,
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

pub struct SandvikenAdapter {
    client: reqwest::Client,
}

impl SandvikenAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for SandvikenAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for SandvikenAdapter {
    fn name(&self) -> &'static str {
        "sandviken"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(URL).send().await?.text().await?;
        let document = Html::parse_document(&body);

        let active = parse_section(&document, "Pagaende");
        let planned = parse_section(&document, "Planerade");

        let mut events: Vec<RawOutageEvent> = active.iter().filter_map(|e| to_event(e, false)).collect();
        events.extend(planned.iter().filter_map(|e| to_event(e, true)));
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_elnat_category_is_filtered() {
        let entry = Entry {
            id: "1".into(),
            date: None,
            place: "Sandviken".into(),
            category: "Vatten och avlopp".into(),
            title: "Underhållsarbete".into(),
            detail: "".into(),
        };
        assert!(to_event(&entry, false).is_none());
    }

    #[test]
    fn stromavbrott_title_is_fault() {
        let entry = Entry {
            id: "1".into(),
            date: None,
            place: "Sandviken".into(),
            category: "Elnät".into(),
            title: "Strömavbrott".into(),
            detail: "Agavägen".into(),
        };
        let event = to_event(&entry, false).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert!(event.area_label.contains("Agavägen"));
    }

    #[test]
    fn underhall_title_is_planned() {
        let entry = Entry {
            id: "1".into(),
            date: None,
            place: "Sandviken".into(),
            category: "Elnät".into(),
            title: "Underhållsarbete".into(),
            detail: "".into(),
        };
        let event = to_event(&entry, false).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn planned_section_is_always_upcoming() {
        let entry = Entry {
            id: "1".into(),
            date: None,
            place: "Sandviken".into(),
            category: "Elnät".into(),
            title: "Strömavbrott".into(),
            detail: "".into(),
        };
        let event = to_event(&entry, true).unwrap();
        assert_eq!(event.status, OutageStatus::Upcoming);
    }

    #[test]
    fn real_fixture_parses_active_section() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let document = Html::parse_document(html);
        let active = parse_section(&document, "Pagaende");
        assert!(!active.is_empty(), "expected at least one active entry in the real fixture");

        let elnat: Vec<_> = active.iter().filter(|e| e.category == ELNAT_CATEGORY).collect();
        assert!(!elnat.is_empty(), "expected at least one Elnät entry");
        let first = elnat[0];
        assert_eq!(first.place, "Sandviken");
        assert!(first.title.contains("Strömavbrott"));
        assert!(first.detail.contains("Agav"), "detail was: {}", first.detail);
        assert!(first.date.is_some());
    }

    #[test]
    fn real_fixture_ids_are_numeric_and_unique() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let document = Html::parse_document(html);
        let active = parse_section(&document, "Pagaende");
        let ids: Vec<&str> = active.iter().map(|e| e.id.as_str()).collect();
        for id in &ids {
            assert!(id.chars().all(|c| c.is_ascii_digit()), "id was not numeric: {id}");
        }
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "expected unique ids, got {ids:?}");
    }
}
