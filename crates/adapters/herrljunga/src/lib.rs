//! Herrljunga Elektriska adapter.
//!
//! Server-rendered WordPress custom post type ("servicestatus") at
//! `/storning/` - no separate API, the HTML itself has everything:
//!
//!   <article class="post-XXXX servicestatus type-servicestatus ...">
//!     <time datetime="2026-09-10 7:46">...</time>
//!     <h2 class="entry-title"><a>[KLAR] Strömavbrott Remmene med omnejd</a></h2>
//!     <div class="entry-content"><p>free-text updates...</p></div>
//!   </article>
//!
//! Same feed covers **both electricity and water** disruptions with no
//! separate category field - only the free-text title says which. This
//! adapter keeps a post only when the title contains an electricity
//! keyword ("ström"/"elavbrott"/"elnät") and skips anything mentioning
//! water ("vatten") to avoid miscategorizing a water-network post as a
//! power outage. Ambiguous titles (neither keyword) are skipped rather
//! than guessed.
//!
//! No coordinates, no affected-customer count, no clean planned/unplanned
//! field - status is inferred from a literal `[KLAR]`/`[KLART]` prefix in
//! the title (closed - both spellings mean the same "resolved", just
//! different Swedish grammatical agreement) vs its absence (still
//! active/Fault). A closed post is skipped entirely rather than emitted
//! as `Resolved`: if we never saw it while active, there's nothing to
//! resolve.

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe::Stockholm;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{Html, Selector};

const URL: &str = "https://www.el.herrljunga.se/storning/";

fn parse_stockholm(s: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M").ok()?;
    match Stockholm.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => None,
    }
}

#[derive(Debug, PartialEq)]
struct Post {
    id: String,
    title: String,
    body: String,
    published_at: Option<DateTime<Utc>>,
    closed: bool,
}

fn is_electricity(title: &str) -> bool {
    let lower = title.to_lowercase();
    let water = lower.contains("vatten");
    let electricity = lower.contains("ström") || lower.contains("elavbrott") || lower.contains("elnät");
    electricity && !water
}

fn parse_posts(html: &str) -> Vec<Post> {
    let document = Html::parse_document(html);
    let article_sel = Selector::parse("article.servicestatus").unwrap();
    let time_sel = Selector::parse("time").unwrap();
    let title_sel = Selector::parse(".entry-title a").unwrap();
    let body_sel = Selector::parse(".entry-content").unwrap();

    let mut posts = Vec::new();
    for article in document.select(&article_sel) {
        let id = article
            .value()
            .attr("id")
            .map(|s| s.trim_start_matches("post-").to_string())
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }

        let raw_title = article
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();
        // Some posts use "[KLAR]" and others "[KLART]" for the same
        // "resolved" meaning (Swedish grammatical agreement, not two
        // different states) - checking the "[KLAR" prefix catches both.
        // The title text itself is only used for posts we actually emit
        // (closed ones are always filtered out below), so no stripping
        // is needed for the non-closed case.
        let closed = raw_title.to_uppercase().starts_with("[KLAR");
        let title = raw_title.clone();

        let published_at = article
            .select(&time_sel)
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .and_then(parse_stockholm);

        let body = article
            .select(&body_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        posts.push(Post { id, title, body, published_at, closed });
    }
    posts
}

fn to_event(post: &Post) -> Option<RawOutageEvent> {
    if post.closed || !is_electricity(&post.title) {
        return None;
    }
    Some(RawOutageEvent {
        provider: Provider::Herrljunga,
        source_id: post.id.clone(),
        status: OutageStatus::Fault,
        area_label: post.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: (!post.body.is_empty()).then(|| post.body.clone()),
        started_at: post.published_at,
        estimated_end_at: None,
        observed_at: Utc::now(),
    })
}

pub struct HerrljungaAdapter {
    client: reqwest::Client,
}

impl HerrljungaAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for HerrljungaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for HerrljungaAdapter {
    fn name(&self) -> &'static str {
        "herrljunga"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let body = self.client.get(URL).send().await?.text().await?;
        let posts = parse_posts(&body);
        Ok(posts.iter().filter_map(to_event).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electricity_keyword_detection() {
        assert!(is_electricity("[KLAR] Strömavbrott Remmene med omnejd"));
        assert!(!is_electricity("Störning i vattennätet kring Armaturvägen i Annelund"));
        assert!(!is_electricity("Harabergsgatan med omnejd- störning i dricksvattenleverans"));
        assert!(!is_electricity("Något helt annat"));
    }

    #[test]
    fn closed_post_is_skipped() {
        let post = Post {
            id: "1".into(),
            title: "Strömavbrott X".into(),
            body: "".into(),
            published_at: None,
            closed: true,
        };
        assert!(to_event(&post).is_none());
    }

    #[test]
    fn open_electricity_post_is_fault() {
        let post = Post {
            id: "1".into(),
            title: "Strömavbrott X med omnejd".into(),
            body: "Felsökning pågår.".into(),
            published_at: None,
            closed: false,
        };
        let event = to_event(&post).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.reason.as_deref(), Some("Felsökning pågår."));
    }

    #[test]
    fn water_post_is_never_emitted() {
        let post = Post {
            id: "1".into(),
            title: "Störning i vattennätet".into(),
            body: "".into(),
            published_at: None,
            closed: false,
        };
        assert!(to_event(&post).is_none());
    }

    #[test]
    fn parses_single_digit_hour() {
        let dt = parse_stockholm("2026-09-10 7:46").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-09-10T05:46:00+00:00");
    }

    #[test]
    fn klart_variant_is_also_detected_as_closed() {
        let html = r#"
            <article id="post-1" class="servicestatus">
                <time datetime="2026-09-10 7:46"></time>
                <h2 class="entry-title"><a>[KLART] Strömavbrott Fåglavik Källeryd mfl</a></h2>
                <div class="entry-content"><p>Åtgärdat.</p></div>
            </article>
        "#;
        let posts = parse_posts(html);
        assert_eq!(posts.len(), 1);
        assert!(posts[0].closed, "\"[KLART]\" must be detected as closed, same as \"[KLAR]\"");
        assert!(to_event(&posts[0]).is_none());
    }

    #[test]
    fn real_fixture_parses_without_panicking() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let posts = parse_posts(html);
        assert!(!posts.is_empty(), "expected at least one post in the real fixture");
        // The fixture's first post is a water disruption and a later one
        // is a closed ("[KLAR]") electricity post - neither should ever
        // become an event, proving the filters actually engage on real
        // markup rather than only on hand-written test structs.
        let water_titles_present = posts.iter().any(|p| p.title.to_lowercase().contains("vatten"));
        let closed_present = posts.iter().any(|p| p.closed);
        assert!(water_titles_present, "fixture should contain a water post");
        assert!(closed_present, "fixture should contain a closed post");
        for post in &posts {
            if post.title.to_lowercase().contains("vatten") || post.closed {
                assert!(to_event(post).is_none());
            }
        }
    }
}
