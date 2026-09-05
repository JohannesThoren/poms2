//! Tekniska verken adapter.
//!
//! Their outage map (avbrott.tekniskaverken.se) is an Angular app whose
//! server-injected config (`ng-state` in the page source) reveals the real
//! API host: `api.tekniskaverken.net`. Most routes there sit behind Azure
//! AD auth (401 with a `WWW-Authenticate: Bearer ...` challenge) - but
//! `/outage/v1/public/outages` is a deliberately anonymous route and
//! returns the full outage list. No key, no brand param needed on the
//! request itself (the response already carries a `brand` field per
//! record, which we filter on defensively in case the endpoint ever
//! starts serving other brands on the same platform, e.g. "Mse").
//!
//! This is the richest feed of the adapters so far: proper ISO 8601
//! timestamps with UTC offsets already included (no DST guessing needed,
//! unlike Vattenfall/Kraftringen), multiple named districts per outage,
//! and separate customer/site/connection-point counts. It covers more
//! than electricity (district heating too) - we filter to
//! `utility == "Electricity"` since that's this system's scope.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const OUTAGES_URL: &str = "https://api.tekniskaverken.net/outage/v1/public/outages";
const BRAND: &str = "TekniskaVerken";

#[derive(Debug, Deserialize)]
struct OutagesResponse {
    outages: Vec<Outage>,
}

#[derive(Debug, Deserialize)]
struct Outage {
    #[serde(rename = "outageId")]
    outage_id: String,
    brand: String,
    status: String,
    planned: bool,
    utility: String,
    cause: Option<String>,
    districts: Vec<District>,
    #[serde(rename = "affectedCustomers")]
    affected_customers: Option<i32>,
    #[serde(rename = "startedTime")]
    started_time: Option<String>,
    #[serde(rename = "plannedStartTime")]
    planned_start_time: Option<String>,
    #[serde(rename = "estimatedCompletionTime")]
    estimated_completion_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct District {
    city: String,
    name: String,
}

fn parse_rfc3339(s: &Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn map_status(status: &str, planned: bool) -> OutageStatus {
    match (status, planned) {
        (_, _) if status.eq_ignore_ascii_case("completed") => OutageStatus::Resolved,
        ("Scheduled", true) => OutageStatus::Upcoming,
        (_, true) => OutageStatus::Planned,
        (_, false) => OutageStatus::Fault,
    }
}

fn area_label(districts: &[District]) -> String {
    if districts.is_empty() {
        return "Okänt område".to_string();
    }
    let names: Vec<&str> = districts.iter().map(|d| d.name.as_str()).collect();
    let city = &districts[0].city;
    format!("{}, {}", names.join(", "), city)
}

fn to_event(outage: &Outage) -> Option<RawOutageEvent> {
    if outage.utility != "Electricity" || outage.brand != BRAND {
        return None;
    }

    let started_at = parse_rfc3339(&outage.started_time).or_else(|| parse_rfc3339(&outage.planned_start_time));
    let estimated_end_at = parse_rfc3339(&outage.estimated_completion_time);

    Some(RawOutageEvent {
        provider: Provider::TekniskaVerken,
        source_id: outage.outage_id.clone(),
        status: map_status(&outage.status, outage.planned),
        area_label: area_label(&outage.districts),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: outage.affected_customers,
        reason: outage.cause.clone(),
        started_at,
        estimated_end_at,
        observed_at: Utc::now(),
    })
}

pub struct TekniskaVerkenAdapter {
    client: reqwest::Client,
}

impl TekniskaVerkenAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for TekniskaVerkenAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for TekniskaVerkenAdapter {
    fn name(&self) -> &'static str {
        "tekniska_verken"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let response: OutagesResponse = self.client.get(OUTAGES_URL).send().await?.json().await?;
        Ok(response.outages.iter().filter_map(to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_completed_to_resolved_regardless_of_planned() {
        assert_eq!(map_status("Completed", true), OutageStatus::Resolved);
        assert_eq!(map_status("Completed", false), OutageStatus::Resolved);
    }

    #[test]
    fn maps_scheduled_planned_to_upcoming() {
        assert_eq!(map_status("Scheduled", true), OutageStatus::Upcoming);
    }

    #[test]
    fn maps_unplanned_non_completed_to_fault() {
        assert_eq!(map_status("InProgress", false), OutageStatus::Fault);
    }

    #[test]
    fn maps_planned_non_scheduled_to_planned() {
        assert_eq!(map_status("InProgress", true), OutageStatus::Planned);
    }

    #[test]
    fn parses_offset_aware_timestamp() {
        let s = Some("2026-09-01T12:45:19+02:00".to_string());
        let dt = parse_rfc3339(&s).unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-09-01T10:45:19+00:00");
    }

    #[test]
    fn filters_out_non_electricity_and_other_brands() {
        let outage = Outage {
            outage_id: "x".into(),
            brand: "Mse".into(),
            status: "Scheduled".into(),
            planned: true,
            utility: "Electricity".into(),
            cause: None,
            districts: vec![],
            affected_customers: None,
            started_time: None,
            planned_start_time: None,
            estimated_completion_time: None,
        };
        assert!(to_event(&outage).is_none());

        let heating = Outage { utility: "DistrictHeating".into(), brand: BRAND.into(), ..outage };
        assert!(to_event(&heating).is_none());
    }

    #[test]
    fn real_shaped_electricity_outage_converts() {
        let json = r#"{
            "outageId": "f6b7a538-0b8b-461f-e084-08df07267731",
            "brand": "TekniskaVerken",
            "status": "Completed",
            "planned": false,
            "utility": "Electricity",
            "startedTime": "2026-09-01T12:45:19+02:00",
            "completionTime": "2026-09-01T12:48:28+02:00",
            "districts": [
                {"city": "Linköping", "county": "Ostergotland", "name": "Ekholmen", "affectedSites": 182}
            ],
            "affectedCustomers": 687
        }"#;
        let outage: Outage = serde_json::from_str(json).unwrap();
        let event = to_event(&outage).unwrap();
        assert_eq!(event.status, OutageStatus::Resolved);
        assert_eq!(event.affected_customers, Some(687));
        assert_eq!(event.area_label, "Ekholmen, Linköping");
    }

    #[test]
    fn real_fixture_file_parses_and_filters() {
        let json = include_str!("../tests/fixtures/real_sample.json");
        let response: OutagesResponse = serde_json::from_str(json).unwrap();
        assert!(!response.outages.is_empty());
        let events: Vec<_> = response.outages.iter().filter_map(to_event).collect();
        // Fixture is known to contain both Electricity and DistrictHeating -
        // only Electricity should survive the filter.
        assert!(events.len() < response.outages.len());
        assert!(events.len() > 0);
    }
}
