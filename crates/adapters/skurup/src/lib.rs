//! Skurups Elverk adapter.
//!
//! `/driftinformation...html` is a SiteVision "webapp" (React-based,
//! `AppRegistry.registerApp`/`registerInitialState`), but unlike the
//! Höganäs/Sandviken SiteVision instances we've seen, this one's initial
//! data is embedded directly in the page as a JSON blob passed to
//! `AppRegistry.registerInitialState(...)` - no separate API call needed
//! at all, just extract and parse that JSON.
//!
//! The schema (`{"articles": [...], "limitPerPage": ..., "totalArticles": ...}`)
//! has no category field - `"tags": []` was empty on every real article
//! seen - so electricity vs fiber/broadband can only be told apart by
//! keywords in the free-text `title` ("ström"/"elnät" vs
//! "fiber"/"internet"/"bredband").
//!
//! Every real article seen so far had `status: "Åtgärdat"` (resolved) -
//! the UI's own icon legend shows "Åtgärdat" and "Planerat" as the two
//! statuses, but no live "Planerat" (or any clearly-active/unplanned)
//! example was available to confirm exact status strings beyond
//! "Åtgärdat". This adapter treats `"Åtgärdat"` as resolved (skipped -
//! nothing to resolve if never seen active) and `"Planerat"` as Planned;
//! anything else is treated as Fault, on the assumption that a status
//! this feed doesn't call "Åtgärdat" or "Planerat" is something actively
//! unplanned. Flagged here because that last mapping is inferred, not
//! confirmed against a real example.
//!
//! `dateString` ("21 feb, 2026 - 21 feb, 2026") only gives dates, no
//! time-of-day - parsed as Stockholm midnight on each date.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;

const URL: &str =
    "https://www.skurupselverk.se/driftinformation.4.c50359218de47eea8c1f83.html";
const ELECTRICITY_KEYWORDS: [&str; 2] = ["ström", "elnät"];
const NON_ELECTRICITY_KEYWORDS: [&str; 3] = ["fiber", "internet", "bredband"];

#[derive(Debug, Deserialize)]
struct Bootstrap {
    articles: Vec<Article>,
}

#[derive(Debug, Deserialize)]
struct Article {
    id: String,
    title: String,
    status: String,
    #[serde(rename = "dateString")]
    date_string: String,
}

/// The page embeds this JSON as an argument to a JS function call, not as
/// a standalone document - `{"articles": [...], ...}` is a real, complete
/// JSON value in there, but with a `);</script>...` tail that a normal
/// full-document parse would choke on. `StreamDeserializer` reads just
/// the first self-delimiting JSON value and ignores everything after it.
fn extract_bootstrap(html: &str) -> Option<Bootstrap> {
    let marker = "\"articles\":";
    let idx = html.find(marker)?;
    let start = html[..idx].rfind('{')?;
    let mut stream = serde_json::Deserializer::from_str(&html[start..]).into_iter::<Bootstrap>();
    stream.next()?.ok()
}

fn is_electricity(title: &str) -> bool {
    let lower = title.to_lowercase();
    let electricity = ELECTRICITY_KEYWORDS.iter().any(|k| lower.contains(k));
    let excluded = NON_ELECTRICITY_KEYWORDS.iter().any(|k| lower.contains(k));
    electricity && !excluded
}

fn swedish_month(abbr: &str) -> Option<u32> {
    Some(match abbr.trim_end_matches('.').to_lowercase().as_str() {
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

/// Parses one side of "21 feb, 2026" into a Stockholm-midnight UTC instant.
fn parse_swedish_date(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim().replace(',', "");
    let mut parts = s.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = swedish_month(parts.next()?)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(0, 0, 0)?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

fn parse_date_range(date_string: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    match date_string.split_once(" - ") {
        Some((start, end)) => (parse_swedish_date(start), parse_swedish_date(end)),
        None => (parse_swedish_date(date_string), None),
    }
}

fn to_event(article: &Article) -> Option<RawOutageEvent> {
    if !is_electricity(&article.title) {
        return None;
    }
    if article.status == "Åtgärdat" {
        return None;
    }
    let status = if article.status == "Planerat" {
        OutageStatus::Planned
    } else {
        OutageStatus::Fault
    };
    let (started_at, estimated_end_at) = parse_date_range(&article.date_string);

    Some(RawOutageEvent {
        provider: Provider::Skurup,
        source_id: article.id.clone(),
        status,
        area_label: article.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: None,
        started_at,
        estimated_end_at,
        observed_at: Utc::now(),
    })
}

pub struct SkurupAdapter {
    client: reqwest::Client,
}

impl SkurupAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for SkurupAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for SkurupAdapter {
    fn name(&self) -> &'static str {
        "skurup"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(URL).send().await?.text().await?;
        let Some(bootstrap) = extract_bootstrap(&body) else {
            anyhow::bail!("could not find/parse the embedded articles JSON");
        };
        Ok(bootstrap.articles.iter().filter_map(to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electricity_keyword_detection() {
        assert!(is_electricity("Planerat strömavbrott i elnätet 2025-03-28"));
        assert!(is_electricity("Just nu har vi inga identifierade driftstörningar i Elnätet"));
        assert!(!is_electricity("Planerat driftuppehåll i Skurups fibernät"));
        assert!(!is_electricity("Något helt annat"));
    }

    #[test]
    fn atgardat_status_is_skipped() {
        let a = Article {
            id: "1".into(),
            title: "Strömavbrott".into(),
            status: "Åtgärdat".into(),
            date_string: "21 feb, 2026 - 21 feb, 2026".into(),
        };
        assert!(to_event(&a).is_none());
    }

    #[test]
    fn planerat_status_is_planned() {
        let a = Article {
            id: "1".into(),
            title: "Planerat strömavbrott".into(),
            status: "Planerat".into(),
            date_string: "21 feb, 2026 - 22 feb, 2026".into(),
        };
        let event = to_event(&a).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn unrecognized_status_defaults_to_fault() {
        let a = Article {
            id: "1".into(),
            title: "Strömavbrott i Skurup".into(),
            status: "Pågående".into(),
            date_string: "21 feb, 2026 - 21 feb, 2026".into(),
        };
        let event = to_event(&a).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
    }

    #[test]
    fn parses_swedish_date_range() {
        let (start, end) = parse_date_range("21 feb, 2026 - 22 feb, 2026");
        assert_eq!(start.unwrap().to_rfc3339(), "2026-02-20T23:00:00+00:00");
        assert_eq!(end.unwrap().to_rfc3339(), "2026-02-21T23:00:00+00:00");
    }

    #[test]
    fn real_fixture_extracts_and_parses() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let bootstrap = extract_bootstrap(html).expect("should find the embedded JSON");
        assert_eq!(bootstrap.articles.len(), 5, "expected 5 articles in the real fixture");
        // Every real article captured was already resolved - confirms
        // the filter engages on real data, not just hand-written structs.
        assert!(bootstrap.articles.iter().all(|a| a.status == "Åtgärdat"));
        for a in &bootstrap.articles {
            assert!(to_event(a).is_none());
        }
    }
}
