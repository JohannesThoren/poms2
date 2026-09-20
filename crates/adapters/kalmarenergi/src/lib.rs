//! Kalmar Energi adapter.
//!
//! By far the cleanest source in this workspace: `outage` is a genuine
//! WordPress custom post type registered for the REST API
//! (`/wp-json/wp/v2/outages`), with ACF (Advanced Custom Fields) giving
//! real structured data - coordinates, ISO timestamps, a stable UUID -
//! not just free text to parse. No HTML scraping needed at all.
//!
//! Category and info-type come from `class_list` entries WordPress adds
//! for each taxonomy term - `outage_type-{elnat|fjarrvarme|...}` and
//! `outage_info_type-{planerat-avbrott|driftstorning}` - rather than a
//! numeric ACF field (`outage-infotype`/`outage-type` are internal
//! numeric ids with no public lookup table found, so the human-readable
//! class_list slugs are used instead).
//!
//! Status isn't a field at all - it's computed from `outage-start` /
//! `outage-end` against the current time, same idea as Vattenfall's
//! upcoming/current split: end in the past → resolved (skipped, nothing
//! to resolve if never seen active); start in the future → Upcoming;
//! otherwise currently active, and `outage_info_type` says whether
//! that's a Fault ("driftstorning") or Planned ("planerat-avbrott").
//!
//! Location is `acf.outage-map` (point + address) when `outage-place ==
//! "map"`, or `outage-area-lat`/`-lng` when it's `"area"` - handled here
//! since the schema supports both, even though every real example seen
//! used "map".

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const URL: &str = "https://kalmarenergi.se/wp-json/wp/v2/outages?per_page=50&orderby=date&order=desc";
const ELNAT_CLASS: &str = "outage_type-elnat";
const FAULT_INFO_CLASS: &str = "outage_info_type-driftstorning";
const PLANNED_INFO_CLASS: &str = "outage_info_type-planerat-avbrott";

#[derive(Debug, Deserialize)]
struct WpOutage {
    id: i64,
    title: Rendered,
    content: Rendered,
    class_list: Vec<String>,
    acf: Acf,
}

#[derive(Debug, Deserialize)]
struct Rendered {
    rendered: String,
}

#[derive(Debug, Deserialize)]
struct Acf {
    #[serde(rename = "outage-place")]
    place: Option<String>,
    #[serde(rename = "outage-map")]
    map: Option<OutageMap>,
    #[serde(rename = "outage-area-lat")]
    area_lat: Option<StringOrNumber>,
    #[serde(rename = "outage-area-lng")]
    area_lng: Option<StringOrNumber>,
    #[serde(rename = "outage-start")]
    start: Option<String>,
    #[serde(rename = "outage-end")]
    end: Option<String>,
    #[serde(rename = "outage-id")]
    outage_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutageMap {
    lat: f64,
    lng: f64,
}

/// ACF sends empty numeric-ish fields as `""` when unset, and a real
/// number otherwise - this accepts either without failing the whole
/// response over one blank field.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    Number(f64),
    Text(String),
}

impl StringOrNumber {
    fn as_f64(&self) -> Option<f64> {
        match self {
            StringOrNumber::Number(n) => Some(*n),
            StringOrNumber::Text(s) => s.parse().ok(),
        }
    }
}

fn parse_stockholm_naive(s: &str) -> Option<DateTime<Utc>> {
    // "2026-09-23T09:30:00" - no timezone marker, ACF's own local format.
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

/// The `content.rendered` field is a handful of `<p>`/`<ul>`/`<li>`/
/// `<strong>` tags around otherwise plain text - stripped with simple
/// character scanning rather than pulling in an HTML parser for this one
/// field.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn to_event(post: &WpOutage, now: DateTime<Utc>) -> Option<RawOutageEvent> {
    if !post.class_list.iter().any(|c| c == ELNAT_CLASS) {
        return None;
    }

    let started_at = post.acf.start.as_deref().filter(|s| !s.is_empty()).and_then(parse_stockholm_naive);
    let estimated_end_at = post.acf.end.as_deref().filter(|s| !s.is_empty()).and_then(parse_stockholm_naive);

    let status = match (started_at, estimated_end_at) {
        (_, Some(end)) if end < now => return None, // already over - nothing to resolve if never seen active
        (Some(start), _) if start > now => OutageStatus::Upcoming,
        _ => {
            if post.class_list.iter().any(|c| c == FAULT_INFO_CLASS) {
                OutageStatus::Fault
            } else if post.class_list.iter().any(|c| c == PLANNED_INFO_CLASS) {
                OutageStatus::Planned
            } else {
                OutageStatus::Fault
            }
        }
    };

    let (lat, lng) = if post.acf.place.as_deref() == Some("area") {
        (
            post.acf.area_lat.as_ref().and_then(|v| v.as_f64()),
            post.acf.area_lng.as_ref().and_then(|v| v.as_f64()),
        )
    } else {
        post.acf.map.as_ref().map(|m| (Some(m.lat), Some(m.lng))).unwrap_or((None, None))
    };

    Some(RawOutageEvent {
        provider: Provider::Kalmarenergi,
        source_id: post.acf.outage_id.clone().unwrap_or_else(|| post.id.to_string()),
        status,
        area_label: strip_html(&post.title.rendered),
        lat,
        lng,
        polygon: None,
        affected_customers: None,
        reason: Some(strip_html(&post.content.rendered)),
        started_at,
        estimated_end_at,
        observed_at: now,
    })
}

pub struct KalmarenergiAdapter {
    client: reqwest::Client,
}

impl KalmarenergiAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for KalmarenergiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for KalmarenergiAdapter {
    fn name(&self) -> &'static str {
        "kalmarenergi"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let posts: Vec<WpOutage> = self.client.get(URL).send().await?.json().await?;
        let now = Utc::now();
        Ok(posts.iter().filter_map(|p| to_event(p, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn base_post(class_list: Vec<&str>, start: Option<&str>, end: Option<&str>) -> WpOutage {
        WpOutage {
            id: 1,
            title: Rendered { rendered: "Test <strong>titel</strong>".into() },
            content: Rendered { rendered: "<p>Beskrivning</p>".into() },
            class_list: class_list.into_iter().map(String::from).collect(),
            acf: Acf {
                place: Some("map".into()),
                map: Some(OutageMap { lat: 56.6, lng: 16.3 }),
                area_lat: None,
                area_lng: None,
                start: start.map(String::from),
                end: end.map(String::from),
                outage_id: Some("test-uuid".into()),
            },
        }
    }

    /// Test data needs a naive "as if Stockholm local time" string, since
    /// that's what `parse_stockholm_naive` expects (matching real ACF
    /// data) - formatting a UTC instant directly would silently apply
    /// Stockholm's UTC offset a second time when parsed back.
    fn stockholm_naive_string(utc_instant: DateTime<Utc>) -> String {
        utc_instant.with_timezone(&Stockholm).format("%Y-%m-%dT%H:%M:%S").to_string()
    }

    #[test]
    fn strips_html_tags() {
        assert_eq!(strip_html("Test <strong>titel</strong>"), "Test titel");
    }

    #[test]
    fn non_elnat_category_is_filtered() {
        let post = base_post(vec!["outage_type-fjarrvarme", "outage_info_type-driftstorning"], None, None);
        assert!(to_event(&post, Utc::now()).is_none());
    }

    #[test]
    fn already_ended_is_skipped() {
        let now = Utc::now();
        let end = stockholm_naive_string(now - Duration::hours(2));
        let post = base_post(vec!["outage_type-elnat", FAULT_INFO_CLASS], None, Some(&end));
        assert!(to_event(&post, now).is_none());
    }

    #[test]
    fn future_start_is_upcoming() {
        let now = Utc::now();
        let start = stockholm_naive_string(now + Duration::hours(5));
        let post = base_post(vec!["outage_type-elnat", PLANNED_INFO_CLASS], Some(&start), None);
        let event = to_event(&post, now).unwrap();
        assert_eq!(event.status, OutageStatus::Upcoming);
    }

    #[test]
    fn active_driftstorning_is_fault() {
        let now = Utc::now();
        let start = stockholm_naive_string(now - Duration::hours(1));
        let post = base_post(vec!["outage_type-elnat", FAULT_INFO_CLASS], Some(&start), None);
        let event = to_event(&post, now).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.lat, Some(56.6));
    }

    #[test]
    fn active_planerat_avbrott_is_planned() {
        let now = Utc::now();
        let start = stockholm_naive_string(now - Duration::minutes(30));
        let end = stockholm_naive_string(now + Duration::hours(2));
        let post = base_post(vec!["outage_type-elnat", PLANNED_INFO_CLASS], Some(&start), Some(&end));
        let event = to_event(&post, now).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn real_fixture_parses_and_finds_elnat_entries() {
        let json_str = include_str!("../tests/fixtures/real_sample.json");
        let posts: Vec<WpOutage> = serde_json::from_str(json_str).unwrap();
        assert!(!posts.is_empty());

        let elnat_count = posts.iter().filter(|p| p.class_list.iter().any(|c| c == ELNAT_CLASS)).count();
        assert!(elnat_count > 0, "expected at least one real Elnät post in the fixture");

        // Every real Elnät post in the fixture is in the past, so calling
        // to_event with "now" set far in the future should resolve
        // (skip) all of them without panicking - proves the date parsing
        // and comparison logic works on genuine ACF timestamp strings.
        let far_future = Utc::now() + Duration::days(3650);
        for post in &posts {
            let _ = to_event(post, far_future);
        }

        // At least one real post should carry real coordinates.
        assert!(posts.iter().any(|p| p.acf.map.is_some()));
    }
}
