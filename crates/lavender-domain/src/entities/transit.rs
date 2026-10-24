//! Transit domain entities for GTFS-Realtime feeds, vehicle positions, and historical trails.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// A registered GTFS-Realtime transit feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TransitFeed {
    /// Unique feed identifier (e.g. `mbta`, `hsl-helsinki`).
    pub id: String,
    /// Human-readable agency or feed name (e.g. "MBTA", "HSL").
    pub name: String,
    /// Operating agency organization.
    pub operator: String,
    /// Geographic region served (e.g. "Boston, MA", "Helsinki, Finland").
    pub region: String,
    /// Center coordinates of the transit coverage area.
    pub center: GeoPoint,
    /// Loading / viewport query radius around the center in kilometers.
    pub load_radius_km: f64,
    /// Upstream GTFS-RT endpoint URL.
    pub url: String,
    /// Open data license name.
    pub license: String,
    /// Attribution text for credits.
    pub attribution: String,
    /// Whether the feed is enabled by default.
    pub default_enabled: bool,
    /// Default transit mode (e.g. "bus", "rail", "tram", "subway", "ferry").
    pub default_mode: String,
}

/// A normalized live public transit vehicle position fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TransitVehicle {
    /// Unique render key in `feed_id:vehicle_id` format.
    pub id: String,
    /// Vehicle identifier within the operator's feed.
    pub vehicle_id: String,
    /// Registry feed identifier (e.g. `mbta`).
    pub feed_id: String,
    /// Geographic position of the vehicle.
    pub position: GeoPoint,
    /// Compass heading / bearing in degrees (0..360) clockwise from true north.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bearing: Option<f32>,
    /// Speed over ground in metres per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub speed_mps: Option<f32>,
    /// Observation timestamp in Unix epoch seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timestamp: Option<i64>,
    /// Provenance ladder of the timestamp: "vehicle", "header", or "fetch".
    pub timestamp_source: String,
    /// Route identifier if assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route_id: Option<String>,
    /// Active trip identifier if assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trip_id: Option<String>,
    /// Trip direction identifier (typically 0 or 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction_id: Option<u32>,
    /// Operator public vehicle label / number.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// Next or current scheduled stop identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop_id: Option<String>,
    /// Current vehicle stop status (e.g. `INCOMING_AT`, `STOPPED_AT`, `IN_TRANSIT_TO`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
    /// Vehicle occupancy status if reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub occupancy: Option<String>,
    /// Resolved transit vehicle mode (e.g. "bus", "tram", "subway", "rail", "ferry", "unknown").
    pub mode: String,
}

/// Operational context epoch for a historical vehicle trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TransitEpoch {
    /// Epoch identifier referenced by fix records.
    pub id: u32,
    /// Active trip identifier during this epoch.
    pub trip: String,
    /// Active route identifier during this epoch.
    pub route: String,
    /// Transit mode during this epoch.
    pub mode: String,
}

/// Historical position trail for a single transit vehicle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TransitTrail {
    /// Schema format version (always 1).
    pub version: u32,
    /// Registry feed identifier.
    pub feed_id: String,
    /// Vehicle identifier.
    pub vehicle_id: String,
    /// Chronological list of position fixes: `[time_ms, lat, lon, flags, epoch_id]`.
    pub fixes: Vec<[f64; 5]>,
    /// Epoch context records describing trip, route, and mode changes.
    pub epochs: Vec<TransitEpoch>,
    /// Whether older fixes were evicted due to the 128-fix ring buffer cap.
    pub truncated: bool,
}
