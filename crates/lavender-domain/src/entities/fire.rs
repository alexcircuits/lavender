//! Validated active-fire detection records and aggregated fire cells (NASA FIRMS).

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An individual active-fire detection from NASA FIRMS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FireDetection {
    pub lat: f64,
    pub lon: f64,
    pub brightness: f64,
    pub frp: f64,
    pub confidence: String,
    pub satellite: String,
    pub acquired_at: Timestamp,
    pub day_night: String,
}

/// A spatial grid cell aggregating active-fire detections (or an individual detection with count = 1).
///
/// Merging cells adds their counts and total FRP, keeps the highest peak FRP and
/// brightness and the newest acquisition time; the other descriptive fields come from
/// the detection with the highest FRP. Exact duplicates are the same detections and
/// count once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FireCell {
    /// Detection position, or the count-weighted centroid of a cell.
    pub lat: f64,
    pub lon: f64,
    pub brightness: f64,
    /// Fire radiative power in MW: the detection's own, or the total of a cell.
    pub frp: f64,
    pub confidence: String,
    pub satellite: String,
    pub acquired_at: Timestamp,
    pub day_night: String,
    /// Number of detections represented (1 for an individual detection).
    pub count: u32,
    /// Highest FRP (MW) of any single detection represented.
    pub max_frp: f64,
    /// Newest acquisition time of any detection represented.
    pub latest_acquired_at: Timestamp,
}

/// An alias for [`FireCell`], representing a cluster of fire detections.
pub type FireCluster = FireCell;

impl From<FireDetection> for FireCell {
    fn from(d: FireDetection) -> Self {
        Self {
            lat: d.lat,
            lon: d.lon,
            brightness: d.brightness,
            frp: d.frp,
            confidence: d.confidence,
            satellite: d.satellite,
            acquired_at: d.acquired_at,
            day_night: d.day_night,
            count: 1,
            max_frp: d.frp,
            latest_acquired_at: d.acquired_at,
        }
    }
}
