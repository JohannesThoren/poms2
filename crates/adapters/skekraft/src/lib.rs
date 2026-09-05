//! Skellefteå Kraft adapter.
//!
//! Their outage map (driftinfo.skekraft.se) is an older AngularJS app whose
//! `scripts/app/app.config.js` plainly states the API host in a config
//! object (other commented-out lines there suggest the vendor product is
//! called "driftinfo.se" - possibly shared with other utilities, not yet
//! confirmed):
//!
//!   https://driftinfo3-api.skekraft.se/api/disturbances
//!
//! Open, unauthenticated GET. Returns the most recent ~100 disturbances
//! regardless of status (sorted by recency, not filtered to "active") - no
//! query parameter for "active only" was found, so this adapter fetches
//! the default page and filters out `Fixed == true` records itself.
//! Since an actually-ongoing outage is by definition recent, it will
//! always be within that default page.
//!
//! No named-area field in the response, only coordinates (a polygon, plus
//! sometimes a single point) - same gap as Kraftringen/Digpro, same
//! fallback (`Avbrott #<id>`).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const DISTURBANCES_URL: &str = "https://driftinfo3-api.skekraft.se/api/disturbances";

#[derive(Debug, Deserialize)]
struct DisturbancesResponse {
    results: Vec<Disturbance>,
}

#[derive(Debug, Deserialize)]
struct Disturbance {
    #[serde(rename = "DisturbanceId")]
    disturbance_id: i64,
    #[serde(rename = "AffectedCustomers")]
    affected_customers: i32,
    #[serde(rename = "Fixed")]
    fixed: bool,
    #[serde(rename = "TypeString")]
    type_string: String,
    #[serde(rename = "StartDate")]
    start_date: String,
    #[serde(rename = "PlannedStopDate")]
    planned_stop_date: String,
    #[serde(rename = "PointLatLng")]
    point_lat_lng: Option<LatLng>,
    #[serde(rename = "CoordinatesLatLng")]
    coordinates_lat_lng: Vec<LatLng>,
    #[serde(rename = "Description")]
    description: String,
}

#[derive(Debug, Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

/// .NET's `DateTime.MinValue` serializes as this - treat it as "not set"
/// rather than a real date.
fn parse_stockholm_time(s: &str) -> Option<DateTime<Utc>> {
    if s.trim().is_empty() || s.starts_with("0001-01-01") {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn polygon_centroid(points: &[LatLng]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let lat = points.iter().map(|p| p.lat).sum::<f64>() / n;
    let lng = points.iter().map(|p| p.lng).sum::<f64>() / n;
    Some((lat, lng))
}

/// No documented status legend was found for this feed, so this maps on
/// the fields that are self-explanatory (`Fixed`, `TypeString`, and
/// whether the start time is still in the future) rather than the opaque
/// numeric `Status` code - adjust here if a future disturbance reveals a
/// status this heuristic gets wrong.
fn map_status(disturbance: &Disturbance, now: DateTime<Utc>, started_at: Option<DateTime<Utc>>) -> OutageStatus {
    if disturbance.fixed {
        return OutageStatus::Resolved;
    }
    if disturbance.type_string.starts_with("Planerat") {
        return match started_at {
            Some(start) if start > now => OutageStatus::Upcoming,
            _ => OutageStatus::Planned,
        };
    }
    OutageStatus::Fault
}

fn to_event(d: &Disturbance, now: DateTime<Utc>) -> RawOutageEvent {
    let started_at = parse_stockholm_time(&d.start_date);
    let coord = d
        .point_lat_lng
        .as_ref()
        .map(|p| (p.lat, p.lng))
        .or_else(|| polygon_centroid(&d.coordinates_lat_lng));
    let polygon = (d.coordinates_lat_lng.len() >= 3)
        .then(|| d.coordinates_lat_lng.iter().map(|p| (p.lat, p.lng)).collect());

    RawOutageEvent {
        provider: Provider::Skekraft,
        source_id: d.disturbance_id.to_string(),
        status: map_status(d, now, started_at),
        area_label: format!("Avbrott #{}", d.disturbance_id),
        lat: coord.map(|(lat, _)| lat),
        lng: coord.map(|(_, lng)| lng),
        polygon,
        affected_customers: Some(d.affected_customers),
        reason: (!d.description.trim().is_empty()).then(|| d.description.clone()),
        started_at,
        estimated_end_at: parse_stockholm_time(&d.planned_stop_date),
        observed_at: now,
    }
}

pub struct SkekraftAdapter {
    client: reqwest::Client,
}

impl SkekraftAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for SkekraftAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for SkekraftAdapter {
    fn name(&self) -> &'static str {
        "skekraft"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let response: DisturbancesResponse = self.client.get(DISTURBANCES_URL).send().await?.json().await?;
        let now = Utc::now();
        Ok(response.results.iter().map(|d| to_event(d, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_disturbance_is_resolved_regardless_of_type() {
        let d = Disturbance {
            disturbance_id: 1,
            affected_customers: 5,
            fixed: true,
            type_string: "Driftstörning".into(),
            start_date: "2026-08-18T03:55:00".into(),
            planned_stop_date: "0001-01-01T00:00:00".into(),
            point_lat_lng: None,
            coordinates_lat_lng: vec![],
            description: String::new(),
        };
        assert_eq!(map_status(&d, Utc::now(), None), OutageStatus::Resolved);
    }

    #[test]
    fn unfixed_fault_type_is_fault() {
        let d = Disturbance {
            disturbance_id: 1,
            affected_customers: 5,
            fixed: false,
            type_string: "Driftstörning".into(),
            start_date: "2026-08-18T03:55:00".into(),
            planned_stop_date: "0001-01-01T00:00:00".into(),
            point_lat_lng: None,
            coordinates_lat_lng: vec![],
            description: String::new(),
        };
        assert_eq!(map_status(&d, Utc::now(), None), OutageStatus::Fault);
    }

    #[test]
    fn future_planned_start_is_upcoming() {
        let now = Utc::now();
        let future = now + chrono::Duration::days(1);
        let d = Disturbance {
            disturbance_id: 1,
            affected_customers: 5,
            fixed: false,
            type_string: "Planerat avbrott".into(),
            start_date: "irrelevant".into(),
            planned_stop_date: "0001-01-01T00:00:00".into(),
            point_lat_lng: None,
            coordinates_lat_lng: vec![],
            description: String::new(),
        };
        assert_eq!(map_status(&d, now, Some(future)), OutageStatus::Upcoming);
    }

    #[test]
    fn past_planned_start_is_planned() {
        let now = Utc::now();
        let past = now - chrono::Duration::hours(1);
        let d = Disturbance {
            disturbance_id: 1,
            affected_customers: 5,
            fixed: false,
            type_string: "Planerat avbrott".into(),
            start_date: "irrelevant".into(),
            planned_stop_date: "0001-01-01T00:00:00".into(),
            point_lat_lng: None,
            coordinates_lat_lng: vec![],
            description: String::new(),
        };
        assert_eq!(map_status(&d, now, Some(past)), OutageStatus::Planned);
    }

    #[test]
    fn min_value_date_parses_as_none() {
        assert_eq!(parse_stockholm_time("0001-01-01T00:00:00"), None);
    }

    #[test]
    fn point_lat_lng_preferred_over_polygon_centroid() {
        let d = Disturbance {
            disturbance_id: 1,
            affected_customers: 5,
            fixed: true,
            type_string: "Driftstörning".into(),
            start_date: "0001-01-01T00:00:00".into(),
            planned_stop_date: "0001-01-01T00:00:00".into(),
            point_lat_lng: Some(LatLng { lat: 64.7, lng: 20.9 }),
            coordinates_lat_lng: vec![LatLng { lat: 0.0, lng: 0.0 }],
            description: String::new(),
        };
        let event = to_event(&d, Utc::now());
        assert_eq!(event.lat, Some(64.7));
        assert_eq!(event.lng, Some(20.9));
    }

    #[test]
    fn real_fixture_parses() {
        let json = include_str!("../tests/fixtures/real_sample.json");
        let response: DisturbancesResponse = serde_json::from_str(json).unwrap();
        assert!(!response.results.is_empty());
        let now = Utc::now();
        for d in &response.results {
            let event = to_event(d, now);
            assert_eq!(event.provider, Provider::Skekraft);
        }
    }
}
