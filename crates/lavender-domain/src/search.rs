use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, FeedId, GeoPoint};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchQuery {
    pub q: String,
    /// Bias place results towards this point (usually the camera centre).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub near: Option<GeoPoint>,
    /// Maximum results per group (default 8, at most 25).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// Coordinates recognised in the query without any network request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CoordinateHit {
    pub position: GeoPoint,
    /// How the input was read: `decimal`, `dms` or `mgrs`.
    pub format: String,
    /// The position formatted for display, for example `60.1699° N, 24.9384° E`.
    pub label: String,
}

/// A geocoded place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlaceHit {
    pub name: String,
    /// Region and country context, for example `Uusimaa, Finland`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context: Option<String>,
    pub position: GeoPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bbox: Option<BBox>,
    /// The place type, for example `city` or `airport`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
    pub source_id: String,
}

/// A live entity whose identifier or name matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityHit {
    pub feed: FeedId,
    pub id: String,
    pub label: String,
    /// Secondary text, for example the aircraft type and altitude.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub position: Option<GeoPoint>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResults {
    pub coordinates: Vec<CoordinateHit>,
    pub places: Vec<PlaceHit>,
    pub entities: Vec<EntityHit>,
    /// Sentences for the analyst, for example that place search is not configured.
    pub notices: Vec<String>,
}
