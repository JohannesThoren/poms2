//! Regression test against a real response captured from Kraftringen's
//! live feed (2026-09-01) - catches the feed's shape changing under us,
//! not just our own parsing logic.

use poms_adapter_kraftringen::{parse_kml, to_event};
use poms_types::OutageStatus;

#[test]
fn parses_real_captured_feed() {
    let xml = include_str!("fixtures/real_sample.kml");
    let placemarks = parse_kml(xml).expect("should parse real feed");

    assert!(!placemarks.is_empty(), "real feed should contain at least one placemark");

    let events: Vec<_> = placemarks.iter().filter_map(to_event).collect();
    assert_eq!(events.len(), placemarks.len(), "every real placemark should have an outage_id");

    for event in &events {
        assert!(event.lat.is_some() && event.lng.is_some(), "every event should have coordinates");
        assert!(
            matches!(
                event.status,
                OutageStatus::Fault | OutageStatus::Planned | OutageStatus::Upcoming | OutageStatus::Resolved
            ),
            "status should be one of the known variants"
        );
    }

    // The captured sample is all resolved ("inactive_outage") plus one
    // not-yet-started planned outage - sanity check we're not
    // misclassifying everything as one status.
    let resolved_count = events.iter().filter(|e| e.status == OutageStatus::Resolved).count();
    let upcoming_count = events.iter().filter(|e| e.status == OutageStatus::Upcoming).count();
    assert!(resolved_count > 0, "expected at least one resolved outage in the sample");
    assert!(upcoming_count > 0, "expected at least one upcoming planned outage in the sample");
}
