//! Generic adapter for the Tekla/GeoServer outage map product (the same
//! system Öresundskraft uses - see that crate's module docs for how the
//! endpoints were originally reverse engineered). Confirmed also used by
//! **Sundsvall Elnät** and **Gävle Energi** (2026-09), each with their own
//! `geoserver-api` origin and bounding box but identical endpoint shapes.
//!
//! Unlike Öresundskraft's outages (all in one `scopes.p`), some
//! deployments (Gävle) split outages across multiple scope keys (`p`, `h`,
//! ...) - this adapter merges outages and areas from every scope key it
//! finds rather than assuming a fixed key, so it works for both shapes.
//!
//! One binary, one container per company (like the Digpro family) - which
//! company to poll, and its bounding box, comes entirely from env vars in
//! `main.rs`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;
use std::collections::HashMap;

const ZOOM: u32 = 10;

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

fn bounding_tiles(lat_min: f64, lat_max: f64, lng_min: f64, lng_max: f64) -> String {
    let f = tile_nr(lat_min, lng_min, ZOOM);
    let l = tile_nr(lat_max, lng_max, ZOOM);
    let n = 1i64 << ZOOM;
    let rows = ((f - l).abs() as f64 / n as f64) as i64 + 1;
    let cols = (tile_nr(lat_min, lng_min, ZOOM) - tile_nr(lat_min, lng_max, ZOOM)).abs() + 1;
    let base_row = f.min(l) / n;
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
    scopes: HashMap<String, ScopeData>,
}

#[derive(Debug, Deserialize)]
struct ScopeData {
    #[serde(default)]
    areas: Vec<AreaAgg>,
    #[serde(default)]
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
            tracing::warn!(t = other, "unrecognized Tekla outage type, treating as fault");
            OutageStatus::Fault
        }
    }
}

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
            "pa" => area_points.push((point.x, point.y, ctx.id)),
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
        .map(|(x, y, id)| ((x - coord.0).powi(2) + (y - coord.1).powi(2), id))
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .and_then(|(_, id)| area_labels.get(id))
        .map(|s| s.as_str())
}

fn to_event(
    provider: Provider,
    outage: &OutageRecord,
    index: &TileIndex,
    area_labels: &HashMap<i64, String>,
) -> RawOutageEvent {
    let coord = index.outage_coords.get(&outage.id).copied();
    let area_label = coord
        .and_then(|c| nearest_area_label(c, index, area_labels))
        .unwrap_or("Okänt område")
        .to_string();

    let started_at = epoch_ms(outage.starttime).or_else(|| epoch_ms(outage.plannedstart));
    let estimated_end_at = epoch_ms(outage.estendtime).or_else(|| epoch_ms(outage.plannedend));

    RawOutageEvent {
        provider,
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

pub struct TeklaAdapter {
    client: reqwest::Client,
    provider: Provider,
    adapter_name: &'static str,
    base_url: String,
    tiles_param: String,
}

impl TeklaAdapter {
    pub fn new(
        provider: Provider,
        adapter_name: &'static str,
        base_url: &str,
        lat_min: f64,
        lat_max: f64,
        lng_min: f64,
        lng_max: f64,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
            provider,
            adapter_name,
            base_url: base_url.trim_end_matches('/').to_string(),
            tiles_param: bounding_tiles(lat_min, lat_max, lng_min, lng_max),
        }
    }
}

#[async_trait]
impl Adapter for TeklaAdapter {
    fn name(&self) -> &'static str {
        self.adapter_name
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let appdata_raw: String = self
            .client
            .get(format!("{}/geoserver-api/GetApplicationData", self.base_url))
            .send()
            .await?
            .json()
            .await?;
        let appdata: AppData = serde_json::from_str(&appdata_raw)?;

        let tiles: TilesResponse = self
            .client
            .get(format!(
                "{}/geoserver-api/GetObjectsByTiles?zoom={ZOOM}&tiles={}",
                self.base_url, self.tiles_param
            ))
            .send()
            .await?
            .json()
            .await?;

        let index = build_tile_index(&tiles);

        let mut area_labels: HashMap<i64, String> = HashMap::new();
        let mut outages: Vec<&OutageRecord> = Vec::new();
        for scope in appdata.scopes.values() {
            for area in &scope.areas {
                area_labels.insert(area.id, area.label.clone());
            }
            outages.extend(scope.outages.iter());
        }

        Ok(outages.iter().map(|o| to_event(self.provider, o, &index, &area_labels)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_tiles_matches_oresundskraft_reference() {
        // Same bbox we hand-derived and verified for Öresundskraft - if
        // this generic version disagrees, the tile math regressed.
        let tiles = bounding_tiles(55.9578323, 56.3934555, 12.5818729, 13.0318556);
        assert_eq!(tiles, "324131-324133,325155-325157,326179-326181");
    }

    #[test]
    fn maps_known_type_codes() {
        assert_eq!(map_status("f"), OutageStatus::Fault);
        assert_eq!(map_status("p"), OutageStatus::Planned);
        assert_eq!(map_status("u"), OutageStatus::Upcoming);
    }

    #[test]
    fn merges_outages_across_multiple_scope_keys() {
        let json = r#"{
            "scopes": {
                "p": {"areas": [{"id": 1, "label": "A"}], "outages": [{"id": 10, "cc": 5, "reason": null, "starttime": 0, "plannedstart": 0, "plannedend": 0, "estendtime": 0, "t": "u"}]},
                "h": {"areas": [{"id": 2, "label": "B"}], "outages": [{"id": 20, "cc": 3, "reason": null, "starttime": 0, "plannedstart": 0, "plannedend": 0, "estendtime": 0, "t": "f"}]}
            }
        }"#;
        let appdata: AppData = serde_json::from_str(json).unwrap();
        let mut area_labels = HashMap::new();
        let mut outages = Vec::new();
        for scope in appdata.scopes.values() {
            for area in &scope.areas {
                area_labels.insert(area.id, area.label.clone());
            }
            outages.extend(scope.outages.iter());
        }
        assert_eq!(outages.len(), 2, "should merge outages from both 'p' and 'h' scopes");
        assert_eq!(area_labels.len(), 2);
    }
}
