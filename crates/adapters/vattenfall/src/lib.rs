//! Vattenfall Eldistribution adapter.
//!
//! Their public outage map (arkmap.vattenfalleldistribution.se) is a small
//! SPA that itself just fetches a static-looking JSON endpoint client-side:
//!
//!   https://arkmap.vattenfalleldistribution.se/incidents.json
//!
//! No auth, no API key. It gives individual incidents (not just area
//! aggregates like Ellevio) including a lat/lng polygon of the affected
//! area, so this adapter is meaningfully richer: real coordinates (polygon
//! centroid), per-incident customer counts, and start times.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const INCIDENTS_URL: &str = "https://arkmap.vattenfalleldistribution.se/incidents.json";

#[derive(Debug, Deserialize)]
struct IncidentsResponse {
    warnings: Warnings,
}

#[derive(Debug, Deserialize)]
struct Warnings {
    warning: Vec<Incident>,
}

#[derive(Debug, Deserialize)]
struct Incident {
    id: String,
    status: i32,
    placenames: String,
    description: String,
    #[serde(rename = "affectedCustomers")]
    affected_customers: i32,
    /// Flat list of alternating [lat, lng, lat, lng, ...] vertices.
    polygon: Vec<f64>,
    #[serde(rename = "startTime")]
    start_time: String,
    #[serde(rename = "completionTime")]
    completion_time: String,
}

/// Status codes observed so far. Vattenfall doesn't publish a legend, so
/// this is built from what's actually come through the feed - unrecognized
/// codes fall back to `Fault` (the more actionable assumption for an
/// unplanned-looking event) with a warning logged, rather than silently
/// dropping the incident. Extending this table as new codes show up is a
/// one-line change, not a redesign.
fn map_status(code: i32) -> OutageStatus {
    match code {
        6 => OutageStatus::Planned,  // "Planerat avbrott pågår"
        7 => OutageStatus::Upcoming, // "Kommande planerat avbrott"
        other => {
            tracing::warn!(status_code = other, "unrecognized Vattenfall status code, treating as fault");
            OutageStatus::Fault
        }
    }
}

/// Centroid of the polygon vertices - good enough for a map pin; we don't
/// need the exact outline for this system.
fn polygon_centroid(polygon: &[f64]) -> Option<(f64, f64)> {
    if polygon.len() < 2 || polygon.len() % 2 != 0 {
        return None;
    }
    let n = (polygon.len() / 2) as f64;
    let mut lat_sum = 0.0;
    let mut lng_sum = 0.0;
    for pair in polygon.chunks(2) {
        lat_sum += pair[0];
        lng_sum += pair[1];
    }
    Some((lat_sum / n, lng_sum / n))
}

/// The polygon as (lat, lng) vertex pairs, when the flat list is
/// well-formed - `None` (not stored) rather than a degenerate 0- or
/// 1-point "polygon" if it isn't.
fn polygon_vertices(polygon: &[f64]) -> Option<Vec<(f64, f64)>> {
    if polygon.len() < 6 || polygon.len() % 2 != 0 {
        return None;
    }
    Some(polygon.chunks(2).map(|pair| (pair[0], pair[1])).collect())
}

/// Parses a "YYYY-MM-DD HH:MM" timestamp as Europe/Stockholm local time and
/// converts to UTC. Vattenfall's feed gives no timezone marker, and Sweden
/// alternates CET/CEST, so a fixed UTC+1 or +2 offset would be wrong for
/// half the year - hence chrono-tz instead of a naive fixed offset.
fn parse_stockholm_time(s: &str) -> Option<chrono::DateTime<Utc>> {
    if s.trim().is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

pub struct VattenfallAdapter {
    client: reqwest::Client,
}

impl VattenfallAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    fn to_event(incident: &Incident) -> RawOutageEvent {
        let (lat, lng) = polygon_centroid(&incident.polygon)
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or((None, None));
        let polygon = polygon_vertices(&incident.polygon);

        RawOutageEvent {
            provider: Provider::Vattenfall,
            source_id: incident.id.clone(),
            status: map_status(incident.status),
            area_label: incident.placenames.clone(),
            lat,
            lng,
            polygon,
            affected_customers: Some(incident.affected_customers),
            reason: Some(incident.description.clone()),
            started_at: parse_stockholm_time(&incident.start_time),
            estimated_end_at: parse_stockholm_time(&incident.completion_time),
            observed_at: Utc::now(),
        }
    }
}

impl Default for VattenfallAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for VattenfallAdapter {
    fn name(&self) -> &'static str {
        "vattenfall"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let response: IncidentsResponse = self
            .client
            .get(INCIDENTS_URL)
            .send()
            .await?
            .json()
            .await?;

        Ok(response.warnings.warning.iter().map(Self::to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_status_codes() {
        assert_eq!(map_status(6), OutageStatus::Planned);
        assert_eq!(map_status(7), OutageStatus::Upcoming);
        assert_eq!(map_status(99), OutageStatus::Fault);
    }

    #[test]
    fn computes_polygon_centroid() {
        let polygon = vec![0.0, 0.0, 2.0, 0.0, 2.0, 2.0, 0.0, 2.0];
        let (lat, lng) = polygon_centroid(&polygon).unwrap();
        assert_eq!(lat, 1.0);
        assert_eq!(lng, 1.0);
    }

    #[test]
    fn rejects_malformed_polygon() {
        assert_eq!(polygon_centroid(&[1.0, 2.0, 3.0]), None);
        assert_eq!(polygon_centroid(&[]), None);
    }

    #[test]
    fn parses_stockholm_summer_time_as_utc() {
        // 2026-09-01 is CEST (UTC+2).
        let dt = parse_stockholm_time("2026-09-01 22:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-09-01T20:00:00+00:00");
    }

    #[test]
    fn parses_stockholm_winter_time_as_utc() {
        // 2026-01-15 is CET (UTC+1).
        let dt = parse_stockholm_time("2026-01-15 10:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-01-15T09:00:00+00:00");
    }

    #[test]
    fn empty_completion_time_is_none() {
        assert_eq!(parse_stockholm_time(""), None);
    }

    #[test]
    fn deserializes_real_shaped_payload() {
        let json = r#"{
            "warnings": {
                "lastUpdate": "2026-09-01 22:52",
                "warning": [{
                    "id": "INCD-7605-A",
                    "status": 6,
                    "placenames": "GUSTAVSBERG",
                    "affectedAreas": [{"affected": 4, "areacode": 13430, "muni": 120}],
                    "description": "Planerat avbrott pågår",
                    "affectedCustomers": 5,
                    "polygon": [59.0, 18.0, 59.1, 18.1],
                    "freeText": "...",
                    "startTime": "2026-09-01 22:00",
                    "completionTime": ""
                }]
            }
        }"#;
        let parsed: IncidentsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.warnings.warning.len(), 1);
        let event = VattenfallAdapter::to_event(&parsed.warnings.warning[0]);
        assert_eq!(event.area_label, "GUSTAVSBERG");
        assert_eq!(event.affected_customers, Some(5));
        assert_eq!(event.status, OutageStatus::Planned);
        assert!(event.lat.is_some());
        assert!(event.estimated_end_at.is_none());
    }
}
