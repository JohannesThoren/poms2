//! Karlshamn Energi adapter.
//!
//! Their `/driftsinformation/` page loads its list via a WordPress-theme
//! AJAX endpoint - found in `scripts.js` (`app.getDriftinfoData`), no
//! auth:
//!
//!   https://www.karlshamnenergi.se/wp-content/themes/karlshamnenergi/ajax/beredskapsalarm/get_data.php?type=info
//!
//! The response has two buckets - `messages.active` (currently running,
//! whether planned or not - distinguished by `_status`) and
//! `messages.planned` (scheduled, not yet started) - each keyed by
//! utility type (`el`, `vatten`, `fjarrvarme`, `bredband`). This adapter
//! only keeps `el`.
//!
//! Two data-quality notes:
//! - PHP serializes an empty associative array as JSON `[]` instead of
//!   `{}`, so `messages.active`/`messages.planned` can legitimately be
//!   either a JSON object (keyed by type) or an empty array depending on
//!   whether *anything* is currently listed, for any utility. This
//!   adapter parses both shapes rather than assuming an object.
//! - No live electricity example was available when this was written
//!   (only "vatten" had active entries) - the `_status` -> Fault/Planned
//!   mapping (based on the same "ok / warning / acute" three-tier
//!   convention seen on other municipal utility sites) is inferred, not
//!   confirmed against a real `el` entry.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const URL: &str =
    "https://www.karlshamnenergi.se/wp-content/themes/karlshamnenergi/ajax/beredskapsalarm/get_data.php?type=info";
const TYPE_FILTER: &str = "el";

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(rename = "Header")]
    header: String,
    #[serde(rename = "Message")]
    message: String,
    #[serde(rename = "_type")]
    utility_type: String,
    #[serde(rename = "_status")]
    status: String,
    #[serde(rename = "_fromTime")]
    from_time: String,
    #[serde(rename = "_toTime")]
    to_time: String,
    #[serde(rename = "_fromYear")]
    from_year: String,
    #[serde(rename = "_toYear")]
    to_year: String,
}

/// Handles the PHP empty-array-vs-object ambiguity described in the
/// module docs: returns every message across all utility types,
/// regardless of whether the source served `[]` or `{"el": [...], ...}`.
fn extract_all_messages(bucket: &Value) -> Vec<Message> {
    let Some(obj) = bucket.as_object() else {
        return Vec::new(); // it was `[]` - PHP's empty-map serialization.
    };
    obj.values()
        .filter_map(|v| v.as_array())
        .flatten()
        .filter_map(|m| serde_json::from_value::<Message>(m.clone()).ok())
        .collect()
}

fn swedish_month(abbr: &str) -> Option<u32> {
    Some(match abbr.to_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "maj" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "okt" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

/// Parses e.g. "2 Sep" + "2026" + "13:00" into a UTC instant, treating the
/// source as Europe/Stockholm local time (no timezone given).
fn parse_swedish_datetime(day_month: &str, year: &str, time: Option<&str>) -> Option<DateTime<Utc>> {
    let mut parts = day_month.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = swedish_month(parts.next()?)?;
    let year: i32 = year.parse().ok()?;
    let (hour, minute) = time
        .and_then(|t| t.split_once(':'))
        .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((0, 0));

    let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, 0)?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn parse_message_time(field: &str, year: &str) -> Option<DateTime<Utc>> {
    // `_fromTime`/`_toTime` are like "2 Sep 13:00" - day, month, time all
    // in one string; year comes from the separate `_fromYear`/`_toYear`.
    let (date_part, time_part) = field.rsplit_once(' ')?;
    parse_swedish_datetime(date_part, year, Some(time_part))
}

fn stable_id(header: &str, from_time: &str) -> String {
    let mut hasher = DefaultHasher::new();
    header.hash(&mut hasher);
    from_time.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn to_event(msg: &Message, upcoming: bool) -> Option<RawOutageEvent> {
    if msg.utility_type != TYPE_FILTER {
        return None;
    }

    let started_at = parse_message_time(&msg.from_time, &msg.from_year);
    let estimated_end_at = parse_message_time(&msg.to_time, &msg.to_year);

    let status = if upcoming {
        OutageStatus::Upcoming
    } else if msg.status == "warning" {
        OutageStatus::Planned
    } else {
        OutageStatus::Fault
    };

    // Strip the small amount of HTML the Message field carries.
    let plain_message = msg.message.replace("<p>", "").replace("</p>", "").trim().to_string();

    Some(RawOutageEvent {
        provider: Provider::Karlshamn,
        source_id: stable_id(&msg.header, &msg.from_time),
        status,
        area_label: msg.header.clone(),
        lat: None,
        lng: None,
        affected_customers: None,
        reason: Some(plain_message),
        started_at,
        estimated_end_at,
        observed_at: Utc::now(),
    })
}

pub struct KarlshamnAdapter {
    client: reqwest::Client,
}

impl KarlshamnAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for KarlshamnAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for KarlshamnAdapter {
    fn name(&self) -> &'static str {
        "karlshamn"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body: Value = self.client.get(URL).send().await?.json().await?;
        let active = extract_all_messages(&body["messages"]["active"]);
        let planned = extract_all_messages(&body["messages"]["planned"]);

        let mut events: Vec<RawOutageEvent> = active.iter().filter_map(|m| to_event(m, false)).collect();
        events.extend(planned.iter().filter_map(|m| to_event(m, true)));
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_array_bucket_yields_no_messages() {
        let v: Value = serde_json::json!([]);
        assert!(extract_all_messages(&v).is_empty());
    }

    #[test]
    fn object_bucket_extracts_messages_across_types() {
        let v: Value = serde_json::json!({
            "vatten": [{"Header":"H","Message":"M","_type":"vatten","_status":"warning","_fromTime":"2 Sep 13:00","_toTime":"4 Sep 07:00","_fromYear":"2026","_toYear":"2026"}],
            "el": [{"Header":"H2","Message":"M2","_type":"el","_status":"error","_fromTime":"1 Sep 08:00","_toTime":"1 Sep 10:00","_fromYear":"2026","_toYear":"2026"}]
        });
        let messages = extract_all_messages(&v);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn non_el_type_is_filtered_out() {
        let msg = Message {
            header: "H".into(),
            message: "M".into(),
            utility_type: "vatten".into(),
            status: "warning".into(),
            from_time: "2 Sep 13:00".into(),
            to_time: "4 Sep 07:00".into(),
            from_year: "2026".into(),
            to_year: "2026".into(),
        };
        assert!(to_event(&msg, false).is_none());
    }

    #[test]
    fn warning_status_active_is_planned() {
        let msg = Message {
            header: "H".into(),
            message: "M".into(),
            utility_type: "el".into(),
            status: "warning".into(),
            from_time: "2 Sep 13:00".into(),
            to_time: "4 Sep 07:00".into(),
            from_year: "2026".into(),
            to_year: "2026".into(),
        };
        assert_eq!(to_event(&msg, false).unwrap().status, OutageStatus::Planned);
    }

    #[test]
    fn non_warning_status_active_is_fault() {
        let msg = Message {
            header: "H".into(),
            message: "M".into(),
            utility_type: "el".into(),
            status: "error".into(),
            from_time: "2 Sep 13:00".into(),
            to_time: "4 Sep 07:00".into(),
            from_year: "2026".into(),
            to_year: "2026".into(),
        };
        assert_eq!(to_event(&msg, false).unwrap().status, OutageStatus::Fault);
    }

    #[test]
    fn planned_bucket_is_always_upcoming() {
        let msg = Message {
            header: "H".into(),
            message: "M".into(),
            utility_type: "el".into(),
            status: "warning".into(),
            from_time: "2 Sep 13:00".into(),
            to_time: "4 Sep 07:00".into(),
            from_year: "2026".into(),
            to_year: "2026".into(),
        };
        assert_eq!(to_event(&msg, true).unwrap().status, OutageStatus::Upcoming);
    }

    #[test]
    fn parses_swedish_date_time() {
        let dt = parse_message_time("2 Sep 13:00", "2026").unwrap();
        // 2026-09-02 is CEST (UTC+2).
        assert_eq!(dt.to_rfc3339(), "2026-09-02T11:00:00+00:00");
    }

    #[test]
    fn real_fixture_parses_without_panicking() {
        let json_str = include_str!("../tests/fixtures/real_sample.json");
        let body: Value = serde_json::from_str(json_str).unwrap();
        let active = extract_all_messages(&body["messages"]["active"]);
        let planned = extract_all_messages(&body["messages"]["planned"]);
        assert!(!active.is_empty(), "fixture is known to have active vatten messages");
        assert!(planned.is_empty(), "fixture is known to have an empty planned bucket");
        // None of the real messages are "el", so no events should survive
        // the filter - confirms parsing + filtering doesn't crash on the
        // real shape.
        let events: Vec<_> = active.iter().filter_map(|m| to_event(m, false)).collect();
        assert_eq!(events.len(), 0);
    }
}
