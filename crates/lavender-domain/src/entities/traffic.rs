//! Domain contracts for traffic flow tiles and budget governor.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Status of the traffic provider and its daily request budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrafficStatus {
    /// Whether an upstream API key is configured.
    pub has_key: bool,
    /// Number of upstream tile requests made today (UTC).
    pub daily_count: u64,
    /// Daily request budget limit.
    pub budget: u64,
    /// Current UTC date key ("YYYY-MM-DD").
    pub date: String,
}

/// A road segment annotated with traffic congestion information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrafficRoadSegment {
    /// Polyline coordinates as `[longitude, latitude]` pairs in degrees.
    pub coords: Vec<[f64; 2]>,
    /// Congestion level in range [0.0, 1.0], where 1.0 represents free-flow speed.
    pub traffic_level: f64,
    /// Whether this road segment is currently closed.
    pub closure: bool,
    /// Road type or class (e.g. "motorway", "primary", "secondary").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub road_type: Option<String>,
}

/// Response payload containing roads with matched traffic flow for a tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrafficTileResponse {
    /// Zoom level.
    pub z: u32,
    /// Column coordinate (X).
    pub x: u32,
    /// Row coordinate (Y).
    pub y: u32,
    /// Road segments with matched traffic flow.
    pub roads: Vec<TrafficRoadSegment>,
}
