//! Contacts / Awareness contracts (`LB-123`): the focus, history and navigation model that
//! backs the compact situational-awareness panel (`legacy/src/layers/awareness/model.js`).

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The contact the awareness panel is centred on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessSubject {
    /// Stable identifier of the subject within its source layer.
    pub id: String,
    /// Operator-facing label, when the source supplies one.
    pub label: Option<String>,
}

/// Per-layer cohort evaluated around the subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessCohort {
    /// Layer id (`flights`, `military`, `ais-live-vessels`, ...).
    pub id: String,
    /// Operator-facing layer label.
    pub label: String,
    /// Upstream source name.
    pub source: String,
    /// Coverage note for the cohort, when known.
    pub coverage: Option<String>,
    /// Relationship to the subject (for example `within range`).
    pub relationship: String,
    /// Contact count; `None` when the feed cannot answer (reported as unknown, never zero).
    pub count: Option<u32>,
    /// Whether the count is exact rather than a lower bound.
    pub complete: bool,
    /// Whether the nearest list was cut at the navigation cap.
    pub truncated: bool,
    /// Why the cohort is empty or incomplete, when applicable.
    pub reason: Option<String>,
    /// Nearest contact identifiers, closest first.
    pub nearest: Vec<String>,
}

/// Navigation affordances of the awareness panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessNavigation {
    pub can_previous: bool,
    pub can_focus: bool,
    pub can_next: bool,
}

/// One evaluation of every cohort around the subject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessResults {
    pub subject: AwarenessSubject,
    pub evaluated_at: Timestamp,
    /// Evaluation radius in metres.
    pub radius_m: Option<f64>,
    pub cohorts: Vec<AwarenessCohort>,
}

/// Read-only awareness snapshot shared with compact HUD consumers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessContextSnapshot {
    pub subject: AwarenessSubject,
    /// Whether the source still reports the subject; `false` renders a CONTACT LOST cue.
    pub subject_present: bool,
    pub evaluated_at: Timestamp,
    pub radius_m: Option<f64>,
    pub cohorts: Vec<AwarenessCohort>,
    pub navigation: AwarenessNavigation,
}

/// Panel-equivalent per-layer counts; `None` means unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactsWindow {
    pub centered_on: Option<String>,
    pub radius_km: Option<u32>,
    pub aircraft: Option<u32>,
    pub flights: Option<u32>,
    pub military: Option<u32>,
    pub vessels: Option<u32>,
    /// Whether both aircraft cohorts answered with exact counts.
    pub complete: bool,
}

/// Per-frame verdict on camera motion and readout refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AwarenessRefreshDecision {
    pub moving: bool,
    pub refresh: bool,
}
