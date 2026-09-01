mod kommuner;

use async_trait::async_trait;
use chrono::Utc;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::Html;

pub use kommuner::KOMMUNER;

const BASE_URL: &str = "https://avbrottskarta.ellevio.se";

pub struct EllevioAdapter {
    client: reqwest::Client,
}

impl EllevioAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/POMS2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Fetch and parse a single kommun's page. Returns `None` when there is
    /// no ongoing outage there (the page says so explicitly), rather than
    /// an event with a zero count - a "no outage" kommun shouldn't leave a
    /// row behind for the ingestion service to have to resolve later.
    async fn fetch_kommun(&self, kommun: &kommuner::Kommun) -> anyhow::Result<Option<RawOutageEvent>> {
        let url = format!("{BASE_URL}/kommun/{}/idag", kommun.slug);
        let body = self.client.get(&url).send().await?.text().await?;
        Self::parse_kommun_page(kommun.name, &body)
    }

    /// Pulled out of `fetch_kommun` so it can be unit tested against saved
    /// HTML fixtures without hitting the network.
    fn parse_kommun_page(kommun_name: &str, html: &str) -> anyhow::Result<Option<RawOutageEvent>> {
        let document = Html::parse_document(html);

        // The server-rendered shell either says "Inga kunder berörda..."
        // (nothing ongoing) or otherwise shows a customer count somewhere
        // in the page text near "kunder berörda". We match on the whole
        // document's text rather than a specific selector because the
        // exact DOM node holding the count isn't confirmed against a live
        // active outage yet - this is intentionally loose until we can
        // check that against real data, at which point tightening this to
        // a proper selector is a small follow-up, not a rewrite.
        let text: String = document.root_element().text().collect::<Vec<_>>().join(" ");

        if text.contains("Inga kunder berörda") {
            return Ok(None);
        }

        // Look for "<N> kund" (kunder/kund berörda) anywhere in the page.
        let count = extract_customer_count(&text);

        let Some(affected_customers) = count else {
            // Page didn't match either the "no outage" or "N kunder"
            // shape - log and skip rather than guessing.
            tracing::warn!(kommun = kommun_name, "could not parse customer count from page");
            return Ok(None);
        };

        if affected_customers == 0 {
            return Ok(None);
        }

        Ok(Some(RawOutageEvent {
            provider: Provider::Ellevio,
            // Ellevio only gives us an area-level aggregate, not
            // individual outage ids - so the kommun name itself is the
            // stable identity of "the current outage situation in this
            // kommun". Ingestion upserts on (provider, source_id), which
            // is exactly the collapsing behavior we want here.
            source_id: kommun_name.to_lowercase(),
            status: OutageStatus::Fault,
            area_label: kommun_name.to_string(),
            lat: None,
            lng: None,
            affected_customers: Some(affected_customers),
            reason: None,
            started_at: None,
            estimated_end_at: None,
            observed_at: Utc::now(),
        }))
    }
}

fn extract_customer_count(text: &str) -> Option<i32> {
    // Find a run of digits immediately followed by " kund" (kund/kunder).
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let rest = &text[i..];
            if rest.trim_start().starts_with("kund") {
                return text[start..i].parse().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

#[async_trait]
impl Adapter for EllevioAdapter {
    fn name(&self) -> &'static str {
        "ellevio"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let mut events = Vec::new();
        for kommun in KOMMUNER {
            match self.fetch_kommun(kommun).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(kommun = kommun.name, error = %err, "failed to fetch kommun page");
                }
            }
        }
        Ok(events)
    }
}

impl Default for EllevioAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_outage_page_returns_none() {
        let html = "<html><body>Inga kunder berörda av strömavbrott i kommunen just nu</body></html>";
        let result = EllevioAdapter::parse_kommun_page("Stockholm", html).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn extracts_customer_count() {
        assert_eq!(extract_customer_count("42 kunder berörda"), Some(42));
        assert_eq!(extract_customer_count("1 kund berörd"), Some(1));
        assert_eq!(extract_customer_count("no match here"), None);
    }

    #[test]
    fn outage_page_produces_event() {
        let html = "<html><body>Aktuella strömavbrott i Karlstad. 17 kunder berörda av strömavbrott.</body></html>";
        let result = EllevioAdapter::parse_kommun_page("Karlstad", html).unwrap().unwrap();
        assert_eq!(result.area_label, "Karlstad");
        assert_eq!(result.affected_customers, Some(17));
        assert_eq!(result.source_id, "karlstad");
    }
}
