//! Travel route domain entities.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Maneuver step in a calculated route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteStep {
    pub index: usize,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub modifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit: Option<u32>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub r#ref: Option<String>,
    #[ts(type = "number")]
    pub distance_m: u64,
    #[ts(type = "number")]
    pub duration_s: u64,
    pub lon: f64,
    pub lat: f64,
    pub instruction: String,
}

/// A calculated route between coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Route {
    pub ok: bool,
    pub profile: String,
    #[ts(type = "number")]
    pub distance_m: u64,
    #[ts(type = "number")]
    pub duration_s: u64,
    pub geometry: Vec<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub steps: Option<Vec<RouteStep>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub steps_truncated: Option<bool>,
}
