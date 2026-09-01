//! Kraftringen adapter.
//!
//! Their live outage map (avbrott.kraftringen.se) is a small JS app that
//! itself just fetches a static KML file hosted on Azure blob storage:
//!
//!   https://stavbrottskartan.blob.core.windows.net/xml/fpp_lnd_outages.kml
//!
//! No auth. Despite the "fpp_lnd" (Lund) name it carries Kraftringen's
//! whole service area, not just Lund - coordinates in the feed span
//! Skåne, Blekinge and beyond. Each `<Placemark>` is one outage, with a
//! `styleUrl` telling us its category (active/planned/resolved) and an
//! `<ExtendedData>` block of `name`/`value` pairs for the rest.
//!
//! There's also a companion `outages.xml` with county-level aggregate
//! counts (same shape as Ellevio's data) - not used here since the KML
//! gives us the individual, coordinate-level records directly, but it's a
//! reasonable sanity-check/fallback source if this feed ever goes away.
//!
//! One gap: the feed has no place-name field, only coordinates - so
//! `area_label` falls back to the outage id rather than a locality name
//! until a reverse-geocoding step is added.

use async_trait::async_trait;
use chrono::{NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;

const KML_URL: &str = "https://stavbrottskartan.blob.core.windows.net/xml/fpp_lnd_outages.kml";

#[derive(Debug, Default)]
pub struct RawPlacemark {
    style_url: String,
    data: HashMap<String, String>,
    /// (lon, lat) from the last `<coordinates>` block in the placemark -
    /// when a Polygon precedes a Point, the Point (the actual pin/centroid)
    /// always comes last in this feed, so taking the last block gives us
    /// the marker location rather than a polygon vertex.
    lon_lat: Option<(f64, f64)>,
}

pub fn parse_kml(xml: &str) -> anyhow::Result<Vec<RawPlacemark>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut placemarks = Vec::new();
    let mut current: Option<RawPlacemark> = None;
    let mut current_data_name: Option<String> = None;
    let mut last_coords: Option<(f64, f64)> = None;
    let mut tag_stack: Vec<String> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Eof => break,
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "Placemark" {
                    current = Some(RawPlacemark::default());
                    last_coords = None;
                }
                if name == "Data" {
                    current_data_name = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"name")
                        .map(|a| String::from_utf8_lossy(&a.value).to_string());
                }
                tag_stack.push(name);
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "Placemark" {
                    if let Some(mut pm) = current.take() {
                        pm.lon_lat = last_coords;
                        placemarks.push(pm);
                    }
                }
                tag_stack.pop();
            }
            Event::Text(t) => {
                let text = t.unescape()?.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let Some(top) = tag_stack.last() else { continue };
                match top.as_str() {
                    "styleUrl" => {
                        if let Some(pm) = current.as_mut() {
                            pm.style_url = text.trim_start_matches('#').to_string();
                        }
                    }
                    "value" => {
                        if let (Some(pm), Some(name)) = (current.as_mut(), current_data_name.take()) {
                            pm.data.insert(name, text);
                        }
                    }
                    "coordinates" => {
                        // "lon,lat,alt" - possibly multiple space-separated
                        // tuples for a polygon ring; we only want a single
                        // point's worth here since Point coordinates are
                        // always a single tuple.
                        if let Some(first_tuple) = text.split_whitespace().next() {
                            let parts: Vec<&str> = first_tuple.split(',').collect();
                            if parts.len() >= 2 {
                                if let (Ok(lon), Ok(lat)) = (parts[0].parse(), parts[1].parse()) {
                                    last_coords = Some((lon, lat));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(placemarks)
}

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

fn parse_customers(s: Option<&String>) -> i32 {
    s.and_then(|v| v.parse().ok()).unwrap_or(0)
}

pub fn to_event(pm: &RawPlacemark) -> Option<RawOutageEvent> {
    let outage_id = pm.data.get("outage_id")?;
    let planned_not_started = pm.data.get("planned_not_started").map(|v| v == "true").unwrap_or(false);

    let status = match pm.style_url.as_str() {
        "active_outage" => OutageStatus::Fault,
        "planned_outage" if planned_not_started => OutageStatus::Upcoming,
        "planned_outage" => OutageStatus::Planned,
        "inactive_outage" => OutageStatus::Resolved,
        other => {
            tracing::warn!(style = other, outage_id, "unrecognized Kraftringen styleUrl, treating as fault");
            OutageStatus::Fault
        }
    };

    // Whichever of these is nonzero reflects the count relevant to this
    // outage's current phase (current for active, future for not-yet-
    // started planned, previous for resolved).
    let affected_customers = [
        parse_customers(pm.data.get("current_affected_customers")),
        parse_customers(pm.data.get("future_affected_customers")),
        parse_customers(pm.data.get("previously_affected_customers")),
    ]
    .into_iter()
    .find(|&c| c > 0)
    .unwrap_or(0);

    let started_at = pm
        .data
        .get("occurred")
        .and_then(|s| parse_stockholm_time(s))
        .or_else(|| pm.data.get("planned_occurred_time").and_then(|s| parse_stockholm_time(s)));

    let estimated_end_at = pm.data.get("planned_restored_time").and_then(|s| parse_stockholm_time(s));

    Some(RawOutageEvent {
        provider: Provider::Kraftringen,
        source_id: outage_id.clone(),
        status,
        // No place name in this feed - see module docs. Using the outage
        // id keeps this unique and stable; swap in reverse-geocoded
        // locality name later without touching anything else.
        area_label: format!("Avbrott #{outage_id}"),
        lat: pm.lon_lat.map(|(_, lat)| lat),
        lng: pm.lon_lat.map(|(lon, _)| lon),
        affected_customers: Some(affected_customers),
        reason: pm.data.get("note_external").filter(|s| !s.is_empty()).cloned(),
        started_at,
        estimated_end_at,
        observed_at: Utc::now(),
    })
}

pub struct KraftringenAdapter {
    client: reqwest::Client,
}

impl KraftringenAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for KraftringenAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for KraftringenAdapter {
    fn name(&self) -> &'static str {
        "kraftringen"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(KML_URL).send().await?.text().await?;
        let placemarks = parse_kml(&body)?;
        Ok(placemarks.iter().filter_map(to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_KML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
    <Document>
        <Placemark>
            <styleUrl>#planned_outage</styleUrl>
            <ExtendedData>
                <Data name="status"><value>2</value></Data>
                <Data name="current_affected_customers"><value>0</value></Data>
                <Data name="future_affected_customers"><value>36</value></Data>
                <Data name="previously_affected_customers"><value>0</value></Data>
                <Data name="planned_not_started"><value>true</value></Data>
                <Data name="planned_restored_time"><value></value></Data>
                <Data name="occurred"><value></value></Data>
                <Data name="planned_occurred_time"><value>2026-09-05 08:00</value></Data>
                <Data name="outage_id"><value>105489</value></Data>
                <Data name="note_external"><value></value></Data>
            </ExtendedData>
            <Point><coordinates>13.030269534235668,56.1834313182776,0.0</coordinates></Point>
        </Placemark>
        <Placemark>
            <styleUrl>#active_outage</styleUrl>
            <ExtendedData>
                <Data name="status"><value>1</value></Data>
                <Data name="current_affected_customers"><value>263</value></Data>
                <Data name="future_affected_customers"><value>0</value></Data>
                <Data name="previously_affected_customers"><value>0</value></Data>
                <Data name="planned_not_started"><value>false</value></Data>
                <Data name="occurred"><value>2026-08-31 06:04</value></Data>
                <Data name="planned_restored_time"><value>2026-08-31 09:00</value></Data>
                <Data name="outage_id"><value>118250</value></Data>
                <Data name="note_external"><value></value></Data>
            </ExtendedData>
            <MultiGeometry>
                <Polygon>
                    <outerBoundaryIs>
                        <LinearRing>
                            <coordinates>13.0,55.7,0.0 13.1,55.8,0.0 13.0,55.7,0.0</coordinates>
                        </LinearRing>
                    </outerBoundaryIs>
                </Polygon>
                <Point><coordinates>13.05,55.75,0.0</coordinates></Point>
            </MultiGeometry>
        </Placemark>
    </Document>
</kml>"#;

    #[test]
    fn parses_placemarks() {
        let placemarks = parse_kml(SAMPLE_KML).unwrap();
        assert_eq!(placemarks.len(), 2);
    }

    #[test]
    fn upcoming_planned_outage_maps_correctly() {
        let placemarks = parse_kml(SAMPLE_KML).unwrap();
        let event = to_event(&placemarks[0]).unwrap();
        assert_eq!(event.status, OutageStatus::Upcoming);
        assert_eq!(event.affected_customers, Some(36));
        assert_eq!(event.source_id, "105489");
        assert_eq!(event.lat, Some(56.1834313182776));
        assert_eq!(event.lng, Some(13.030269534235668));
    }

    #[test]
    fn active_fault_uses_point_not_polygon_vertex() {
        let placemarks = parse_kml(SAMPLE_KML).unwrap();
        let event = to_event(&placemarks[1]).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.affected_customers, Some(263));
        // Must be the <Point> (13.05, 55.75), not a polygon vertex.
        assert_eq!(event.lng, Some(13.05));
        assert_eq!(event.lat, Some(55.75));
    }

    #[test]
    fn resolved_status_from_style_url() {
        let mut data = HashMap::new();
        data.insert("outage_id".to_string(), "1".to_string());
        data.insert("previously_affected_customers".to_string(), "10".to_string());
        let pm = RawPlacemark {
            style_url: "inactive_outage".to_string(),
            data,
            lon_lat: Some((13.0, 55.7)),
        };
        let event = to_event(&pm).unwrap();
        assert_eq!(event.status, OutageStatus::Resolved);
        assert_eq!(event.affected_customers, Some(10));
    }

    #[test]
    fn missing_outage_id_is_skipped() {
        let pm = RawPlacemark::default();
        assert!(to_event(&pm).is_none());
    }
}
