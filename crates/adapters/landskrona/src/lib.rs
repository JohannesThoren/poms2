//! Landskrona Energi adapter.
//!
//! `/avbrott/` is a WordPress archive of a custom post type (permalinks
//! are `/avbrott/{slug}/`), but that type isn't registered for the REST
//! API (`/wp-json/wp/v2/types` doesn't list it, and no "avbrott" category
//! or tag exists either despite a genuine "elnät" category existing for
//! unrelated general news) - so this scrapes the archive HTML directly
//! rather than using the REST API.
//!
//! One shared feed covers electricity, stadsnät (fiber) and fjärrvärme
//! with no category field at all - only free text. Worse, several posts
//! are about **E.ON's** outages (Landskrona Energi is a separate legal
//! entity from E.ON but their stadsnät equipment depends on E.ON's grid
//! in the area) affecting Landskrona's own stadsnät/fiber - e.g.
//! "Driftstörning stadsnät i Glumslöv ... E.on har ett pågående
//! strömavbrott". These must be excluded too: they're not Landskrona
//! Energi's own electricity fault, and if we wanted E.ON's own outages
//! we'd read them from E.ON directly (currently blocked, see README).
//!
//! Each post is a rolling update log for one incident, newest update
//! prepended to the top - so "resolved" keywords anywhere in the text
//! are treated as resolved/skipped rather than only checking the very
//! start, erring toward not showing something as still-active that may
//! already be fixed.

use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{Html, Selector};

const URL: &str = "https://landskronaenergi.se/avbrott/";
const ELECTRICITY_KEYWORDS: [&str; 2] = ["ström", "elnät"];
const NON_ELECTRICITY_KEYWORDS: [&str; 7] =
    ["stadsnät", "fiber", "bredband", "fjärrvärme", "tv-tjänst", "internet", "telefoni"];
const RESOLVED_KEYWORDS: [&str; 5] = ["åtgärdat", "åter till alla", "strömmen åter", "avklarat", "klar kl"];

#[derive(Debug, PartialEq)]
struct Post {
    id: String,
    title: String,
    excerpt: String,
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_posts(html: &str) -> Vec<Post> {
    let document = Html::parse_document(html);
    let article_sel = Selector::parse("article.post").unwrap();
    let title_sel = Selector::parse(".entry-title a").unwrap();
    let excerpt_sel = Selector::parse("p").unwrap();

    let mut posts = Vec::new();
    for article in document.select(&article_sel) {
        let Some(link) = article.select(&title_sel).next() else { continue };
        let href = link.value().attr("href").unwrap_or("");
        // "/avbrott/{slug}/" - the slug itself is a stable, unique id.
        let id = href.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let title = text_of(&link);
        let excerpt = article.select(&excerpt_sel).next().map(|p| text_of(&p)).unwrap_or_default();

        posts.push(Post { id, title, excerpt });
    }
    posts
}

fn is_electricity(combined: &str) -> bool {
    let lower = combined.to_lowercase();
    let electricity = ELECTRICITY_KEYWORDS.iter().any(|k| lower.contains(k));
    let excluded = NON_ELECTRICITY_KEYWORDS.iter().any(|k| lower.contains(k));
    electricity && !excluded
}

fn is_eon_incident(combined: &str) -> bool {
    combined.to_lowercase().contains("e.on") || combined.to_lowercase().contains("eon ")
}

fn is_resolved(combined: &str) -> bool {
    let lower = combined.to_lowercase();
    RESOLVED_KEYWORDS.iter().any(|k| lower.contains(k))
}

fn to_event(post: &Post) -> Option<RawOutageEvent> {
    let combined = format!("{} {}", post.title, post.excerpt);
    if !is_electricity(&combined) || is_eon_incident(&combined) || is_resolved(&combined) {
        return None;
    }
    let status =
        if combined.to_lowercase().contains("planerat") { OutageStatus::Planned } else { OutageStatus::Fault };

    Some(RawOutageEvent {
        provider: Provider::Landskrona,
        source_id: post.id.clone(),
        status,
        area_label: post.title.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: None,
        reason: Some(post.excerpt.clone()),
        started_at: None,
        estimated_end_at: None,
        observed_at: chrono::Utc::now(),
    })
}

pub struct LandskronaAdapter {
    client: reqwest::Client,
}

impl LandskronaAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Default for LandskronaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Adapter for LandskronaAdapter {
    fn name(&self) -> &'static str {
        "landskrona"
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
    fn stadsnat_is_filtered() {
        let combined = "Driftstörning stadsnät i Glumslöv";
        assert!(!is_electricity(combined));
    }

    #[test]
    fn eon_incident_is_excluded_even_if_electricity_keyword_present() {
        let post = Post {
            id: "1".into(),
            title: "Driftstörning stadsnät i Glumslöv".into(),
            excerpt: "Strömavbrott Eon Info: E.on har ett pågående strömavbrott".into(),
        };
        assert!(to_event(&post).is_none());
    }

    #[test]
    fn resolved_post_is_skipped() {
        let post = Post {
            id: "1".into(),
            title: "Strömavbrott – Östra Landskrona".into(),
            excerpt: "13.00 Efter felsökning och åtgärd är strömmen åter till alla.".into(),
        };
        assert!(to_event(&post).is_none());
    }

    #[test]
    fn active_own_fault_is_fault() {
        let post = Post {
            id: "1".into(),
            title: "Strömavbrott Viktoriagatan".into(),
            excerpt: "Vi har just nu ett strömavbrott som påverkar delar av Viktoriagatan. Felsökning pågår.".into(),
        };
        let event = to_event(&post).unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
    }

    #[test]
    fn planerat_underhall_is_planned() {
        let post = Post {
            id: "1".into(),
            title: "Planerat underhållsarbete på Gärdesgatan".into(),
            excerpt: "Tisdag kommer vi utföra underhållsarbeten som medför ett planerat strömavbrott.".into(),
        };
        let event = to_event(&post).unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
    }

    #[test]
    fn real_fixture_parses_and_filters_correctly() {
        let html = include_str!("../tests/fixtures/real_sample.html");
        let posts = parse_posts(html);
        assert_eq!(posts.len(), 9, "expected 9 posts in the real fixture");

        // The real fixture's first post is an already-resolved own fault,
        // and its second is an E.ON-caused stadsnät issue - neither
        // should survive, proving the filters engage on real markup.
        assert!(to_event(&posts[0]).is_none());
        assert!(to_event(&posts[1]).is_none());
    }
}
