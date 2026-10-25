//! Spatial search contracts for querying live entities across feeds within geographic boundaries.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, FeedId, GeoPoint};

/// Spatial search geometric filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SpatialFilter {
    /// Axis-aligned geographic bounding box.
    #[serde(rename = "bbox", rename_all = "camelCase")]
    BBox {
        #[serde(flatten)]
        bbox: BBox,
    },
    /// Radial circular area around a center point.
    #[serde(rename = "circle", rename_all = "camelCase")]
    Circle {
        center: GeoPoint,
        /// Radius in metres (minimum 1m, maximum 20,000,000m).
        #[serde(alias = "radius_m")]
        radius_m: f64,
    },
    /// Arbitrary polygon boundary ring.
    #[serde(rename = "polygon", rename_all = "camelCase")]
    Polygon {
        /// Sequence of vertices forming the boundary.
        points: Vec<GeoPoint>,
    },
    /// Route or corridor: linear path with a lateral buffer.
    #[serde(rename = "corridor", rename_all = "camelCase")]
    Corridor {
        /// Sequence of waypoints (at least 2 points).
        waypoints: Vec<GeoPoint>,
        /// Lateral buffer distance in metres from the path centerline.
        #[serde(alias = "buffer_m")]
        buffer_m: f64,
    },
}

/// Request for a spatial entity search across active feeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SpatialSearchRequest {
    /// Spatial filter geometry.
    pub filter: SpatialFilter,
    /// Target feeds to query. If None or empty, all active feeds with positions are queried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feeds: Option<Vec<FeedId>>,
    /// Maximum number of matching entities returned (default: 50, maximum: 500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// A matching live entity discovered within the spatial boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SpatialHit {
    /// Source feed identifier.
    pub feed: FeedId,
    /// Entity unique identifier (e.g. icao24, MMSI, event ID).
    pub id: String,
    /// Primary human-readable label (e.g. flight callsign, vessel name).
    pub label: String,
    /// Secondary detail (e.g. altitude, speed, magnitude).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Entity geographic coordinate.
    pub position: GeoPoint,
    /// Distance in metres from the query reference (center of circle or nearest waypoint), if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<f64>,
}

/// Response containing spatial search matches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SpatialSearchResponse {
    /// Matched entities up to the requested limit.
    pub hits: Vec<SpatialHit>,
    /// Total count of matching entities found.
    pub total_matched: usize,
    /// Whether the hits list was truncated due to the limit.
    pub truncated: bool,
    /// Informational notices or warnings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
}
