//! Global wind forecast contracts and metadata types (ADR 0007).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Forecast model provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum WindModel {
    Gfs,
    Ifs,
}

impl WindModel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gfs => "gfs",
            Self::Ifs => "ifs",
        }
    }
}

/// Optional same-cycle weather overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum WeatherOverlay {
    #[default]
    None,
    Temperature,
    Pressure,
}

impl WeatherOverlay {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Temperature => "temperature",
            Self::Pressure => "pressure",
        }
    }
}

/// Model cycle reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindCycle {
    // Date formatted as YYYYMMDD.
    pub date: String,
    // Cycle hour (0, 6, 12, 18).
    pub hour: u32,
    // Forecast step hour.
    #[serde(default)]
    pub forecast_hour: u32,
    // ISO 8601 timestamp of model run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_iso: Option<String>,
    // ISO 8601 timestamp of forecast validity.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub valid_iso: Option<String>,
}

/// Resampled equirectangular grid geometry.
///
/// The binary grid (`/api/v1/wind/grid/{id}.bin`, desktop `wind_grid`) is IEEE-754 binary32
/// **little-endian**, no header: the U block, the V block, then the scalar block when the
/// manifest has `scalar`. Each block is `ny` rows of `nx` values, row-major; row 0 is
/// latitude `la1` (90° N) stepping south by `dy`, column 0 is longitude `lo1` (0° E)
/// stepping east by `dx`. U/V are m/s; the scalar is °C or hPa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindGridDef {
    pub nx: usize,
    pub ny: usize,
    pub lo1: f64,
    pub la1: f64,
    pub dx: f64,
    pub dy: f64,
}

/// Metadata for companion weather scalar field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WeatherScalarMetadata {
    pub kind: WeatherOverlay,
    pub units: String,
    pub level: String,
}

/// Manifest returned by `/api/v1/wind/manifest` (or `/api/v1/wind/status` without `gridUrl`
/// and `gridId`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WindManifest {
    pub schema_version: u32,
    pub model: WindModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cycle: Option<WindCycle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub overlay: Option<WeatherOverlay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scalar: Option<WeatherScalarMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scalar_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub fetched_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub units: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grid: Option<WindGridDef>,
    pub stale: bool,
    pub unavailable: bool,
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grid_url: Option<String>,
    /// Identifier of the binary grid for this manifest, accepted by every platform's grid
    /// call (the HTTP `gridUrl` embeds the same identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grid_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attribution: Option<String>,
}

pub type WindStatus = WindManifest;
