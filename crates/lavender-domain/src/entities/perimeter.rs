//! Wildland fire perimeters (NIFC WFIGS) with the facts an incident card shows, each
//! outline simplified for one range of map scales.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, GeoPoint};

/// How far an outline was simplified. A snapshot carries one level, chosen for its view:
/// wide views get coarse outlines, close views the outline as published.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PerimeterDetail {
    /// 0 is the outline as published; each higher level is coarser.
    pub level: u8,
    /// How far, in metres, simplification may move the outline's kept rings from the
    /// published ones (0 for the outline as published). Parts and holes smaller than
    /// this in both directions may be left out entirely; the fire's largest part always
    /// stays.
    pub tolerance_m: f64,
}

/// One fire's perimeter: the area burnt or burning, as mapped by the incident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FirePerimeter {
    /// The WFIGS unique fire identifier, for example `2026-CALPF-002271`.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Reported incident size in acres.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub acres: Option<f64>,
    /// Reported containment, 0–100.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contained_pct: Option<f64>,
    /// State of the point of origin, for example `US-OR`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    /// Incident type category: `WF` (wildfire) or `RX` (prescribed fire).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub discovered_at: Option<Timestamp>,
    /// When the perimeter itself was last mapped.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cause: Option<String>,
    /// General fire behaviour, for example `Minimal` or `Active`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub behavior: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub personnel: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub county: Option<String>,
    /// Estimated cost to date in US dollars.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cost_to_date_usd: Option<f64>,
    /// Incident complexity, for example `Type 3 Incident`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub complexity: Option<String>,
    /// The complex (fires managed together) this fire belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub complex_name: Option<String>,
    /// Where the incident card is anchored: the mean corner of the largest polygon's
    /// exterior ring, computed from the full-detail outline.
    pub anchor: GeoPoint,
    /// The extent of the full-detail outline.
    pub extent: BBox,
    /// The incident's InciWeb page, present only when it was matched and verified as
    /// describing this fire.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inciweb_url: Option<String>,
    pub detail: PerimeterDetail,
    /// GeoJSON `MultiPolygon` coordinates: polygons of rings of `[longitude, latitude]`.
    /// Each polygon's first ring is its exterior (counter-clockwise); later rings are holes.
    pub polygons: Vec<Vec<Vec<[f64; 2]>>>,
}
