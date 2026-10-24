//! Domain contracts for terrain point-height queries.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request payload for batch terrain heights lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerrainHeightsRequest {
    /// Points as `[longitude, latitude]` coordinate pairs in degrees.
    pub points: Vec<[f64; 2]>,
}

/// A resolved ground-height datum for one requested coordinate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerrainPointResult {
    /// Optional identifier if provided upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,

    /// Ellipsoidal height above WGS84 ellipsoid in meters.
    pub ellipsoid: f64,

    /// Elevation above mean sea level (orthometric height) in meters.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub elevation: Option<f64>,

    /// Geoid undulation (separation between geoid and ellipsoid) in meters.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub geoid: Option<f64>,
}

/// Response payload containing terrain height results in exact request order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerrainHeightsResponse {
    /// Results corresponding 1:1 with requested points in input order.
    pub results: Vec<TerrainPointResult>,
}
