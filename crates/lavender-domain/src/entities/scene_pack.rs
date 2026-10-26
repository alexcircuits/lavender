//! Director-led scene tours and geospatial evidence packs (`LB-118` — porting `legacy/src/scenes/` & `legacy/src/director/`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Authored 6-DOF camera pose in WGS84 degrees and ellipsoidal meters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneCameraPose {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub heading_deg: f64,
    pub pitch_deg: f64,
    pub roll_deg: f64,
}

/// A single authored beat/shot in a director-led scene tour or evidence pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneShot {
    /// Stable shot beat identifier (e.g. `"debris-dammed-lake"`, `"dhunche"`).
    pub id: String,
    /// Display title for the shot beat.
    pub title: String,
    /// Target camera pose at the end of the transition.
    pub pose: SceneCameraPose,
    /// Flight transition duration in seconds (`4.2s` default).
    pub duration_s: f64,
    /// Hold duration at the target pose in seconds before advancing.
    pub hold_s: f64,
    /// Preferred map stack identifier (`"photoreal"`, `"esri-imagery"`, `"osm"`).
    pub map_stack: String,
    /// Optional analyst briefing note or caption for this beat.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub narration: Option<String>,
    /// Optional reach schematic animation duration in seconds (`PATH_DURATION_BY_BEAT`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reach_duration_s: Option<f64>,
    /// Optional locator presentation overlay id (`"bhote-koshi-incident-places"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub locator_presentation: Option<String>,
}

/// A curated multi-beat geospatial evidence tour (`LB-118`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EvidencePack {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub region: String,
    pub required_layers: Vec<String>,
    pub shots: Vec<SceneShot>,
}
