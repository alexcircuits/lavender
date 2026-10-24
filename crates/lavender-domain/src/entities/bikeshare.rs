//! Bike-share domain entities for GBFS systems and stations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::GeoPoint;

/// A configured bike-share system in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BikeshareSystem {
    /// Unique identifier for the system (e.g. `nyc-citibike`, `austin-capmetro`).
    pub id: String,
    /// Human-readable system or network name (e.g. "Citi Bike", "`CapMetro`").
    pub name: String,
    /// City and state/country served (e.g. "New York, NY", "Austin, TX").
    pub city: String,
    /// Center coordinates of the operating area.
    pub center: GeoPoint,
    /// Operating/loading radius around the center in kilometers.
    pub load_radius_km: f64,
    /// Operator / provider organization (e.g. "Lyft", "`BCycle`", "`CapMetro`").
    pub provider: String,
    /// Upstream GBFS discovery URL (`gbfs.json`).
    pub gbfs_url: String,
}

/// A normalized bike-share station combining static information and live status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BikeshareStation {
    /// Globally unique render key in `system_id:station_id` format.
    pub id: String,
    /// Station identifier within the system feed.
    pub station_id: String,
    /// System identifier from the registry (e.g. "nyc-citibike").
    pub system_id: String,
    /// Station display name.
    pub name: String,
    /// Geographic position of the station.
    pub position: GeoPoint,
    /// Total dock capacity if reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capacity: Option<u32>,
    /// Number of operational bikes currently available for rent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bikes_available: Option<u32>,
    /// Number of empty docks currently available for returns.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub docks_available: Option<u32>,
    /// Whether the station is physically deployed and installed on the ground.
    pub is_installed: bool,
    /// Whether the station is currently renting bikes to riders.
    pub is_renting: bool,
    /// Whether the station is currently accepting bike returns.
    pub is_returning: bool,
    /// Unix timestamp in seconds of the last status update from the operator.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_reported: Option<i64>,
}
