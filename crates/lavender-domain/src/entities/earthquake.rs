//! Validated earthquake records shared by providers, transports and the frontend.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// A seismic event. `position.altM` is not used; depth is `depthKm` below the surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Earthquake {
    /// The provider's event id, for example `us7000abcd`.
    pub id: String,
    pub magnitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub magnitude_type: Option<String>,
    pub depth_km: f64,
    pub position: GeoPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub place: Option<String>,
    /// Origin time.
    pub time: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated: Option<Timestamp>,
    /// Link to the provider's event page.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    pub tsunami: bool,
    /// PAGER alert level (`green`, `yellow`, `orange`, `red`) when issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alert: Option<String>,
    /// Number of "did you feel it?" reports.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub felt: Option<u32>,
    /// USGS significance (0–1000+).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub significance: Option<u32>,
    /// `reviewed` or `automatic`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
}
