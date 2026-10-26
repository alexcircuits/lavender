//! Multi-provider street-level coverage contracts (`LB-116` — `Mapillary`, `KartaView`, `Panoramax`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Supported street-level imagery and coverage providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum StreetLevelProviderId {
    Mapillary,
    Kartaview,
    Panoramax,
}

/// A street-level capture sequence decoded from a coverage tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreetLevelSequence {
    /// Provider-scoped sequence identifier (`"z/x/y/idx"` for `KartaView` or UUID for `Panoramax`/`Mapillary`).
    pub id: String,
    /// Upstream street-level provider.
    pub provider: StreetLevelProviderId,
    /// Connected `[lon, lat]` polyline parts.
    pub parts: Vec<Vec<[f64; 2]>>,
    /// Sanitized capture epoch milliseconds (`0` when undated or rejected as implausible).
    pub captured_at_ms: i64,
    /// `Some(true)` for 360° equirectangular panoramas, `Some(false)` for flat perspective frames, `None` when unspecified.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_pano: Option<bool>,
}

/// A decoded street-level coverage tile containing chained or simplified sequences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StreetLevelCoverageTile {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub provider: StreetLevelProviderId,
    pub sequences: Vec<StreetLevelSequence>,
}
