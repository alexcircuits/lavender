use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatelliteTrackRequest {
    pub norad_id: u32,
    /// Start of the track; defaults to now.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<Timestamp>,
    /// Track length in minutes (1–1440, default one orbit capped at 120).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minutes: Option<u32>,
    /// Step between points in seconds (10–600, default 60).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub step_s: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackPoint {
    pub time: Timestamp,
    /// Sub-satellite point with altitude in metres.
    pub position: GeoPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatelliteTrack {
    pub norad_id: u32,
    pub name: String,
    pub points: Vec<TrackPoint>,
    /// Orbital period in minutes.
    pub period_min: f64,
}
