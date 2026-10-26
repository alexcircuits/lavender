//! Automated License Plate Reader (`ALPR`) surveillance camera domain entities.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, GeoPoint};

/// A community-mapped `OpenStreetMap` automated license plate reader (`ALPR`) camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlprCamera {
    /// Stable camera identifier (`"alpr:{osm_id}"`).
    pub id: String,
    /// Positive `OpenStreetMap` node identifier.
    pub osm_id: i64,
    /// Validated WGS84 camera coordinates.
    pub position: GeoPoint,
    /// Operating agency or entity (`operator` tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub operator: Option<String>,
    /// Hardware manufacturer or vendor (`manufacturer` tag, e.g. `"Flock Safety"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub manufacturer: Option<String>,
    /// Camera mount or enclosure type (`camera:type` tag, e.g. `"fixed"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub camera_type: Option<String>,
    /// Monitored zone description (`surveillance:zone` tag, e.g. `"traffic"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub zone: Option<String>,
    /// Camera facing bearing in degrees `[0, 360)` (`camera:direction` or `direction` tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction_deg: Option<f64>,
    /// External reference or asset tag (`ref` tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ref_code: Option<String>,
    /// Last verification or survey date (`check_date` or `survey:date` tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_verified: Option<String>,
    /// Mapping source attribution (`source` tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
}

/// Viewport-bounded snapshot of `ALPR` surveillance camera records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlprSnapshot {
    /// Timestamp when this snapshot was evaluated.
    pub as_of: Timestamp,
    /// Bounding box covered by this snapshot.
    pub bbox: BBox,
    /// Whether the snapshot originates from a stale cache.
    pub stale: bool,
    /// Whether the query reached its element cap and coverage may be truncated.
    pub saturated: bool,
    /// Validated `ALPR` camera records within the query bounds.
    pub records: Vec<AlprCamera>,
}
