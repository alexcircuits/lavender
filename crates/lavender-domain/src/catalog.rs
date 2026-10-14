//! The layers and imagery the application can offer, including availability and required credits.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{FeedId, LicenceMode, SourceInfo};

/// How a layer appears grouped in the layer explorer, and which validated map colour
/// family it uses (design system §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LayerFamily {
    Air,
    Maritime,
    Space,
    Hazards,
    Weather,
    Infrastructure,
    Mobility,
    Sensors,
}

/// Whether a feed fetches the whole world at once or per region.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ProviderScope {
    /// One upstream request covers the world; subscribers receive filtered views.
    Global,
    /// Upstream requests are made per grid cell of `cellDeg` degrees; a subscription may
    /// cover at most `maxCells` cells before coverage becomes partial.
    Regional { cell_deg: f64, max_cells: u32 },
}

/// Why a layer can or cannot be turned on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Availability {
    Available,
    /// A key or setting is missing; `missing` names it for the operator.
    Unconfigured {
        missing: Vec<String>,
    },
    /// Commercial licence mode disables every contributing source.
    DisabledByLicence,
}

/// A layer the interface can show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LayerDescriptor {
    pub feed: FeedId,
    pub name: String,
    pub description: String,
    pub family: LayerFamily,
    pub source_ids: Vec<String>,
    pub availability: Availability,
    /// Expected refresh interval; freshness is judged against it.
    pub cadence_s: u32,
    pub scope: ProviderScope,
    /// Whether the layer is on when a workspace starts fresh.
    pub default_on: bool,
}

/// How an imagery layer is addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageryKind {
    /// `{z}/{x}/{y}`-style URL template in Web Mercator.
    UrlTemplate,
    /// OGC WMTS in geographic (EPSG:4326) tiling, as served by NASA GIBS.
    WmtsGeographic,
    /// Cesium's bundled Natural Earth II (works offline).
    Bundled,
}

/// A base or overlay imagery source the globe can show. Tile requests go from the WebView
/// directly to `tileHosts` (ADR 0008, the only browser egress).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageryDescriptor {
    pub id: String,
    pub name: String,
    pub kind: ImageryKind,
    /// URL or template; empty for `bundled`.
    pub url: String,
    /// WMTS layer identifier when `kind` is `wmtsGeographic`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layer: Option<String>,
    /// WMTS tile matrix set, for example `500m`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tile_matrix_set: Option<String>,
    /// Image format, for example `image/jpeg`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub format: Option<String>,
    pub maximum_level: u32,
    pub source_id: String,
    pub tile_hosts: Vec<String>,
    /// Night-time imagery suits the dark interface but hides daytime surface detail.
    pub is_dark: bool,
    pub is_default: bool,
}

/// Everything the interface needs to know about what Lavender can show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Catalog {
    pub layers: Vec<LayerDescriptor>,
    pub sources: Vec<SourceInfo>,
    pub imagery: Vec<ImageryDescriptor>,
    pub licence_mode: LicenceMode,
}
