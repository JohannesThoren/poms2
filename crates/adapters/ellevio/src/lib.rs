use async_trait::async_trait;
use chrono::Utc;
use poms_adapter_sdk::Adapter;
use poms_types::{OutageStatus, Provider, RawOutageEvent};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://avbrottskarta.ellevio.se";

/// Ellevio redesigned this site (2026-09). The root URL's server-rendered
/// "#prerendered" snapshot always shows a nationwide table - one row per
/// län currently in Ellevio's coverage, each linking to `/län/{slug}`.
/// Crucially, `/län/{slug}/idag` *does* properly server-render a full,
/// live per-kommun breakdown for that län (discovered from a real browser
/// screenshot after `/län/{slug}` alone - without the trailing `/idag` -
/// turned out to just be another instance of the same nationwide
/// fallback). This lets us drop the old approach entirely: instead of a
/// hand-maintained, increasingly-stale list of ~56 kommun slugs (many of
/// which stopped resolving after Ellevio's redesign and fell back to
/// nationwide/län-level data), we now discover the current län list from
/// the root page and pull real kommun-level counts straight out of each
/// län's own page - self-updating, and covering all ~76 kommuner
/// currently in Ellevio's territory (up from the ~56 we used to guess at,
/// across only 4 län - Ellevio's coverage now spans at least 7: Dalarna,
/// Gävleborg, Halland, Stockholm, Värmland, Västra Götaland, Örebro).
///
/// Both "Oplanerade" and "Planerade" are customer counts (not incident
/// counts) - confirmed because a single kommun's count can exactly equal
/// its whole län's total when it's the only affected place, which only
/// makes sense for customer counts.
struct TableRow {
    name: String,
    href: String,
    oplanerade: i32,
    planerade: i32,
}

fn text_of(el: &scraper::ElementRef) -> String {
    el.text().collect::<Vec<_>>().join(" ").trim().to_string()
}

/// Parses whichever `<table>` is in the "#prerendered" snapshot - the
/// same three-column (name/link, Oplanerade, Planerade) shape is used for
/// both the nationwide (län rows) and per-län (kommun rows) pages.
fn parse_table(html: &str) -> Vec<TableRow> {
    let document = Html::parse_document(html);
    let row_sel = Selector::parse("#prerendered table tbody tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let a_sel = Selector::parse("a").unwrap();

    let mut rows = Vec::new();
    for tr in document.select(&row_sel) {
        let tds: Vec<_> = tr.select(&td_sel).collect();
        if tds.len() != 3 {
            continue;
        }
        let Some(link) = tds[0].select(&a_sel).next() else { continue };
        let name = text_of(&link);
        let href = link.value().attr("href").unwrap_or("").to_string();
        let oplanerade: i32 = text_of(&tds[1]).parse().unwrap_or(0);
        let planerade: i32 = text_of(&tds[2]).parse().unwrap_or(0);
        if !name.is_empty() && !href.is_empty() {
            rows.push(TableRow { name, href, oplanerade, planerade });
        }
    }
    rows
}

fn status_for(oplanerade: i32, planerade: i32) -> Option<OutageStatus> {
    if oplanerade > 0 {
        Some(OutageStatus::Fault)
    } else if planerade > 0 {
        Some(OutageStatus::Planned)
    } else {
        None
    }
}

fn to_event(row: &TableRow, source_id: &str) -> Option<RawOutageEvent> {
    let status = status_for(row.oplanerade, row.planerade)?;
    let affected_customers = if row.oplanerade > 0 { row.oplanerade } else { row.planerade };
    Some(RawOutageEvent {
        provider: Provider::Ellevio,
        source_id: source_id.to_string(),
        status,
        area_label: row.name.clone(),
        lat: None,
        lng: None,
        polygon: None,
        affected_customers: Some(affected_customers),
        reason: None,
        started_at: None,
        estimated_end_at: None,
        observed_at: Utc::now(),
    })
}

pub struct EllevioAdapter {
    client: reqwest::Client,
}

impl EllevioAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("POMS2/0.1 (+https://github.com/JohannesThoren/poms2)")
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn fetch(&self, path: &str) -> anyhow::Result<Vec<TableRow>> {
        let body = self.client.get(format!("{BASE_URL}{path}")).send().await?.text().await?;
        Ok(parse_table(&body))
    }
}

impl Default for EllevioAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adapter for EllevioAdapter {
    fn name(&self) -> &'static str {
        "ellevio"
    }

    async fn poll(&self) -> anyhow::Result<Vec<RawOutageEvent>> {
        let lan_rows = self.fetch("/").await?;
        let mut events = Vec::new();

        for lan in &lan_rows {
            // Kommun-level detail from this län's own page - more useful
            // than the län aggregate alone, so we always drill down
            // rather than only doing so when the län shows nonzero
            // counts (a kommun could in principle have an outage even if
            // some rounding/timing quirk showed the län row as zero).
            match self.fetch(&lan.href).await {
                Ok(kommun_rows) if !kommun_rows.is_empty() => {
                    for kommun in &kommun_rows {
                        let source_id = kommun.name.to_lowercase();
                        if let Some(event) = to_event(kommun, &source_id) {
                            events.push(event);
                        }
                    }
                }
                Ok(_) => {
                    tracing::warn!(lan = lan.name, "län page had no kommun rows, using län-level aggregate");
                    let source_id = format!("lan-{}", lan.name.to_lowercase().replace(' ', "-"));
                    if let Some(event) = to_event(lan, &source_id) {
                        events.push(event);
                    }
                }
                Err(err) => {
                    tracing::warn!(lan = lan.name, error = %err, "failed to fetch län page, using län-level aggregate");
                    let source_id = format!("lan-{}", lan.name.to_lowercase().replace(' ', "-"));
                    if let Some(event) = to_event(lan, &source_id) {
                        events.push(event);
                    }
                }
            }
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nationwide_lan_table() {
        let html = include_str!("../tests/fixtures/knivsta_fallback.html");
        let rows = parse_table(html);
        assert_eq!(rows.len(), 7);
        let halland = rows.iter().find(|r| r.name.contains("Halland")).unwrap();
        assert_eq!(halland.oplanerade, 37);
        assert_eq!(halland.href, "/län/halland");
    }

    #[test]
    fn parses_real_lan_drilldown_page() {
        let html = include_str!("../tests/fixtures/dalarna_lan_idag.html");
        let rows = parse_table(html);
        assert_eq!(rows.len(), 8);
        let alvdalen = rows.iter().find(|r| r.name.contains("lvdalen")).unwrap();
        assert_eq!(alvdalen.oplanerade, 1);
        assert_eq!(alvdalen.href, "/kommun/älvdalen");
    }

    #[test]
    fn zero_counts_yield_no_event() {
        let row = TableRow { name: "Test".into(), href: "/kommun/test".into(), oplanerade: 0, planerade: 0 };
        assert!(to_event(&row, "test").is_none());
    }

    #[test]
    fn nonzero_oplanerade_is_fault_with_customer_count() {
        let row = TableRow { name: "Test".into(), href: "/kommun/test".into(), oplanerade: 5, planerade: 0 };
        let event = to_event(&row, "test").unwrap();
        assert_eq!(event.status, OutageStatus::Fault);
        assert_eq!(event.affected_customers, Some(5));
    }

    #[test]
    fn planerade_only_is_planned() {
        let row = TableRow { name: "Test".into(), href: "/kommun/test".into(), oplanerade: 0, planerade: 3 };
        let event = to_event(&row, "test").unwrap();
        assert_eq!(event.status, OutageStatus::Planned);
        assert_eq!(event.affected_customers, Some(3));
    }
}
