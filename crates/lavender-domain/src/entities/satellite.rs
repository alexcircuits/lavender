//! Orbital elements shared by the satellite provider, propagation code and frontend.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Mean orbital elements (CCSDS OMM, SGP4 theory). Positions are propagated from these by
/// the client per frame, and by the core for tracks and passes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SatelliteElements {
    pub norad_id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub intl_designator: Option<String>,
    /// Catalogue group the element set was fetched from, for example `stations`.
    pub group: String,
    pub epoch: Timestamp,
    /// Revolutions per day.
    pub mean_motion: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_of_pericenter_deg: f64,
    pub mean_anomaly_deg: f64,
    pub bstar: f64,
    pub mean_motion_dot: f64,
    pub mean_motion_ddot: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rev_at_epoch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub classification: Option<String>,
}
