//! Validated space launch records shared by providers, transports and the frontend.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Space launch pad location and name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LaunchPad {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location_name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

/// A space launch mission from Launch Library 2 with matched satellite catalogue references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Launch {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub net: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub window_start: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub window_end: Option<Timestamp>,
    /// COSPAR launch designator such as `2026-230`, assigned once a launch reached orbit.
    /// Satellites from the launch carry it as the prefix of their international designator.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub launch_designator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rocket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mission: Option<String>,
    /// The mission summary published by Launch Library 2.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mission_description: Option<String>,
    /// Target orbit, for example "Low Earth Orbit".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub orbit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pad: Option<LaunchPad>,
    #[serde(default)]
    pub payload_names: Vec<String>,
    /// Catalogue numbers of tracked satellites this launch carried, matched on the server.
    #[serde(default)]
    pub matched_norad_ids: Vec<u32>,
}
