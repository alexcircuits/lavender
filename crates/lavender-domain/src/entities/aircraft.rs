//! Validated aircraft records shared by providers, transports and the frontend.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// An aircraft contact from ADS-B/MLAT aggregation. Units follow aviation practice
/// (feet, knots, feet per minute) and are named in the field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Aircraft {
    /// 24-bit ICAO address in lowercase hex, the stable id.
    pub hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub callsign: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub registration: Option<String>,
    /// ICAO type designator, for example `A320`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub type_code: Option<String>,
    /// ADS-B emitter category, for example `A3`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    pub position: GeoPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub altitude_baro_ft: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub altitude_geom_ft: Option<f64>,
    pub on_ground: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ground_speed_kt: Option<f64>,
    /// True track over ground, degrees clockwise from north.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub track_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vertical_rate_fpm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub squawk: Option<String>,
    /// Emergency status when not `none`, for example `general` or `lordo`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub emergency: Option<String>,
    pub is_military: bool,
    /// Seconds since the position was last updated at the source.
    pub seen_pos_s: f64,
    /// The source id that reported this contact.
    pub source: String,
}
