//! Mälarenergi adapter.
//!
//! Mälarenergi runs three separate outage systems simultaneously on their
//! `/avbrott/` page (a Tekla/GeoServer map at driftinfo.malarenergi.se,
//! ServiceAlert via se.sms-service.dk, and their own Next.js API) - this
//! adapter uses the last one, since it's the cleanest, self-built, and
//! gives readable place names directly:
//!
//!   GET https://www.malarenergi.se/api/outages/?type=Ongoing&take=100&skip=0
//!   GET https://www.malarenergi.se/api/outages/?type=Planned&take=100&skip=0
//!
//! (note the trailing slash before the query string - without it the API
//! 308-redirects). No auth. `type` only accepts "Ongoing" or "Planned" -
//! other values (including "All"/"Resolved") return a 400 error, so this
//! adapter queries both valid values and combines them; resolved outages
//! simply aren't returned by this API at all, which is fine since the
//! ingestion service's staleness sweep already covers "stopped being
//! reported" as the resolution signal.
//!
//! No live "Ongoing" or `utility: "Electricity"` example was available
//! when this was written (only district-heating "Planned" entries were
//! active) - the `status` field's possible values beyond "Maintenance" are
//! therefore unconfirmed; adjust `map_status` if a differently-worded
//! status shows up for an ongoing electricity fault.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const BASE_URL: &str = "https://www.malarenergi.se/api/outages/";
const PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize)]
struct OutagesResponse {
    items: Vec<OutageItem>,
}

#[derive(Debug, Deserialize)]
struct OutageItem {
    #[serde(rename = "outageId")]
    outage_id: i64,
    #[serde(rename = "affectedAreas")]
    affected_areas: String,
    #[serde(rename = "currentlyAffectedConnectionPoints")]
    currently_affected_connection_points: i32,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "endTime")]
    end_time: Option<String>,
    #[serde(rename = "type")]
    outage_type: String,
    status: String,
    utility: String,
    description: Option<String>,
}

fn parse_stockholm_time(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn map_status(item: &OutageItem, now: DateTime<Utc>, started_at: Option<DateTime<Utc>>) -> OutageStatus {
    if item.outage_type == "Ongoing" {
        return OutageStatus::Fault;
    }
    // "Planned" - could be before, during, or (in principle) after its
    // window; distinguish using the timestamps rather than trusting the
    // type alone, same approach as the Skellefteå Kraft adapter.
    match started_at {
        Some(start) if start > now => OutageStatus::Upcoming,
        _ => OutageStatus::Planned,
    }
}

fn to_event(item: &OutageItem, now: DateTime<Utc>) -> Option<RawOutageEvent> {
    if item.utility != "Electricity" {
        return None;
    }

    let started_at = parse_stockholm_time(&item.start_time);
    let estimated_end_at = item.end_time.as_deref().and_then(parse_stockholm_time);

    Some(RawOutageEvent {
        provider: Provider::Malarenergi,
        source_id: item.outage_id.to_string(),
        status: map_status(item, now, started_at),
        area_label: item.affected_areas.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: Some(item.currently_affected_connection_points),
        reason: item.description.clone().or_else(|| Some(item.status.clone())),
        started_at,
        estimated_end_at,
        observed_at: now,
    })
}

pub struct MalarenergiAdapter {
    client: reqwest::Client,
}

impl MalarenergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn fetch_type(&self, outage_type: &str) -> anyhow::Result<Vec<OutageItem>> {
        let response: OutagesResponse = self
            .client
            .get(BASE_URL)
            .query(&[("type", outage_type), ("take", &PAGE_SIZE.to_string()), ("skip", "0")])
            .send()
            .await?
            .json()
            .await?;
        Ok(response.items)
    }
}

impl Default for MalarenergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for MalarenergiAdapter {
    fn name(&self) -> &'static str {
        "malarenergi"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let now = Utc::now();
        let mut items = self.fetch_type("Ongoing").await?;
        items.extend(self.fetch_type("Planned").await?);
        Ok(items.iter().filter_map(|i| to_event(i, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ongoing_type_is_fault() {
        let item = OutageItem {
            outage_id: 1,
            affected_areas: "Test".into(),
            currently_affected_connection_points: 5,
            start_time: "2026-09-02T07:00:00".into(),
            end_time: None,
            outage_type: "Ongoing".into(),
            status: "Fault".into(),
            utility: "Electricity".into(),
            description: None,
        };
        assert_eq!(map_status(&item, Utc::now(), None), OutageStatus::Fault);
    }

    #[test]
    fn future_planned_is_upcoming() {
        let now = Utc::now();
        let future = now + chrono::Duration::days(1);
        let item = OutageItem {
            outage_id: 1,
            affected_areas: "Test".into(),
            currently_affected_connection_points: 5,
            start_time: "irrelevant".into(),
            end_time: None,
            outage_type: "Planned".into(),
            status: "Maintenance".into(),
            utility: "Electricity".into(),
            description: None,
        };
        assert_eq!(map_status(&item, now, Some(future)), OutageStatus::Upcoming);
    }

    #[test]
    fn non_electricity_is_filtered_out() {
        let item = OutageItem {
            outage_id: 1,
            affected_areas: "Test".into(),
            currently_affected_connection_points: 5,
            start_time: "2026-09-02T07:00:00".into(),
            end_time: None,
            outage_type: "Ongoing".into(),
            status: "Fault".into(),
            utility: "DistrictHeating".into(),
            description: None,
        };
        assert!(to_event(&item, Utc::now()).is_none());
    }

    #[test]
    fn real_planned_fixture_parses() {
        let json = include_str!("../tests/fixtures/planned_real_sample.json");
        let response: OutagesResponse = serde_json::from_str(json).unwrap();
        assert!(!response.items.is_empty());
        // The captured sample is all DistrictHeating, so should all be
        // filtered out - this just confirms parsing + filtering doesn't
        // panic on the real shape.
        let now = Utc::now();
        let events: Vec<_> = response.items.iter().filter_map(|i| to_event(i, now)).collect();
        assert_eq!(events.len(), 0);
    }
}
