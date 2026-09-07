//! Umeå Energi adapter.
//!
//! Their driftinformation page (umeaenergi.se, a Nuxt app) embeds a direct
//! link to a static JSON blob in Azure Blob Storage:
//!
//!   https://uewebstorage.blob.core.windows.net/disturbance-data-prod/disturbances.json
//!
//! Open, unauthenticated GET, no API host or key needed at all - found by
//! grepping the page's rendered HTML for `.json` rather than digging
//! through the Nuxt JS bundle. Returns every disturbance on record (67
//! at time of writing, going back to July 2026), not just active ones,
//! so this adapter fetches the whole file each poll and filters/maps
//! every record itself.
//!
//! One feed covers four utilities Umeå Energi runs, distinguished by
//! `domain`: `0` = el (the only one this project tracks), `1` = bredband
//! (fiber), `2` = fjärrvärme (district heating), `3` = fjärrkyla
//! (district cooling). Non-electricity records are dropped here.
//!
//! Unusually rich for a Swedish nätägare feed: real GeoJSON `Point` or
//! `Polygon` geometry per record (already lat/lng, no coordinate system
//! to guess at), an explicit `endDate`, and a `fromDigpro` flag showing
//! Umeå Energi sources their own el data from a Digpro backend
//! internally, even though this JSON endpoint is not itself Digpro's
//! KML API.
//!
//! **Status logic is a best-effort guess, not a documented contract** -
//! no legend for `state`/`type` was published anywhere:
//!   - `state == 2` was only ever observed on records with a future
//!     `startDate`, so it's treated as "upcoming/planned, not started".
//!   - Otherwise: `endDate` in the past -> `Resolved` (the source is one
//!     of the few that tells us this directly, so we don't have to rely
//!     purely on the ingestion service's staleness sweep); free-text
//!     `Planerat` in the message -> `Planned`; anything else -> `Fault`.
//!     `type` (0/1) did *not* correlate cleanly with planned vs.
//!     unplanned in samples checked, so it's ignored for status.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const DISTURBANCES_URL: &str =
    "https://uewebstorage.blob.core.windows.net/disturbance-data-prod/disturbances.json";

const EL_DOMAIN: i32 = 0;

#[derive(Debug, Deserialize)]
struct Disturbance {
    id: i64,
    title: Option<String>,
    message: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    domain: i32,
    state: i32,
    #[serde(rename = "affectedCount")]
    affected_count: Option<i32>,
    #[serde(rename = "geoCoordinates")]
    geo_coordinates: Option<GeoCoordinates>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum GeoCoordinates {
    Point {
        coordinates: (f64, f64),
    },
    Polygon {
        // GeoJSON Polygon: a list of linear rings, each a list of
        // [lng, lat] pairs. We only ever expect one ring (no holes) from
        // this source, so only the first ring is used.
        coordinates: Vec<Vec<(f64, f64)>>,
    },
}

/// GeoJSON coordinates are `[lng, lat]`; `RawOutageEvent` wants `(lat, lng)`.
fn point_lat_lng(coords: &GeoCoordinates) -> (Option<f64>, Option<f64>) {
    match coords {
        GeoCoordinates::Point { coordinates: (lng, lat) } => (Some(*lat), Some(*lng)),
        GeoCoordinates::Polygon { coordinates } => {
            let ring = coordinates.first();
            let ring = match ring {
                Some(r) if !r.is_empty() => r,
                _ => return (None, None),
            };
            let n = ring.len() as f64;
            let lat = ring.iter().map(|(_, lat)| lat).sum::<f64>() / n;
            let lng = ring.iter().map(|(lng, _)| lng).sum::<f64>() / n;
            (Some(lat), Some(lng))
        }
    }
}

fn polygon_vertices(coords: &GeoCoordinates) -> Option<Vec<(f64, f64)>> {
    match coords {
        GeoCoordinates::Point { .. } => None,
        GeoCoordinates::Polygon { coordinates } => coordinates
            .first()
            .filter(|ring| ring.len() >= 3)
            .map(|ring| ring.iter().map(|(lng, lat)| (*lat, *lng)).collect()),
    }
}

/// Naive local (Europe/Stockholm) timestamp, with a variable-length
/// fractional-seconds component (`.NET`'s `DateTime` serializes with
/// anywhere from 0 to 7 digits of sub-second precision).
fn parse_stockholm_time(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn status_for(d: &Disturbance, started_at: Option<DateTime<Utc>>, ended_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> OutageStatus {
    if let Some(end) = ended_at {
        if end < now {
            return OutageStatus::Resolved;
        }
    }
    if d.state == 2 {
        return OutageStatus::Upcoming;
    }
    let text = format!(
        "{} {}",
        d.title.as_deref().unwrap_or(""),
        d.message.as_deref().unwrap_or("")
    );
    if text.to_lowercase().contains("planerat") {
        return OutageStatus::Planned;
    }
    // Defensive fallback: a start time still in the future without
    // state == 2 hasn't been observed, but don't report a fault that
    // hasn't started yet if it somehow occurs.
    if let Some(start) = started_at {
        if start > now {
            return OutageStatus::Upcoming;
        }
    }
    OutageStatus::Fault
}

/// How long after `endDate` a resolved record is still worth reporting.
/// The source returns its *entire* history on every request, not just
/// active disturbances (same shape as Skellefteå Kraft's feed - see
/// `crates/adapters/skekraft/src/lib.rs`), so without a cutoff this
/// adapter would re-emit the same old resolved record on every 60s poll
/// forever, each time overwriting `outages.resolved_at` with the current
/// time (`upsert_outage` in the ingestion service always takes the
/// latest write's `resolved_at`). That would make months-old outages look
/// like they *just* resolved, permanently drowning out genuinely recent
/// ones on the "senast åtgärdade" list. A record's resolution only needs
/// reporting once, shortly after it happens - this window is comfortably
/// wider than the 60s poll interval so the transition isn't missed, but
/// short enough that ancient history gets dropped.
const RESOLVED_REPORT_WINDOW: chrono::Duration = chrono::Duration::hours(2);

pub struct UmeaEnergiAdapter {
    client: reqwest::Client,
}

impl UmeaEnergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for UmeaEnergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for UmeaEnergiAdapter {
    fn name(&self) -> &'static str {
        Provider::Umea.as_str()
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let disturbances: Vec<Disturbance> = self
            .client
            .get(DISTURBANCES_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let now = Utc::now();
        let events = disturbances
            .into_iter()
            .filter(|d| d.domain == EL_DOMAIN)
            .filter_map(|d| {
                let started_at = d.start_date.as_deref().and_then(parse_stockholm_time);
                let estimated_end_at = d.end_date.as_deref().and_then(parse_stockholm_time);
                let status = status_for(&d, started_at, estimated_end_at, now);

                // See RESOLVED_REPORT_WINDOW: drop anything resolved long
                // enough ago that re-reporting it would just be noise.
                if status == OutageStatus::Resolved {
                    if let Some(end) = estimated_end_at {
                        if now - end > RESOLVED_REPORT_WINDOW {
                            return None;
                        }
                    }
                }

                let (lat, lng) = d
                    .geo_coordinates
                    .as_ref()
                    .map(point_lat_lng)
                    .unwrap_or((None, None));
                let polygon = d.geo_coordinates.as_ref().and_then(polygon_vertices);
                let area_label = d
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| "Umeå".to_string());

                Some(RawOutageEvent {
                    provider: Provider::Umea,
                    source_id: d.id.to_string(),
                    status,
                    area_label,
                    lat,
                    lng,
                    polygon,
                    affected_customers: d.affected_count,
                    reason: d.message.clone(),
                    started_at,
                    estimated_end_at,
                    observed_at: now,
                })
            })
            .collect();

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variable_fractional_seconds() {
        assert!(parse_stockholm_time("2026-07-13T14:31:41").is_some());
        assert!(parse_stockholm_time("2026-07-09T09:53:12.548").is_some());
        assert!(parse_stockholm_time("2026-07-09T10:50:40.8714791").is_some());
        assert!(parse_stockholm_time("").is_none());
    }

    #[test]
    fn point_geometry_swaps_lng_lat_order() {
        // GeoJSON gives [lng, lat]; we want (lat, lng) out.
        let geo = GeoCoordinates::Point {
            coordinates: (20.555, 63.904),
        };
        let (lat, lng) = point_lat_lng(&geo);
        assert_eq!(lat, Some(63.904));
        assert_eq!(lng, Some(20.555));
        assert!(polygon_vertices(&geo).is_none());
    }

    #[test]
    fn polygon_geometry_centroid_and_vertices() {
        let geo = GeoCoordinates::Polygon {
            coordinates: vec![vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]],
        };
        let (lat, lng) = point_lat_lng(&geo);
        assert_eq!(lat, Some(1.0));
        assert_eq!(lng, Some(1.0));
        let vertices = polygon_vertices(&geo).unwrap();
        assert_eq!(vertices.len(), 4);
        // vertices are (lat, lng) - first ring point (0.0, 0.0) stays (0.0, 0.0)
        assert_eq!(vertices[1], (0.0, 2.0));
    }
}
