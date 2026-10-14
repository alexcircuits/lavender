//! Aircraft observations and satellite ground tracks returned by the Rust core.

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
    /// Geographic position with altitude in metres when known.
    pub position: GeoPoint,
}

/// Retained aircraft observations from shared feed refreshes, ordered oldest to newest.
/// This is a bounded trail collected since this process started, not upstream backfill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AircraftTrack {
    /// Lowercase transponder address, including '~' for a non-ICAO TIS-B address.
    pub hex: String,
    pub points: Vec<TrackPoint>,
    /// Source ids that reported the retained points; use the catalog for attribution.
    pub sources: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatellitePassRequest {
    pub norad_id: u32,
    /// WGS84 observer; altitude defaults to zero and must be -500–10,000 metres.
    pub observer: GeoPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<Timestamp>,
    /// Search window in hours (1–24, default 24).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hours: Option<u32>,
    /// Geometric elevation defining rise/set (0–89 degrees, default 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_elevation_deg: Option<f64>,
}

/// An interval at or above the requested geometric elevation, clipped to the window.
/// Atmospheric refraction, sunlight and naked-eye visibility are not modelled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatellitePass {
    /// Threshold crossing, or the window start when alreadyAbove is true.
    pub rise: Timestamp,
    /// Highest geometric elevation within this interval of the requested window.
    pub peak: Timestamp,
    /// Threshold crossing, or the window end when continuesAfterWindow is true.
    pub set: Timestamp,
    pub max_elevation_deg: f64,
    pub already_above: bool,
    pub continues_after_window: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatellitePasses {
    pub norad_id: u32,
    pub name: String,
    pub observer: GeoPoint,
    pub from: Timestamp,
    pub until: Timestamp,
    pub min_elevation_deg: f64,
    pub passes: Vec<SatellitePass>,
}
