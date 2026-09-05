//! Västra Orusts Energitjänst adapter.
//!
//! Their site loads `https://voe.se/mirakel/news.json` (no auth) - "Mirakel"
//! appears to be the platform/vendor name behind their `drift.js` widget.
//! The response groups outage records ("groups") under `utilities.el` for
//! electricity (other utility keys may exist for other services, not seen
//! here), each with real UTC timestamps (`Z` suffix - no DST guessing
//! needed) and human-written `deliveries` (subject/message pairs) rather
//! than area names or customer counts. There's no customer-count field at
//! all, so `affected_customers` is always `None` here - `area_label` comes
//! from the first delivery's subject line, which in practice already
//! names the affected place (e.g. "Planerat elavbrott Huseby 7
//! september").

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use serde::Deserialize;
use std::collections::HashMap;

const NEWS_URL: &str = "https://voe.se/mirakel/news.json";

#[derive(Debug, Deserialize)]
struct NewsResponse {
    utilities: HashMap<String, Utility>,
}

#[derive(Debug, Deserialize)]
struct Utility {
    groups: Vec<Group>,
}

#[derive(Debug, Deserialize)]
struct Group {
    #[serde(rename = "_id")]
    id: String,
    deliveries: Vec<Delivery>,
    #[serde(rename = "type")]
    group_type: String,
    #[serde(rename = "startTime")]
    start_time: DateTime<Utc>,
    #[serde(rename = "endTime")]
    end_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct Delivery {
    subject: String,
    message: String,
}

fn map_status(group: &Group, now: DateTime<Utc>) -> OutageStatus {
    if now > group.end_time {
        OutageStatus::Resolved
    } else if now < group.start_time {
        OutageStatus::Upcoming
    } else if group.group_type == "unplanned" {
        OutageStatus::Fault
    } else {
        OutageStatus::Planned
    }
}

fn to_event(group: &Group, now: DateTime<Utc>) -> RawOutageEvent {
    let first = group.deliveries.first();

    RawOutageEvent {
        provider: Provider::Voe,
        source_id: group.id.clone(),
        status: map_status(group, now),
        area_label: first.map(|d| d.subject.clone()).unwrap_or_else(|| format!("Avbrott #{}", group.id)),
        lat: None,
        lng: None,
        polygon: None,
        // No customer-count field exists in this feed at all.
        affected_customers: None,
        reason: first.map(|d| d.message.clone()),
        started_at: Some(group.start_time),
        estimated_end_at: Some(group.end_time),
        observed_at: now,
    }
}

pub struct VoeAdapter {
    client: reqwest::Client,
}

impl VoeAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for VoeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for VoeAdapter {
    fn name(&self) -> &'static str {
        "voe"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let response: NewsResponse = self.client.get(NEWS_URL).send().await?.json().await?;
        let now = Utc::now();

        let Some(el) = response.utilities.get("el") else {
            return Ok(Vec::new());
        };

        Ok(el.groups.iter().map(|g| to_event(g, now)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_start_is_upcoming() {
        let now = Utc::now();
        let group = Group {
            id: "1".into(),
            deliveries: vec![],
            group_type: "planned".into(),
            start_time: now + chrono::Duration::days(1),
            end_time: now + chrono::Duration::days(1) + chrono::Duration::hours(2),
        };
        assert_eq!(map_status(&group, now), OutageStatus::Upcoming);
    }

    #[test]
    fn past_end_is_resolved() {
        let now = Utc::now();
        let group = Group {
            id: "1".into(),
            deliveries: vec![],
            group_type: "unplanned".into(),
            start_time: now - chrono::Duration::hours(3),
            end_time: now - chrono::Duration::hours(1),
        };
        assert_eq!(map_status(&group, now), OutageStatus::Resolved);
    }

    #[test]
    fn within_window_unplanned_is_fault() {
        let now = Utc::now();
        let group = Group {
            id: "1".into(),
            deliveries: vec![],
            group_type: "unplanned".into(),
            start_time: now - chrono::Duration::hours(1),
            end_time: now + chrono::Duration::hours(1),
        };
        assert_eq!(map_status(&group, now), OutageStatus::Fault);
    }

    #[test]
    fn within_window_planned_is_planned() {
        let now = Utc::now();
        let group = Group {
            id: "1".into(),
            deliveries: vec![],
            group_type: "planned".into(),
            start_time: now - chrono::Duration::hours(1),
            end_time: now + chrono::Duration::hours(1),
        };
        assert_eq!(map_status(&group, now), OutageStatus::Planned);
    }

    #[test]
    fn area_label_uses_first_delivery_subject() {
        let now = Utc::now();
        let group = Group {
            id: "1".into(),
            deliveries: vec![Delivery { subject: "Planerat elavbrott Huseby".into(), message: "text".into() }],
            group_type: "planned".into(),
            start_time: now,
            end_time: now + chrono::Duration::hours(1),
        };
        assert_eq!(to_event(&group, now).area_label, "Planerat elavbrott Huseby");
    }

    #[test]
    fn real_fixture_parses() {
        let json = include_str!("../tests/fixtures/real_sample.json");
        let response: NewsResponse = serde_json::from_str(json).unwrap();
        let el = response.utilities.get("el").unwrap();
        assert_eq!(el.groups.len(), 3);
        let now = Utc::now();
        for g in &el.groups {
            let event = to_event(g, now);
            assert_eq!(event.provider, Provider::Voe);
        }
    }
}
