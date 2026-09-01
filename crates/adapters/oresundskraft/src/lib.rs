//! Öresundskraft adapter.
//!
//! Their outage map (driftinformation.oresundskraft.se) is a Tekla/GeoServer-
//! based system - see the module-level research notes in this repo's
//! history for how it was reverse engineered. Two open, unauthenticated
//! endpoints are combined:
//!
//! - `GetApplicationData`: individual outage records (id, customer count,
//!   reason, planned/estimated/actual times as epoch milliseconds, and a
//!   type code `f`/`p`/`u` for fault/planned/upcoming) - but no
//!   coordinates or place names.
//! - `GetObjectsByTiles`: map markers with real coordinates, keyed by the
//!   same outage id (`oid`) - including area label markers (`pa`) with
//!   their own ids matching `GetApplicationData`'s area list.
//!
//! We join outage records to coordinates via `oid`, and label each outage
//! with the name of whichever known area centroid is geographically
//! closest to it (nearest-neighbor on lat/lng - Öresundskraft's whole
//! service area is only ~50km across, so simple Euclidean distance in
//! degrees is more than accurate enough to pick the right locality).
//!
//! The tile list passed to `GetObjectsByTiles` is derived once from the
//! service's fixed lat/lng bounding box (see [`bounding_tiles`]) rather
//! than hardcoded, so it keeps working if their coverage area ever grows.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;
use std::collections::HashMap;

const BASE_URL: &str = "https://driftinformation.oresundskraft.se/OutageMap/geoserver-api";
const ZOOM: u32 = 10;

// Öresundskraft's fixed service-area bounding box, read from this system's
// own `configuration.js` (the `generated.mapBounds` object).
const LAT_MIN: f64 = 55.9578323;
const LAT_MAX: f64 = 56.3934555;
const LNG_MIN: f64 = 12.5818729;
const LNG_MAX: f64 = 13.0318556;

fn long2tile(lng: f64, z: u32) -> i64 {
    ((lng + 180.0) / 360.0 * 2f64.powi(z as i32)).floor() as i64
}

fn lat2tile(lat: f64, z: u32) -> i64 {
    let rad = lat.to_radians();
    ((1.0 - (rad.tan() + 1.0 / rad.cos()).ln() / std::f64::consts::PI) / 2.0 * 2f64.powi(z as i32)).floor() as i64
}

fn tile_nr(lat: f64, lng: f64, z: u32) -> i64 {
    (1i64 << z) * lat2tile(lat, z) + long2tile(lng, z)
}

/// Compresses a sorted list of tile numbers into ranges ("a-b") where at
/// least 3 are consecutive, matching the compression scheme the map's own
/// JS client uses when building its `tiles=` query parameter.
fn compress(mut tiles: Vec<i64>) -> Vec<String> {
    tiles.sort_unstable();
    if tiles.len() < 3 {
        return tiles.iter().map(|t| t.to_string()).collect();
    }
    let mut out = Vec::new();
    let mut r = 0usize;
    let mut i = 1usize;
    while i <= tiles.len() {
        while i < tiles.len() && tiles[i] - tiles[i - 1] == 1 {
            i += 1;
        }
        if i - r >= 3 {
            out.push(format!("{}-{}", tiles[r], tiles[i - 1]));
        } else {
            out.push(tiles[r].to_string());
            if i - r == 2 {
                out.push(tiles[i - 1].to_string());
            }
        }
        r = i;
        i += 1;
    }
    out
}

/// Every tile at [`ZOOM`] covering the service's bounding box, compressed
/// into the `tiles=` query param format `GetObjectsByTiles` expects.
fn bounding_tiles() -> String {
    let f = tile_nr(LAT_MIN, LNG_MIN, ZOOM);
    let l = tile_nr(LAT_MAX, LNG_MAX, ZOOM);
    let n = 1i64 << ZOOM;
    let rows = ((f - l).abs() as f64 / n as f64) as i64 + 1;
    let cols = (tile_nr(LAT_MIN, LNG_MIN, ZOOM) - tile_nr(LAT_MIN, LNG_MAX, ZOOM)).abs() + 1;
    let base_row = f.min(l) / n;
    // Deliberately the min of the two remainders (not the remainder of the
    // min) - f and l sit in different rows and columns, so this matches
    // the reference JS client's arithmetic exactly rather than a
    // seemingly-equivalent simplification that shifts the result.
    let base_col = (f.rem_euclid(n)).min(l.rem_euclid(n));

    let mut tiles = Vec::new();
    for row in base_row..(base_row + rows) {
        for col in base_col..(base_col + cols) {
            tiles.push(n * row + col);
        }
    }
    compress(tiles).join(",")
}

#[derive(Debug, Deserialize)]
struct AppData {
    scopes: Scopes,
}

#[derive(Debug, Deserialize)]
struct Scopes {
    p: PScope,
}

#[derive(Debug, Deserialize)]
struct PScope {
    areas: Vec<AreaAgg>,
    outages: Vec<OutageRecord>,
}

#[derive(Debug, Deserialize)]
struct AreaAgg {
    id: i64,
    label: String,
}

#[derive(Debug, Deserialize)]
struct OutageRecord {
    id: i64,
    cc: i32,
    reason: Option<String>,
    /// Epoch milliseconds, 0 if not yet started.
    starttime: i64,
    plannedstart: i64,
    plannedend: i64,
    estendtime: i64,
    t: String,
}

#[derive(Debug, Deserialize)]
struct TilesResponse {
    points: Vec<TilePoint>,
}

#[derive(Debug, Deserialize)]
struct TilePoint {
    context: String,
    x: f64,
    y: f64,
}

#[derive(Debug, Deserialize)]
struct PointContext {
    t: String,
    #[serde(default)]
    oid: i64,
    #[serde(default)]
    us: Vec<BundledOutage>,
    /// Present on `pa` (area label) points - matches an [`AreaAgg`] id.
    #[serde(rename = "_id", default)]
    id: i64,
}

#[derive(Debug, Deserialize)]
struct BundledOutage {
    oid: i64,
}

fn epoch_ms(ms: i64) -> Option<DateTime<Utc>> {
    if ms <= 0 {
        return None;
    }
    DateTime::from_timestamp_millis(ms)
}

fn map_status(t: &str) -> OutageStatus {
    match t {
        "f" => OutageStatus::Fault,
        "p" => OutageStatus::Planned,
        "u" => OutageStatus::Upcoming,
        other => {
            tracing::warn!(t = other, "unrecognized Öresundskraft outage type, treating as fault");
            OutageStatus::Fault
        }
    }
}

/// Coordinates and area-label lookups built from a `GetObjectsByTiles`
/// response: outage id -> (lon, lat), and area id -> (lon, lat, label) for
/// nearest-neighbor labeling.
struct TileIndex {
    outage_coords: HashMap<i64, (f64, f64)>,
    area_points: Vec<(f64, f64, i64)>,
}

fn build_tile_index(tiles: &TilesResponse) -> TileIndex {
    let mut outage_coords = HashMap::new();
    let mut area_points = Vec::new();

    for point in &tiles.points {
        let Ok(ctx) = serde_json::from_str::<PointContext>(&point.context) else { continue };
        match ctx.t.as_str() {
            "po" => {
                if ctx.oid != 0 {
                    outage_coords.insert(ctx.oid, (point.x, point.y));
                }
                for bundled in &ctx.us {
                    outage_coords.insert(bundled.oid, (point.x, point.y));
                }
            }
            "pa" => {
                area_points.push((point.x, point.y, ctx.id));
            }
            _ => {}
        }
    }

    TileIndex { outage_coords, area_points }
}

fn nearest_area_label<'a>(
    coord: (f64, f64),
    index: &TileIndex,
    area_labels: &'a HashMap<i64, String>,
) -> Option<&'a str> {
    index
        .area_points
        .iter()
        .map(|(x, y, id)| {
            let d = (x - coord.0).powi(2) + (y - coord.1).powi(2);
            (d, id)
        })
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .and_then(|(_, id)| area_labels.get(id))
        .map(|s| s.as_str())
}

fn to_event(outage: &OutageRecord, index: &TileIndex, area_labels: &HashMap<i64, String>) -> RawOutageEvent {
    let coord = index.outage_coords.get(&outage.id).copied();
    let area_label = coord
        .and_then(|c| nearest_area_label(c, index, area_labels))
        .unwrap_or("Okänt område")
        .to_string();

    let started_at = epoch_ms(outage.starttime).or_else(|| epoch_ms(outage.plannedstart));
    let estimated_end_at = epoch_ms(outage.estendtime).or_else(|| epoch_ms(outage.plannedend));

    RawOutageEvent {
        provider: Provider::Oresundskraft,
        source_id: outage.id.to_string(),
        status: map_status(&outage.t),
        area_label,
        lat: coord.map(|(_, lat)| lat),
        lng: coord.map(|(lon, _)| lon),
        affected_customers: Some(outage.cc),
        reason: outage.reason.clone(),
        started_at,
        estimated_end_at,
        observed_at: Utc::now(),
    }
}

pub struct OresundskraftAdapter {
    client: reqwest::Client,
    tiles_param: String,
}

impl OresundskraftAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
            tiles_param: bounding_tiles(),
        }
    }
}

impl Default for OresundskraftAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for OresundskraftAdapter {
    fn name(&self) -> &'static str {
        "oresundskraft"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let appdata_raw: String = self
            .client
            .get(format!("{BASE_URL}/GetApplicationData"))
            .send()
            .await?
            .json()
            .await?;
        let appdata: AppData = serde_json::from_str(&appdata_raw)?;

        let tiles: TilesResponse = self
            .client
            .get(format!("{BASE_URL}/GetObjectsByTiles?zoom={ZOOM}&tiles={}", self.tiles_param))
            .send()
            .await?
            .json()
            .await?;

        let index = build_tile_index(&tiles);
        let area_labels: HashMap<i64, String> =
            appdata.scopes.p.areas.iter().map(|a| (a.id, a.label.clone())).collect();

        Ok(appdata
            .scopes
            .p
            .outages
            .iter()
            .map(|o| to_event(o, &index, &area_labels))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_tiles_covers_known_service_area() {
        // Regression check against the value derived by hand during
        // research - if this ever changes, Öresundskraft's coverage
        // bounds moved and area_points below may need rechecking too.
        let tiles = bounding_tiles();
        assert_eq!(tiles, "324131-324133,325155-325157,326179-326181");
    }

    #[test]
    fn maps_known_type_codes() {
        assert_eq!(map_status("f"), OutageStatus::Fault);
        assert_eq!(map_status("p"), OutageStatus::Planned);
        assert_eq!(map_status("u"), OutageStatus::Upcoming);
        assert_eq!(map_status("x"), OutageStatus::Fault);
    }

    #[test]
    fn epoch_zero_is_none() {
        assert_eq!(epoch_ms(0), None);
        assert!(epoch_ms(1_700_000_000_000).is_some());
    }

    #[test]
    fn joins_real_appdata_and_tiles_fixtures() {
        let appdata_raw: String = serde_json::from_str(include_str!("../tests/fixtures/real_appdata.json")).unwrap();
        let appdata: AppData = serde_json::from_str(&appdata_raw).unwrap();
        let tiles: TilesResponse = serde_json::from_str(include_str!("../tests/fixtures/real_tiles.json")).unwrap();

        let index = build_tile_index(&tiles);
        let area_labels: HashMap<i64, String> =
            appdata.scopes.p.areas.iter().map(|a| (a.id, a.label.clone())).collect();

        assert!(!appdata.scopes.p.outages.is_empty());
        assert!(!index.outage_coords.is_empty());
        assert!(!index.area_points.is_empty());

        let events: Vec<_> = appdata
            .scopes
            .p
            .outages
            .iter()
            .map(|o| to_event(o, &index, &area_labels))
            .collect();

        // Every outage in the real fixture has a corresponding tile
        // marker, so all should resolve to real coordinates and a named
        // area, not the "unknown" fallback.
        let with_coords = events.iter().filter(|e| e.lat.is_some()).count();
        assert_eq!(with_coords, events.len(), "every real outage should join to a coordinate");

        let unknown_area = events.iter().filter(|e| e.area_label == "Okänt område").count();
        assert_eq!(unknown_area, 0, "every real outage should resolve to a named area");

        for event in &events {
            assert!(event.affected_customers.unwrap() >= 0);
        }
    }
}
