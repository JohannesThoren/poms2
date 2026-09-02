//! Eksjö Energi adapter.
//!
//! Their `/driftinformation/` page is plain server-rendered HTML (a
//! "Compileit" widget, judging by the `data-cbid` attribute, but the
//! actual content ships in the HTML itself - no separate API call
//! needed). Each incident is a `.driftitem` block with a severity class
//! (`color-klar` = cleared/resolved, others = still active) and, inside
//! its `<h3>`, an area label (`.omrade`, e.g. "Elnät", "Vatten &
//! avlopp"), a type label (`.planerat`, present only for planned work),
//! and a time range (`.tid`).
//!
//! **Confidence note**: at the time this was written, Elnät had no
//! *currently active* incident, but the page's history does include past
//! electricity outages (e.g. "Strömavbrott Tannarp, Alversjö och Övrabo"),
//! which confirmed the "no `.planerat` span = unplanned fault" and
//! "`color-klar` = resolved" assumptions against real Elnät data - not
//! just the one Vatten & avlopp example. The one part still unconfirmed
//! against a real electricity case is the "planned and not yet started"
//! (Upcoming) branch, since the only `.planerat` example seen was for
//! water. There's also no stable id anywhere in the markup, so
//! `source_id` is a hash of the title + time range - stable across polls
//! as long as the wording doesn't change, but not a real identifier.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{ElementRef, Html, Selector};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const URL: &str = "https://eksjoenergi.se/driftinformation/";
const AREA_FILTER: &str = "Elnät";

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn parse_stockholm(s: &str) -> Option<DateTime<Utc>> {
    // "2026-08-31 kl 00:00"
    let cleaned = s.replace("kl", "").trim().to_string();
    let naive = NaiveDateTime::parse_from_str(&cleaned, "%Y-%m-%d %H:%M").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

/// Splits a "`start` – `end`" range (en-dash separated) into its two
/// halves; if there's no dash, treats the whole string as just a start.
fn split_time_range(tid: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    if let Some((start, end)) = tid.split_once('–') {
        (parse_stockholm(start), parse_stockholm(end))
    } else {
        (parse_stockholm(tid), None)
    }
}

fn stable_id(title: &str, tid: &str) -> String {
    let mut hasher = DefaultHasher::new();
    title.hash(&mut hasher);
    tid.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

struct ParsedItem {
    is_cleared: bool,
    area: String,
    planerat: Option<String>,
    tid: String,
    title: String,
    description: Option<String>,
}

fn parse_items(html: &str) -> Vec<ParsedItem> {
    let document = Html::parse_document(html);
    let item_sel = Selector::parse("div.driftitem").unwrap();
    let h3_sel = Selector::parse("h3").unwrap();
    let omrade_sel = Selector::parse("span.omrade").unwrap();
    let planerat_sel = Selector::parse("span.planerat").unwrap();
    let tid_sel = Selector::parse("span.tid").unwrap();
    let merinfo_sel = Selector::parse("div.merinfo p").unwrap();

    let mut items = Vec::new();

    for item in document.select(&item_sel) {
        let class = item.value().attr("class").unwrap_or("");
        let is_cleared = class.contains("color-klar");

        let Some(h3) = item.select(&h3_sel).next() else { continue };
        let area = h3.select(&omrade_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let planerat = h3.select(&planerat_sel).next().map(|e| text_of(&e));
        let tid = h3.select(&tid_sel).next().map(|e| text_of(&e)).unwrap_or_default();

        // The title is whatever text is left in the <h3> once the known
        // sub-spans' text is stripped out - there's no dedicated element
        // for it in this markup.
        let mut title = text_of(&h3);
        if !area.is_empty() {
            title = title.replace(&area, "");
        }
        if let Some(p) = &planerat {
            title = title.replace(p, "");
        }
        if !tid.is_empty() {
            title = title.replace(&tid, "");
        }
        let title = title.trim().to_string();

        let description = item.select(&merinfo_sel).next().map(|e| text_of(&e)).filter(|s| !s.is_empty());

        items.push(ParsedItem { is_cleared, area, planerat, tid, title, description });
    }

    items
}

fn to_event(item: &ParsedItem, now: DateTime<Utc>) -> Option<RawOutageEvent> {
    if item.area != AREA_FILTER {
        return None;
    }

    let (started_at, estimated_end_at) = split_time_range(&item.tid);

    let status = if item.is_cleared {
        OutageStatus::Resolved
    } else if item.planerat.as_deref().is_some_and(|p| p.contains("Planerat")) {
        match started_at {
            Some(start) if start > now => OutageStatus::Upcoming,
            _ => OutageStatus::Planned,
        }
    } else {
        OutageStatus::Fault
    };

    Some(RawOutageEvent {
        provider: Provider::Eksjo,
        source_id: stable_id(&item.title, &item.tid),
        status,
        area_label: if item.title.is_empty() { item.area.clone() } else { item.title.clone() },
        lat: None,
        lng: None,
        affected_customers: None,
        reason: item.description.clone(),
        started_at,
        estimated_end_at,
        observed_at: now,
    })
}

pub struct EksjoAdapter {
    client: reqwest::Client,
}

impl EksjoAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for EksjoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for EksjoAdapter {
    fn name(&self) -> &'static str {
        "eksjo"
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
    fn real_fixture_finds_elnat_items_all_resolved() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let items = parse_items(html);
        assert!(!items.is_empty());

        let now = Utc::now();
        let events: Vec<_> = items.iter().filter_map(|i| to_event(i, now)).collect();
        // The captured fixture's Elnät items are all past, cleared
        // incidents (there's no live outage right now) - confirms the
        // "no .planerat span" heuristic correctly reads a real
        // electricity fault (not just the one Vatten & avlopp example).
        assert!(!events.is_empty(), "should find real historical Elnät items");
        assert!(events.iter().all(|e| e.status == OutageStatus::Resolved));
        assert!(events.iter().any(|e| e.area_label.contains("Tannarp")));
    }

    #[test]
    fn cleared_class_is_resolved() {
        let item = ParsedItem {
            is_cleared: true,
            area: "Elnät".into(),
            planerat: None,
            tid: "2026-01-01 kl 00:00".into(),
            title: "Test".into(),
            description: None,
        };
        assert_eq!(to_event(&item, Utc::now()).unwrap().status, OutageStatus::Resolved);
    }

    #[test]
    fn no_planerat_span_is_fault() {
        let item = ParsedItem {
            is_cleared: false,
            area: "Elnät".into(),
            planerat: None,
            tid: "2026-01-01 kl 00:00".into(),
            title: "Test".into(),
            description: None,
        };
        assert_eq!(to_event(&item, Utc::now()).unwrap().status, OutageStatus::Fault);
    }

    #[test]
    fn non_elnat_area_is_filtered() {
        let item = ParsedItem {
            is_cleared: false,
            area: "Vatten & avlopp".into(),
            planerat: None,
            tid: "2026-01-01 kl 00:00".into(),
            title: "Test".into(),
            description: None,
        };
        assert!(to_event(&item, Utc::now()).is_none());
    }

    #[test]
    fn splits_time_range() {
        let (start, end) = split_time_range("2026-08-31 kl 00:00 – 2026-10-11 kl 00:00");
        assert!(start.is_some());
        assert!(end.is_some());
        assert!(start.unwrap() < end.unwrap());
    }

    #[test]
    fn single_time_has_no_end() {
        let (start, end) = split_time_range("2026-08-31 kl 00:00");
        assert!(start.is_some());
        assert!(end.is_none());
    }
}

