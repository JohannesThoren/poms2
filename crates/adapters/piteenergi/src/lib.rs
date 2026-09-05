//! PiteEnergi adapter.
//!
//! Their `/driftinformation/` page is server-rendered HTML with three
//! clearly separated sections - `<h2>` headings "Pågående avbrott",
//! "Planerade avbrott", "Avklarade avbrott" - each followed by a
//! `.o-disruptions__list` of `.m-disruption-list-item` cards. Unlike
//! Eksjö Energi, status here comes directly from which section an item is
//! in, not from a fragile class/heuristic guess, and each item already
//! carries a household count (`Berörda hushåll`) - among the cleaner
//! sources in this system.
//!
//! Field labels vary slightly by section: planned items say "Förväntas
//! klart", resolved ones say "Sluttid" - both are just "when it ends" and
//! are read interchangeably here. "Berörda hushåll" is sometimes absent
//! (seen missing on at least one resolved item), so it's optional.
//!
//! The map link (`ettnoll.isy.se/avbrott/karta/PiteEnergi/el`) points at
//! a small separate vendor product ("EttNoll") that didn't yield an
//! obvious public API of its own when checked - but PiteEnergi's own page
//! already has richer structured data than the map alone would, so this
//! adapter doesn't need it.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;

const URL: &str = "https://www.piteenergi.se/driftinformation/";
const TYPE_FILTER: &str = "Elnät";

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_stockholm(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn section_status(heading: &str) -> Option<fn(Option<DateTime<Utc>>, DateTime<Utc>) -> OutageStatus> {
    if heading.contains("Pågående") {
        Some(|_start, _now| OutageStatus::Fault)
    } else if heading.contains("Planerade") {
        Some(|start, now| match start {
            Some(s) if s > now => OutageStatus::Upcoming,
            _ => OutageStatus::Planned,
        })
    } else if heading.contains("Avklarade") {
        Some(|_start, _now| OutageStatus::Resolved)
    } else {
        None
    }
}

struct ParsedItem {
    id: String,
    description: Option<String>,
    facts: HashMap<String, String>,
    status_fn: fn(Option<DateTime<Utc>>, DateTime<Utc>) -> OutageStatus,
}

fn parse_items(html: &str) -> Vec<ParsedItem> {
    let document = Html::parse_document(html);
    let section_sel = Selector::parse("div.o-disruptions__disruptions > div").unwrap();
    let heading_sel = Selector::parse("h2.o-disruptions__list-heading").unwrap();
    let item_sel = Selector::parse("div.m-disruption-list-item").unwrap();
    let intro_sel = Selector::parse(".m-disruption-list-item__intro-text").unwrap();
    let row_sel = Selector::parse(".m-disruption-list-item__facts-row").unwrap();
    let label_sel = Selector::parse(".m-disruption-list-item__facts-label").unwrap();
    let button_sel = Selector::parse("button.js-toggle-list-item").unwrap();

    let mut items = Vec::new();

    for section in document.select(&section_sel) {
        let Some(heading_el) = section.select(&heading_sel).next() else { continue };
        let Some(status_fn) = section_status(&text_of(&heading_el)) else { continue };

        for item in section.select(&item_sel) {
            let id = item
                .select(&button_sel)
                .next()
                .and_then(|b| b.value().attr("aria-controls"))
                .unwrap_or("")
                .to_string();

            let description = item.select(&intro_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());

            let mut facts = HashMap::new();
            for row in item.select(&row_sel) {
                let raw_label = row.select(&label_sel).next().map(|e| text_of(&e)).unwrap_or_default();
                if raw_label.is_empty() {
                    continue;
                }
                let key = raw_label.trim_end_matches(':').trim().to_lowercase();
                let full_text = text_of(&row);
                let value = full_text.replacen(&raw_label, "", 1);
                facts.insert(key, value.trim().to_string());
            }

            items.push(ParsedItem { id, description, facts, status_fn });
        }
    }

    items
}

fn to_event(item: &ParsedItem, now: DateTime<Utc>) -> Option<RawOutageEvent> {
    let outage_type = item.facts.get("avbrottstyp")?;
    if outage_type != TYPE_FILTER {
        return None;
    }
    if item.id.is_empty() {
        return None;
    }

    let started_at = item.facts.get("starttid").and_then(|s| parse_stockholm(s));
    let estimated_end_at = item
        .facts
        .get("förväntas klart")
        .or_else(|| item.facts.get("sluttid"))
        .and_then(|s| parse_stockholm(s));

    let affected_customers = item
        .facts
        .get("berörda hushåll")
        .and_then(|s| s.split_whitespace().next())
        .and_then(|n| n.parse::<i32>().ok());

    Some(RawOutageEvent {
        provider: Provider::Pite,
        source_id: item.id.clone(),
        status: (item.status_fn)(started_at, now),
        area_label: item.description.clone().unwrap_or_else(|| format!("Avbrott #{}", item.id)),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers,
        reason: item.description.clone(),
        started_at,
        estimated_end_at,
        observed_at: now,
    })
}

pub struct PiteEnergiAdapter {
    client: reqwest::Client,
}

impl PiteEnergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for PiteEnergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for PiteEnergiAdapter {
    fn name(&self) -> &'static str {
        "piteenergi"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(URL).send().await?.text().await?;
        let items = parse_items(&body);
        let now = Utc::now();
        Ok(items.iter().filter_map(|i| to_event(i, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_fixture_parses_and_filters_to_elnat() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let items = parse_items(html);
        assert!(!items.is_empty());

        let now = Utc::now();
        let events: Vec<_> = items.iter().filter_map(|i| to_event(i, now)).collect();
        assert!(!events.is_empty(), "should find at least one real Elnät item");
        for e in &events {
            assert_eq!(e.provider, Provider::Pite);
        }
    }

    #[test]
    fn resolved_section_maps_to_resolved() {
        let f = section_status("Avklarade avbrott").unwrap();
        assert_eq!(f(None, Utc::now()), OutageStatus::Resolved);
    }

    #[test]
    fn ongoing_section_maps_to_fault() {
        let f = section_status("Pågående avbrott").unwrap();
        assert_eq!(f(None, Utc::now()), OutageStatus::Fault);
    }

    #[test]
    fn planned_section_future_start_is_upcoming() {
        let f = section_status("Planerade avbrott").unwrap();
        let now = Utc::now();
        assert_eq!(f(Some(now + chrono::Duration::hours(1)), now), OutageStatus::Upcoming);
    }

    #[test]
    fn non_elnat_type_is_filtered() {
        let mut facts = HashMap::new();
        facts.insert("avbrottstyp".to_string(), "Fjärrvärme".to_string());
        let item = ParsedItem {
            id: "1".into(),
            description: None,
            facts,
            status_fn: |_, _| OutageStatus::Fault,
        };
        assert!(to_event(&item, Utc::now()).is_none());
    }

    #[test]
    fn missing_id_is_skipped() {
        let mut facts = HashMap::new();
        facts.insert("avbrottstyp".to_string(), "Elnät".to_string());
        let item = ParsedItem { id: String::new(), description: None, facts, status_fn: |_, _| OutageStatus::Fault };
        assert!(to_event(&item, Utc::now()).is_none());
    }
}
