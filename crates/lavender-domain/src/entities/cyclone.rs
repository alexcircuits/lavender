//! NHC/CPHC advisories and geometry. Forecast cones describe centre-track uncertainty,
//! not the extent of wind, rain or other hazards.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum CycloneGeometryStatus {
    Pending,
    Current,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CycloneMovement {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction_degrees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub speed_kt: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CycloneForecastPoint {
    pub position: GeoPoint,
    pub tau_hours: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wind_kt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gust_kt: Option<f64>,
}

/// GeoJSON coordinates are longitude, latitude. Separate parts preserve dateline splits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "coordinates")]
#[ts(export)]
pub enum CycloneTrackGeometry {
    LineString(Vec<[f64; 2]>),
    MultiLineString(Vec<Vec<[f64; 2]>>),
}

/// Each polygon's first ring is its exterior; later rings are holes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "coordinates")]
#[ts(export)]
pub enum CycloneConeGeometry {
    Polygon(Vec<Vec<[f64; 2]>>),
    MultiPolygon(Vec<Vec<Vec<[f64; 2]>>>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cyclone {
    pub id: String,
    pub name: String,
    pub classification: String,
    pub basin: String,
    pub position: GeoPoint,
    pub position_at: Timestamp,
    pub advisory_number: String,
    pub issued_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub intensity_kt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pressure_mb: Option<f64>,
    pub movement: CycloneMovement,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub advisory_url: Option<String>,
    pub outlook_url: String,
    pub geometry_status: CycloneGeometryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub geometry_advisory_number: Option<String>,
    pub forecast_points: Vec<CycloneForecastPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub track: Option<CycloneTrackGeometry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cone: Option<CycloneConeGeometry>,
}
