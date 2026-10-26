//! Base map imagery catalog, availability diagnostics, and static credit attribution (`LB-119` — porting `legacy/src/maps/`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Base map stack provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum MapStackKind {
    Photoreal,
    Ion,
    EsriImagery,
    Osm,
}

/// Catalog entry describing a base map stack, its credential requirements, and live availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MapStackEntry {
    pub id: String,
    pub label: String,
    pub short_label: String,
    pub kind: MapStackKind,
    pub requires_ion: bool,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable_reason: Option<String>,
    pub credit_html: String,
}

/// Snapshot of all registered base map stacks and their current availability diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MapCatalogSnapshot {
    pub stacks: Vec<MapStackEntry>,
}
