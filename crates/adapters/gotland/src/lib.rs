//! Gotlands Energi (GEAB) adapter.
//!
//! Unlike every other source in this workspace, this isn't a
//! purpose-built outage-map product - GEAB's public "Avbrottskarta" is an
//! Esri ArcGIS Experience Builder app pointing straight at their internal
//! network work-order layer, exposed as a public (no-auth) ArcGIS Feature
//! Service:
//!
//!   https://services7.arcgis.com/0S7jo8hScZjFyvuM/arcgis/rest/services/Outages_view/FeatureServer/0
//!
//! Found by pulling the Experience Builder app's config
//! (`{portal}/sharing/rest/content/items/{itemId}/data`) to get its web
//! map's item id, then that web map's own data to find the operational
//! layer URLs.
//!
//! The schema is a real internal work-order system, not a clean
//! outage API - field names read like a Swedish DMS/OMS export:
//! - `type_txt`: "Driftavbrott" (unplanned operational disruption) or
//!   "Koppling" (a planned switching operation) - this is what actually
//!   distinguishes Fault from Planned, not `state_txt`.
//! - `state_txt`: GEAB's own workflow state ("Pågående", "Godkänt",
//!   "Kräver uppföljning", ...) - an internal process label, not a clean
//!   3-state outage taxonomy, so this adapter ignores it in favor of
//!   `type_txt` + actual customer counts.
//! - `num_ns` / `num_ab`: likely "nedsläckta" (currently blacked out) and
//!   "berörda" (affected) customer counts - `num_ab` is used as the
//!   reported customer count since it appeared to be the broader of the
//!   two in samples seen.
//! - Coordinates are Web Mercator (EPSG:3857), not WGS84 - converted here.
//!
//! Because this is an internal system incidentally made public rather
//! than a designed-for-the-public API, field meanings are inferred from
//! sample data rather than documentation - flagged wherever that's true.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const QUERY_URL: &str = "https://services7.arcgis.com/0S7jo8hScZjFyvuM/arcgis/rest/services/Outages_view/FeatureServer/0/query?where=1=1&outFields=*&f=json";

#[derive(Debug, Deserialize)]
struct QueryResponse {
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
struct Feature {
    attributes: Attributes,
    geometry: Option<Geometry>,
}

#[derive(Debug, Deserialize)]
struct Geometry {
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct Attributes {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "type_txt")]
    type_txt: Option<String>,
    starttime: Option<i64>,
    timelimit: Option<i64>,
    num_ns: Option<i64>,
    num_ab: Option<i64>,
    eventid: i64,
}

/// Converts Web Mercator (EPSG:3857) meters to WGS84 (lat, lng) degrees.
fn web_mercator_to_lat_lng(x: f64, y: f64) -> (f64, f64) {
    const ORIGIN_SHIFT: f64 = 20037508.342789244;
    let lng = (x / ORIGIN_SHIFT) * 180.0;
    let lat_rad_component = (y / ORIGIN_SHIFT) * 180.0;
    let lat = 180.0 / std::f64::consts::PI
        * (2.0 * (lat_rad_component * std::f64::consts::PI / 180.0).exp().atan() - std::f64::consts::PI / 2.0);
    (lat, lng)
}

fn epoch_millis_to_datetime(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

fn to_event(f: &Feature, now: DateTime<Utc>) -> Option<RawOutageEvent> {
    let attrs = &f.attributes;
    let affected = attrs.num_ab.filter(|n| *n > 0).or(attrs.num_ns).unwrap_or(0);
    let is_unplanned = attrs.type_txt.as_deref() == Some("Driftavbrott");
    let started_at = attrs.starttime.and_then(epoch_millis_to_datetime);

    let status = if affected > 0 {
        if is_unplanned {
            OutageStatus::Fault
        } else {
            OutageStatus::Planned
        }
    } else if started_at.is_some_and(|s| s > now) {
        OutageStatus::Upcoming
    } else {
        // Zero current impact and not scheduled in the future - a closed
        // work order still sitting in the feed (e.g. GEAB's own
        // "Kräver uppföljning" follow-up state). Nothing to report.
        return None;
    };

    let (lat, lng) = f
        .geometry
        .as_ref()
        .map(|g| web_mercator_to_lat_lng(g.x, g.y))
        .map(|(lat, lng)| (Some(lat), Some(lng)))
        .unwrap_or((None, None));

    Some(RawOutageEvent {
        provider: Provider::Gotland,
        source_id: attrs.eventid.to_string(),
        status,
        area_label: attrs.name.clone().unwrap_or_else(|| format!("Arbete #{}", attrs.eventid)),
        lat,
        lng,
        polygon: None,
        affected_customers: Some(affected as i32),
        reason: attrs.description.clone().filter(|d| !d.trim().is_empty()),
        started_at,
        estimated_end_at: attrs.timelimit.and_then(epoch_millis_to_datetime),
        observed_at: now,
    })
}

pub struct GotlandAdapter {
    client: reqwest::Client,
}

impl GotlandAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for GotlandAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for GotlandAdapter {
    fn name(&self) -> &'static str {
        "gotland"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body: QueryResponse = self.client.get(QUERY_URL).send().await?.json().await?;
        let now = Utc::now();
        Ok(body.features.iter().filter_map(|f| to_event(f, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_mercator_conversion_is_accurate() {
        // Stockholm, roughly - known reference point.
        let (lat, lng) = web_mercator_to_lat_lng(2003751.0, 8380038.0);
        assert!((lat - 59.5).abs() < 0.5, "lat was {lat}");
        assert!((lng - 18.0).abs() < 0.5, "lng was {lng}");
    }

    fn feature(type_txt: &str, num_ab: i64, num_ns: i64, starttime_offset_days: i64) -> Feature {
        let start = Utc::now() + chrono::Duration::days(starttime_offset_days);
        Feature {
            attributes: Attributes {
                name: Some("Test".into()),
                description: Some("beskrivning".into()),
                type_txt: Some(type_txt.into()),
                starttime: Some(start.timestamp_millis()),
                timelimit: None,
                num_ns: Some(num_ns),
                num_ab: Some(num_ab),
                eventid: 1,
            },
            geometry: Some(Geometry { x: 2003751.0, y: 8380038.0 }),
        }
    }

    #[test]
    fn unplanned_type_with_impact_is_fault() {
        let f = feature("Driftavbrott", 5, 5, -1);
        let event = to_event(&f, Utc::now()).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.affected_customers, Some(5));
    }

    #[test]
    fn switching_type_with_impact_is_planned() {
        let f = feature("Koppling", 4, 1, -1);
        let event = to_event(&f, Utc::now()).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn future_scheduled_zero_impact_is_upcoming() {
        let f = feature("Koppling", 0, 0, 5);
        let event = to_event(&f, Utc::now()).unwrap();
        assert_eq!(event.status, OutageStatus::Upcoming);
    }

    #[test]
    fn past_zero_impact_is_skipped() {
        let f = feature("Koppling", 0, 0, -5);
        assert!(to_event(&f, Utc::now()).is_none());
    }

    #[test]
    fn real_fixture_parses_without_panicking() {
        let json_str = include_str!("../tests/fixtures/real_sample.json");
        let body: QueryResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(body.features.len(), 5);
        let now = Utc::now();
        let events: Vec<_> = body.features.iter().filter_map(|f| to_event(f, now)).collect();
        // At least the one real "Pågående"/"Driftavbrott"-or-"Koppling"
        // row with nonzero num_ab/num_ns should survive.
        assert!(!events.is_empty());
    }
}
