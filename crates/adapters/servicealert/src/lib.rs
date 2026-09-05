//! Generic adapter for the "ServiceAlert" platform (`se.sms-service.dk`),
//! a shared Danish SaaS embedded as an iframe on several Swedish
//! utilities' sites. Confirmed tenants (2026-09): Karlstads El, Eskilstuna
//! Strängnäs Energi, Tranås Energi, Uddevalla Energi - each identified
//! only by a `customerId` GUID baked into their iframe's query string, no
//! per-tenant subdomain.
//!
//! The endpoint wasn't discoverable from the JS bundle (which contained no
//! literal URLs) - it was found by recording real network traffic with a
//! headless browser against the iframe URL. It's a single POST, no auth:
//!
//! ```text
//! POST https://se.sms-service.dk/api/WebMessage/GetDriftstatusWebMessagesMapModel
//! Content-Type: application/json
//! {"customerIds": "<guid>", "internalOnly": false, "urlParams": {"customerId": "<guid>"}}
//! ```
//!
//! The response groups messages by utility type (`profileTitle`: "Elnät",
//! "Fjärrvärme", "Fibernät", ...) - this adapter only keeps "Elnät".
//!
//! **Known data-quality caveat**, worth remembering before trusting this
//! provider's status field: unlike every other source in this system,
//! ServiceAlert has no explicit status code. Each message only carries a
//! display window (`dateDelayUtc`..`dateExpireUtc`) and a free-text HTML
//! body meant for humans - the text can say an outage was already fixed
//! while the message is technically still "live" (still within its
//! display window). Status here is therefore inferred, not authoritative:
//! before the window → upcoming, inside it → planned/fault (guessed from
//! whether the title says "Planerat"), after it → resolved. No live
//! electricity outage was available to validate this heuristic against
//! when it was written (only district heating/fiber messages were active)
//! - treat the fault/planned split with proportionally more suspicion
//! than other adapters until it's checked against a real one.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use regex::Regex;
use serde::Deserialize;
use serde_json::json;

const ENDPOINT: &str = "https://se.sms-service.dk/api/WebMessage/GetDriftstatusWebMessagesMapModel";
const ELNAT_PROFILE_TITLE: &str = "Elnät";

#[derive(Debug, Deserialize)]
struct MapModel {
    #[serde(rename = "profileSettingsAndMessages")]
    profiles: Vec<ProfileMessages>,
}

#[derive(Debug, Deserialize)]
struct ProfileMessages {
    #[serde(rename = "profileTitle")]
    profile_title: Option<String>,
    #[serde(rename = "webMessages")]
    web_messages: Vec<WebMessage>,
}

#[derive(Debug, Deserialize)]
struct WebMessage {
    id: i64,
    title: String,
    text: String,
    #[serde(rename = "dateExpireUtc")]
    date_expire_utc: DateTime<Utc>,
    #[serde(rename = "dateDelayUtc")]
    date_delay_utc: DateTime<Utc>,
    #[serde(rename = "affectedAddressesCoordinates", default)]
    affected_addresses_coordinates: Vec<LatLng>,
}

#[derive(Debug, Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

fn strip_html(html: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let collapsed = tag_re.replace_all(html, " ");
    let ws_re = Regex::new(r"\s+").unwrap();
    ws_re.replace_all(collapsed.trim(), " ").to_string()
}

fn extract_area(text: &str) -> Option<String> {
    let re = Regex::new(r"Berört område:\s*</strong>\s*<span[^>]*>([^<]+)<").ok()?;
    re.captures(text).map(|c| c[1].trim().to_string())
}

fn centroid(points: &[LatLng]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let lat = points.iter().map(|p| p.lat).sum::<f64>() / n;
    let lng = points.iter().map(|p| p.lng).sum::<f64>() / n;
    Some((lat, lng))
}

fn to_event(provider: Provider, msg: &WebMessage, now: DateTime<Utc>) -> RawOutageEvent {
    let status = if now < msg.date_delay_utc {
        OutageStatus::Upcoming
    } else if now <= msg.date_expire_utc {
        if msg.title.contains("Planerat") {
            OutageStatus::Planned
        } else {
            OutageStatus::Fault
        }
    } else {
        OutageStatus::Resolved
    };

    let area_label = extract_area(&msg.text).unwrap_or_else(|| msg.title.clone());
    let coord = centroid(&msg.affected_addresses_coordinates);

    RawOutageEvent {
        provider,
        source_id: msg.id.to_string(),
        status,
        area_label,
        lat: coord.map(|(lat, _)| lat),
        lng: coord.map(|(_, lng)| lng),
        polygon: None,
        // No customer count field exists in this feed - the number of
        // affected *addresses* is the closest proxy available, and is
        // likely an undercount of actual customers per address.
        affected_customers: if msg.affected_addresses_coordinates.is_empty() {
            None
        } else {
            Some(msg.affected_addresses_coordinates.len() as i32)
        },
        reason: Some(strip_html(&msg.text)),
        started_at: Some(msg.date_delay_utc),
        estimated_end_at: Some(msg.date_expire_utc),
        observed_at: now,
    }
}

pub struct ServiceAlertAdapter {
    client: reqwest::Client,
    provider: Provider,
    adapter_name: &'static str,
    customer_id: String,
}

impl ServiceAlertAdapter {
    pub fn new(provider: Provider, adapter_name: &'static str, customer_id: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
            provider,
            adapter_name,
            customer_id: customer_id.to_string(),
        }
    }
}

#[async_trait]
impl Adapter for ServiceAlertAdapter {
    fn name(&self) -> &'static str {
        self.adapter_name
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = json!({
            "customerIds": self.customer_id,
            "internalOnly": false,
            "urlParams": { "customerId": self.customer_id },
        });

        let model: MapModel = self.client.post(ENDPOINT).json(&body).send().await?.json().await?;
        let now = Utc::now();

        let events = model
            .profiles
            .iter()
            .filter(|p| p.profile_title.as_deref() == Some(ELNAT_PROFILE_TITLE))
            .flat_map(|p| p.web_messages.iter())
            .map(|m| to_event(self.provider, m, now))
            .collect();

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_tags() {
        let html = "<p>Hello <strong>world</strong></p>";
        assert_eq!(strip_html(html), "Hello world");
    }

    #[test]
    fn extracts_affected_area() {
        let text = r#"<p><strong style="color: rgb(51, 51, 51);">Berört område:</strong><span style="color: rgb(51, 51, 51);"> Eskilstuna: Sundbyvägen</span></p>"#;
        assert_eq!(extract_area(text).as_deref(), Some("Eskilstuna: Sundbyvägen"));
    }

    #[test]
    fn before_delay_is_upcoming() {
        let now = Utc::now();
        let msg = WebMessage {
            id: 1,
            title: "Planerat avbrott".into(),
            text: String::new(),
            date_delay_utc: now + chrono::Duration::hours(1),
            date_expire_utc: now + chrono::Duration::hours(2),
            affected_addresses_coordinates: vec![],
        };
        assert_eq!(to_event(Provider::Karlstad, &msg, now).status, OutageStatus::Upcoming);
    }

    #[test]
    fn within_window_planned_title_is_planned() {
        let now = Utc::now();
        let msg = WebMessage {
            id: 1,
            title: "Planerat avbrott".into(),
            text: String::new(),
            date_delay_utc: now - chrono::Duration::hours(1),
            date_expire_utc: now + chrono::Duration::hours(1),
            affected_addresses_coordinates: vec![],
        };
        assert_eq!(to_event(Provider::Karlstad, &msg, now).status, OutageStatus::Planned);
    }

    #[test]
    fn within_window_non_planned_title_is_fault() {
        let now = Utc::now();
        let msg = WebMessage {
            id: 1,
            title: "Driftstörning".into(),
            text: String::new(),
            date_delay_utc: now - chrono::Duration::hours(1),
            date_expire_utc: now + chrono::Duration::hours(1),
            affected_addresses_coordinates: vec![],
        };
        assert_eq!(to_event(Provider::Karlstad, &msg, now).status, OutageStatus::Fault);
    }

    #[test]
    fn after_expiry_is_resolved() {
        let now = Utc::now();
        let msg = WebMessage {
            id: 1,
            title: "Planerat avbrott".into(),
            text: String::new(),
            date_delay_utc: now - chrono::Duration::hours(2),
            date_expire_utc: now - chrono::Duration::hours(1),
            affected_addresses_coordinates: vec![],
        };
        assert_eq!(to_event(Provider::Karlstad, &msg, now).status, OutageStatus::Resolved);
    }

    #[test]
    fn real_fixture_filters_to_elnat_only() {
        let json_str = include_str!("../tests/fixtures/eskilstuna_real_sample.json");
        let model: MapModel = serde_json::from_str(json_str).unwrap();
        let elnat_count = model
            .profiles
            .iter()
            .filter(|p| p.profile_title.as_deref() == Some(ELNAT_PROFILE_TITLE))
            .map(|p| p.web_messages.len())
            .sum::<usize>();
        // The captured fixture has no active Elnät messages (only
        // Fjärrvärme/Fibernät were live) - this just confirms the filter
        // doesn't crash and correctly finds zero, not some other count.
        assert_eq!(elnat_count, 0);
    }
}
