//! Kungälv Energi adapter.
//!
//! `/driftinformation` is server-rendered (SiteVision "sv-script-portlet",
//! class `b3-rss-driftinformation` - a custom RSS-backed widget, unlike
//! Sandviken/VänerEnergi's built-in "archive portlet"). Also embeds an
//! iframe pointing at `avbrottsinfo.svenskaenergigruppen.se/kungalv-energi/`
//! (a shared multi-tenant platform also used by Njudung Energi and Ale
//! El, per README) - but that domain was unreachable (503) when this was
//! written, so this adapter reads Kungälv's own server-rendered mirror of
//! the same data instead, sidestepping the block entirely.
//!
//! One shared feed covers both "Elnät" and "Stadsnät" (fiber) - the
//! category is embedded in a badge as "{Kategori} ({Område})", e.g.
//! "Elnät (Kode)" or "Stadsnät (44274 Harestad)" - only "Elnät" is kept.
//! Status comes from a CSS class on a dedicated badge span - `ended`
//! (skipped, nothing to resolve if never seen active) or `planned`
//! (Planned); anything else defaults to Fault, since no genuinely-active
//! unplanned entry was available to confirm that class name against.
//!
//! No id or link per entry - the heading text itself ("Avbrott {date}
//! {time} - {Kategori} ({Område})") is already a natural unique key and
//! is used directly as the source id.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{Html, Selector};

const URL: &str = "https://www.kungalvenergi.se/driftinformation";

#[derive(Debug, PartialEq)]
struct Entry {
    heading: String,
    category_area: String,
    status_class: String,
    started_at: Option<DateTime<Utc>>,
    estimated_end_at: Option<DateTime<Utc>>,
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_stockholm(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn parse_entries(html: &str) -> Vec<Entry> {
    let document = Html::parse_document(html);
    let item_sel = Selector::parse(".b3-rss-driftinformation__item").unwrap();
    let badge_sel = Selector::parse(".b3-rss-driftinformation__item-meta .env-badge").unwrap();
    let heading_sel = Selector::parse(".b3-rss-driftinformation__item-heading").unwrap();
    let status_sel = Selector::parse(".b3-rss-driftinformation__item-badge.env-badge").unwrap();
    let time_sel = Selector::parse(".b3-rss-driftinformation__item-info time").unwrap();

    let mut entries = Vec::new();
    for item in document.select(&item_sel) {
        let category_area = item.select(&badge_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        let heading = item.select(&heading_sel).next().map(|e| text_of(&e)).unwrap_or_default();
        if heading.is_empty() {
            continue;
        }

        let status_class = item
            .select(&status_sel)
            .next()
            .map(|e| e.value().classes().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        let times: Vec<DateTime<Utc>> =
            item.select(&time_sel).filter_map(|t| parse_stockholm(&text_of(&t))).collect();

        entries.push(Entry {
            heading,
            category_area,
            status_class,
            started_at: times.first().copied(),
            estimated_end_at: times.get(1).copied(),
        });
    }
    entries
}

fn to_event(entry: &Entry) -> Option<RawOutageEvent> {
    if !entry.category_area.starts_with("Elnät") {
        return None;
    }
    if entry.status_class.split_whitespace().any(|c| c == "ended") {
        return None;
    }
    let status = if entry.status_class.split_whitespace().any(|c| c == "planned") {
        OutageStatus::Planned
    } else {
        OutageStatus::Fault
    };

    Some(RawOutageEvent {
        provider: Provider::Kungalv,
        source_id: entry.heading.clone(),
        status,
        area_label: entry.category_area.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: None,
        started_at: entry.started_at,
        estimated_end_at: entry.estimated_end_at,
        observed_at: Utc::now(),
    })
}

pub struct KungalvAdapter {
    client: reqwest::Client,
}

impl KungalvAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for KungalvAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for KungalvAdapter {
    fn name(&self) -> &'static str {
        "kungalv"
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

    fn entry(category_area: &str, status_class: &str) -> Entry {
        Entry {
            heading: "Avbrott 2026-09-23 09:00 - Elnät (Kungälv)".into(),
            category_area: category_area.into(),
            status_class: status_class.into(),
            started_at: None,
            estimated_end_at: None,
        }
    }

    #[test]
    fn stadsnat_category_is_filtered() {
        let e = entry("Stadsnät (44274 Harestad)", "planned");
        assert!(to_event(&e).is_none());
    }

    #[test]
    fn ended_status_is_skipped() {
        let e = entry("Elnät (Kungälv)", "env-badge ended");
        assert!(to_event(&e).is_none());
    }

    #[test]
    fn planned_status_is_planned() {
        let e = entry("Elnät (Kode)", "b3-rss-driftinformation__item-badge env-badge env-d--inline-flex planned");
        let event = to_event(&e).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn unrecognized_status_defaults_to_fault() {
        let e = entry("Elnät (Hålta)", "env-badge active");
        let event = to_event(&e).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
    }

    #[test]
    fn parses_start_and_end_times() {
        let dt = parse_stockholm("2026-10-06 01:00:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-10-05T23:00:00+00:00");
    }

    #[test]
    fn real_fixture_parses_and_filters_correctly() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let entries = parse_entries(html);
        assert_eq!(entries.len(), 11, "expected 11 items in the real fixture");

        let elnat_count = entries.iter().filter(|e| e.category_area.starts_with("Elnät")).count();
        let stadsnat_count = entries.iter().filter(|e| e.category_area.starts_with("Stadsnät")).count();
        assert!(elnat_count > 0, "expected at least one Elnät entry");
        assert!(stadsnat_count > 0, "expected at least one Stadsnät entry (to prove filtering matters)");

        for e in &entries {
            if e.category_area.starts_with("Stadsnät") {
                assert!(to_event(e).is_none());
            }
        }

        // At least one real entry should have real start/end times parsed.
        assert!(entries.iter().any(|e| e.started_at.is_some()));
    }
}
