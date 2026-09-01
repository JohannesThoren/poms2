//! Shared domain types for POMS2 (Power Outage Monitoring System).
//!
//! Every adapter (one per grid operator / nätägare) normalizes whatever
//! shape its source uses into a `RawOutageEvent`, and pushes those through
//! `poms-adapter-sdk`'s `PostgresEventSink` into the `staged_events` table.
//! The ingestion service later normalizes staged events into the long-lived
//! `outages` table.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Which grid operator (nätägare) an event came from.
///
/// Stored as plain text in the DB (not a Postgres enum) so adding a new
/// provider never requires a migration - just a new match arm here and a
/// new adapter crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Ellevio,
    Vattenfall,
    Kraftringen,
    Jamtkraft,
    TekniskaVerken,
    Oresundskraft,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Ellevio => "ellevio",
            Provider::Vattenfall => "vattenfall",
            Provider::Kraftringen => "kraftringen",
            Provider::Jamtkraft => "jamtkraft",
            Provider::TekniskaVerken => "tekniska_verken",
            Provider::Oresundskraft => "oresundskraft",
        }
    }
}

/// Lifecycle status of an outage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutageStatus {
    /// Unplanned, currently ongoing ("fel", "pågående avbrott").
    Fault,
    /// Planned maintenance, currently ongoing.
    Planned,
    /// Planned maintenance that hasn't started yet.
    Upcoming,
    /// No longer active (resolved / ended). Adapters that can't tell us
    /// this directly rely on the ingestion service's staleness sweep.
    Resolved,
}

/// A single normalized outage event, as produced by an adapter.
///
/// `source_id` + `provider` together must be stable and unique for the same
/// real-world outage across polls, so the ingestion service can upsert
/// instead of duplicating rows. Adapters that lack a natural id from their
/// source (e.g. Jämtkraft) build one deterministically (e.g. hash of
/// location + start time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawOutageEvent {
    pub provider: Provider,
    /// Stable id for this outage as known to the source (or a deterministic
    /// synthetic id if the source doesn't provide one).
    pub source_id: String,
    pub status: OutageStatus,
    /// Free-text area / locality name as given by the source (e.g. a
    /// kommun, tätort, or address). Always present even when we also have
    /// coordinates, since it's shown in the UI list.
    pub area_label: String,
    /// Precise coordinates, when the source provides them (or we can derive
    /// them, e.g. from a polygon centroid). `None` for sources that only
    /// give area-level aggregates (e.g. Ellevio).
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// Number of affected customers, when known.
    pub affected_customers: Option<i32>,
    pub reason: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub estimated_end_at: Option<DateTime<Utc>>,
    /// When the adapter observed this event. Used by the ingestion
    /// service's staleness sweep to detect resolved outages that a source
    /// simply stops reporting instead of marking resolved explicitly.
    pub observed_at: DateTime<Utc>,
}
