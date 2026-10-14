//! Provider health and feed usage reported by the core to operators and the interface.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Availability, FeedId, LicenceMode};

/// The health of one data source, judged from recent fetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HealthState {
    /// No fetch has been attempted yet (nobody has subscribed).
    Idle,
    Healthy,
    /// The latest attempt failed but an earlier one succeeded recently.
    Degraded,
    /// Several consecutive attempts failed, or nothing has ever succeeded.
    Failing,
    /// The source is disabled by configuration or licence policy.
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderHealth {
    pub source_id: String,
    pub state: HealthState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_success: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_attempt: Option<Timestamp>,
    /// The last failure, phrased for an operator (no secrets, no upstream bodies).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
    /// Median fetch latency over the recent window, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub latency_ms_p50: Option<u32>,
}

/// Per-feed availability and current load.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FeedStatus {
    pub feed: FeedId,
    pub availability: Availability,
    /// Refresh tasks currently running for this feed (one per region key).
    pub active_regions: u32,
    pub subscribers: u32,
}

/// The whole system's status, for the status bar and the Sources panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemStatus {
    pub version: String,
    pub licence_mode: LicenceMode,
    pub started_at: Timestamp,
    pub sources: Vec<ProviderHealth>,
    pub feeds: Vec<FeedStatus>,
}
