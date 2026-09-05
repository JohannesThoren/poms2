//! Generic adapter for Digpro's "Outage Map" product (the same underlying
//! system Kraftringen uses, branded "outagemap2" and typically embedded as
//! `https://<origin>/outagemap2/?cust=<cust>&app=<app>` on each utility's
//! own site).
//!
//! Reverse engineered from Dala Energi's outagemap2 JS bundle: the app
//! fetches its KML from
//!
//!   {origin}/bios/servlet/sys.outagemap.servlets.api.GetOutagesKML?app={app}_{cust}
//!
//! - a plain, unauthenticated GET, no API key. `{app}` is consistently
//! `fpp` across every instance checked so far. Confirmed working
//! (2026-09) against: Växjö Energi, Lerum Energi, Västerbergslagens Elnät,
//! Partille Energi - all return the identical KML shape Kraftringen's own
//! feed does (same style ids, same `ExtendedData` field names), so this
//! reuses that exact parsing logic rather than re-deriving it.
//!
//! Because every deployment is otherwise identical, one binary serves all
//! of them - which company (and its provider/origin/cust) to poll is
//! chosen entirely by environment variables (see `main.rs`), and
//! docker-compose runs one container per company with different env.
//! Adding a newly-verified company is a docker-compose service block, not
//! new code.

use async_trait::async_trait;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;

#[derive(Debug, Default)]
struct RawPlacemark {
    style_url: String,
    data: HashMap<String, String>,
    lon_lat: Option<(f64, f64)>,
}

fn parse_kml(xml: &str) -> anyhow::Result<Vec<RawPlacemark>> {
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

fn parse_stockholm_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    if s.trim().is_empty() {
        return None;
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok()?;
    match chrono_tz::Europe::Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&chrono::Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&chrono::Utc)),
        chrono::LocalResult::None => None,
    }
}

fn parse_customers(s: Option<&String>) -> i32 {
    s.and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn to_event(provider: Provider, pm: &RawPlacemark) -> Option<RawOutageEvent> {
    let outage_id = pm.data.get("outage_id")?;
    let planned_not_started = pm.data.get("planned_not_started").map(|v| v == "true").unwrap_or(false);

    let status = match pm.style_url.as_str() {
        "active_outage" => OutageStatus::Fault,
        "planned_outage" if planned_not_started => OutageStatus::Upcoming,
        "planned_outage" => OutageStatus::Planned,
        "inactive_outage" => OutageStatus::Resolved,
        other => {
            tracing::warn!(style = other, outage_id, "unrecognized Digpro styleUrl, treating as fault");
            OutageStatus::Fault
        }
    };

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
        provider,
        source_id: outage_id.clone(),
        status,
        // Same gap as Kraftringen: this feed has no place-name field.
        area_label: format!("Avbrott #{outage_id}"),
        lat: pm.lon_lat.map(|(_, lat)| lat),
        lng: pm.lon_lat.map(|(lon, _)| lon),
        polygon: None,
        affected_customers: Some(affected_customers),
        reason: pm.data.get("note_external").filter(|s| !s.is_empty()).cloned(),
        started_at,
        estimated_end_at,
        observed_at: chrono::Utc::now(),
    })
}

pub struct DigproAdapter {
    client: reqwest::Client,
    provider: Provider,
    adapter_name: &'static str,
    kml_url: String,
}

impl DigproAdapter {
    /// `adapter_name` is used only for logging (must be `'static`, so pass
    /// a literal or leaked string from config).
    pub fn new(provider: Provider, adapter_name: &'static str, base_url: &str, cust: &str, app: &str) -> Self {
        let kml_url =
            format!("{base_url}/bios/servlet/sys.outagemap.servlets.api.GetOutagesKML?app={app}_{cust}");
        Self::from_url(provider, adapter_name, kml_url)
    }

    /// Some deployments (e.g. Telge Nät) serve the same product under a
    /// servlet path missing the `.api.` segment
    /// (`sys.outagemap.servlets.GetOutagesKML` instead of
    /// `sys.outagemap.servlets.api.GetOutagesKML`) - and in Telge's case,
    /// their own site API happens to publish the exact working KML URL
    /// directly, which is easier and more robust than guessing at the
    /// path variant. Use this constructor when you have that URL already
    /// rather than trying to reconstruct it from base_url/cust/app.
    pub fn from_url(provider: Provider, adapter_name: &'static str, kml_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
            provider,
            adapter_name,
            kml_url: kml_url.into(),
        }
    }
}

#[async_trait]
impl Adapter for DigproAdapter {
    fn name(&self) -> &'static str {
        self.adapter_name
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(&self.kml_url).send().await?.text().await?;
        let placemarks = parse_kml(&body)?;
        Ok(placemarks.iter().filter_map(|pm| to_event(self.provider, pm)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_kml_url() {
        let adapter = DigproAdapter::new(Provider::Vaxjo, "vaxjo", "https://driftinformation.veab.se", "vax", "fpp");
        assert_eq!(
            adapter.kml_url,
            "https://driftinformation.veab.se/bios/servlet/sys.outagemap.servlets.api.GetOutagesKML?app=fpp_vax"
        );
    }

    #[test]
    fn parses_real_vaxjo_fixture() {
        let xml = include_str!("../tests/fixtures/vaxjo_real_sample.kml");
        let placemarks = parse_kml(xml).expect("should parse");
        assert!(!placemarks.is_empty());
        let events: Vec<_> = placemarks.iter().filter_map(|pm| to_event(Provider::Vaxjo, pm)).collect();
        assert_eq!(events.len(), placemarks.len());
        for event in &events {
            assert_eq!(event.provider, Provider::Vaxjo);
        }
    }

    #[test]
    fn missing_outage_id_is_skipped() {
        assert!(to_event(Provider::Lerum, &RawPlacemark::default()).is_none());
    }
}
