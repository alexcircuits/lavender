//! Administrative and territorial boundary features (international land borders,
//! disputed boundary lines, breakaway/contested areas, and maritime indicative lines).
//!
//! Sourced from Natural Earth public domain admin-0 datasets.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, GeoPoint};

/// Classification of administrative boundary feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BoundaryClass {
    /// International land border between sovereign nations.
    InternationalLand,
    /// Disputed or armistice line of control.
    DisputedLine,
    /// Breakaway or contested geographic area/region.
    BreakawayArea,
    /// Maritime indicative line (e.g. median lines, treaty lines).
    MaritimeIndicative,
}

/// A geographic boundary feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoundaryFeature {
    /// Unique identifier for the boundary feature.
    pub id: String,
    /// Descriptive name (e.g. sovereign name, disputed region name, or segment description).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Boundary classification.
    pub class: BoundaryClass,
    /// Disputed or administrative status (e.g. "Disputed", "Breakaway", "Line of control", "De facto").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
    /// Recognized sovereign country if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sovereign: Option<String>,
    /// Claimants or parties involved in dispute or treaty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claimants: Vec<String>,
    /// Notes or historical context provided by the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Polyline coordinate paths (each line is a list of [lon, lat] `GeoPoints`).
    /// For lines, each line is an open polyline. For areas (`BreakawayArea`), each line is a ring.
    pub lines: Vec<Vec<GeoPoint>>,
    /// Bounding box enclosing all coordinates of this boundary feature.
    pub bbox: BBox,
}

/// Response returned by the boundaries query API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoundaryResponse {
    /// Boundary features matching query filters.
    pub features: Vec<BoundaryFeature>,
    /// Total count of matching features.
    pub total: usize,
    /// Source attribution string.
    pub attribution: String,
}
