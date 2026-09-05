//! Upplands Energi adapter.
//!
//! Their outage map is a small custom Google-Maps-based widget. Its own JS
//! (`module-497-combined.min.js`) plainly calls two static JSON endpoints,
//! no auth:
//!
//!   https://avbrott.upplandsenergi.se/managed/interruptions.json  - active interruptions
//!   https://avbrott.upplandsenergi.se/managed/area/list.json      - area id -> name/customers
//!
//! The interruption object's shape (`id`, `areaId`, `type`, `status`,
//! `start`, `end`, `est`, `customers`, `message`) was read directly out of
//! that JS rather than observed live - `interruptions.json` returned an
//! empty array `[]` the whole time this was written, so **the exact
//! JSON type of `start`/`end`/`est` (epoch number vs. date string) is
//! unconfirmed**. [`parse_flexible_time`] accepts either shape as a
//! best-effort guess; verify against a real interruption before trusting
//! `started_at`/`estimated_end_at` from this adapter.
//!
//! `area/list.json` is served as Latin-1 (Windows-1252-ish), not UTF-8 -
//! decoding it as UTF-8 directly corrupts Swedish characters (å/ä/ö), so
//! this fetches the raw bytes and decodes with `encoding_rs`.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

const INTERRUPTIONS_URL: &str = "https://avbrott.upplandsenergi.se/managed/interruptions.json";
const AREA_LIST_URL: &str = "https://avbrott.upplandsenergi.se/managed/area/list.json";

#[derive(Debug, Deserialize)]
struct Interruption {
    id: i64,
    #[serde(rename = "areaId")]
    area_id: i64,
    #[serde(rename = "type")]
    interruption_type: String,
    status: Option<String>,
    start: Option<Value>,
    end: Option<Value>,
    est: Option<Value>,
    customers: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Area {
    id: i64,
    name: String,
    customers: i32,
}

/// Best-effort parse for an unconfirmed time representation - see module
/// docs. Tries, in order: epoch milliseconds, epoch seconds, RFC 3339,
/// then a naive "YYYY-MM-DD HH:MM[:SS]" interpreted as Europe/Stockholm.
fn parse_flexible_time(v: &Value) -> Option<DateTime<Utc>> {
    if let Some(n) = v.as_i64() {
        return if n > 10_000_000_000 {
            DateTime::from_timestamp_millis(n)
        } else {
            DateTime::from_timestamp(n, 0)
        };
    }
    let s = v.as_str()?;
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return match Stockholm.from_local_datetime(&naive) {
                chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
                chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
                chrono::LocalResult::None => None,
            };
        }
    }
    None
}

fn map_status(interruption: &Interruption, now: DateTime<Utc>, started_at: Option<DateTime<Utc>>) -> OutageStatus {
    match interruption.interruption_type.as_str() {
        "ongoing" => OutageStatus::Fault,
        "planned" => match started_at {
            Some(start) if start > now => OutageStatus::Upcoming,
            _ => OutageStatus::Planned,
        },
        other => {
            tracing::warn!(t = other, "unrecognized Upplands Energi interruption type, treating as fault");
            OutageStatus::Fault
        }
    }
}

fn to_event(interruption: &Interruption, areas: &HashMap<i64, Area>, now: DateTime<Utc>) -> RawOutageEvent {
    let area = areas.get(&interruption.area_id);
    let started_at = interruption.start.as_ref().and_then(parse_flexible_time);
    let estimated_end_at = interruption
        .end
        .as_ref()
        .or(interruption.est.as_ref())
        .and_then(parse_flexible_time);

    RawOutageEvent {
        provider: Provider::UpplandsEnergi,
        source_id: interruption.id.to_string(),
        status: map_status(interruption, now, started_at),
        area_label: area.map(|a| a.name.clone()).unwrap_or_else(|| format!("Område #{}", interruption.area_id)),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: interruption.customers.or(area.map(|a| a.customers)),
        reason: interruption.message.clone().or_else(|| interruption.status.clone()),
        started_at,
        estimated_end_at,
        observed_at: now,
    }
}

pub struct UpplandsEnergiAdapter {
    client: reqwest::Client,
}

impl UpplandsEnergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn fetch_areas(&self) -> anyhow::Result<HashMap<i64, Area>> {
        let bytes = self.client.get(AREA_LIST_URL).send().await?.bytes().await?;
        let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
        let areas: Vec<Area> = serde_json::from_str(&decoded)?;
        Ok(areas.into_iter().map(|a| (a.id, a)).collect())
    }
}

impl Default for UpplandsEnergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for UpplandsEnergiAdapter {
    fn name(&self) -> &'static str {
        "upplands_energi"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let interruptions: Vec<Interruption> =
            self.client.get(INTERRUPTIONS_URL).send().await?.json().await?;
        if interruptions.is_empty() {
            return Ok(Vec::new());
        }
        let areas = self.fetch_areas().await?;
        let now = Utc::now();
        Ok(interruptions.iter().map(|i| to_event(i, &areas, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_epoch_millis() {
        let v = Value::from(1_700_000_000_000i64);
        assert!(parse_flexible_time(&v).is_some());
    }

    #[test]
    fn parses_epoch_seconds() {
        let v = Value::from(1_700_000_000i64);
        assert!(parse_flexible_time(&v).is_some());
    }

    #[test]
    fn parses_naive_datetime_string() {
        let v = Value::from("2026-09-02 14:30");
        assert!(parse_flexible_time(&v).is_some());
    }

    #[test]
    fn ongoing_type_is_fault() {
        let i = Interruption {
            id: 1,
            area_id: 1,
            interruption_type: "ongoing".into(),
            status: None,
            start: None,
            end: None,
            est: None,
            customers: Some(10),
            message: None,
        };
        assert_eq!(map_status(&i, Utc::now(), None), OutageStatus::Fault);
    }

    #[test]
    fn area_name_falls_back_when_missing() {
        let i = Interruption {
            id: 1,
            area_id: 999,
            interruption_type: "ongoing".into(),
            status: None,
            start: None,
            end: None,
            est: None,
            customers: Some(10),
            message: None,
        };
        let event = to_event(&i, &HashMap::new(), Utc::now());
        assert_eq!(event.area_label, "Område #999");
    }

    #[test]
    fn decodes_windows_1252_area_names() {
        // "Öregrund" encoded as Windows-1252 (0xd6 = Ö).
        let bytes: &[u8] = &[
            b'[', b'{', b'"', b'i', b'd', b'"', b':', b'1', b',', b'"', b'n', b'a', b'm', b'e', b'"', b':', b'"',
            0xd6, b'r', b'e', b'g', b'r', b'u', b'n', b'd', b'"', b',', b'"', b'c', b'u', b's', b't', b'o', b'm',
            b'e', b'r', b's', b'"', b':', b'1', b'0', b'}', b']',
        ];
        let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
        let areas: Vec<Area> = serde_json::from_str(&decoded).unwrap();
        assert_eq!(areas[0].name, "Öregrund");
    }
}
