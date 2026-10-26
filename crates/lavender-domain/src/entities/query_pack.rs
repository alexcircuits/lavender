//! Analyst query pack contracts (`LB-121`): multi-domain OSINT query templates,
//! breakdown statistics, `DuckDB` Spatial SQL plans, and structured row results.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, GeoPoint};

/// Operational domain of an analyst query pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum QueryPackDomain {
    Aviation,
    Maritime,
    Space,
    Hazards,
    Surveillance,
    Infrastructure,
    Mobility,
    Environment,
    Briefing,
}

/// Metadata describing one registered analyst query pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackDefinition {
    /// Stable identifier (e.g., `"aircraft_in_area"`, `"maritime_dark_targets"`).
    pub id: String,
    /// Human-readable title shown in the query pack catalog.
    pub title: String,
    /// Operational category.
    pub domain: QueryPackDomain,
    /// Concise analyst description of what the query computes and summarizes.
    pub description: String,
    /// Default row cap when executed without an explicit `limit`.
    pub default_limit: usize,
}

/// Request payload for executing an analyst query pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackRequest {
    /// Optional geographic bounding box filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bbox: Option<BBox>,
    /// Maximum number of matched rows to return (1..=500).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<usize>,
    /// Optional domain-specific filter token (e.g. airline code, vessel type, category).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filter: Option<String>,
}

/// A single category count in a query result breakdown (e.g., by airline or vessel type).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackBreakdownBucket {
    /// Group key (e.g. `"UAL"`, `"CARGO"`, `"HIGH"`).
    pub key: String,
    /// Number of matched entities in this bucket across all matches.
    pub count: usize,
}

/// Numeric summary statistics (`min`, `median`, `max`) across matched entities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackNumericStats {
    /// Metric name and unit (e.g., `"altitude_m"`, `"speed_knots"`).
    pub metric: String,
    /// Sample count with finite values.
    pub count: usize,
    /// Minimum observed value.
    pub min: f64,
    /// Median (P50) observed value.
    pub median: f64,
    /// Maximum observed value.
    pub max: f64,
}

/// One matched entity or observation row returned by an analyst query pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackRow {
    /// Entity or observation identifier.
    pub id: String,
    /// Originating feed or layer identifier (`"aircraft"`, `"vessels"`, `"gnss"`, etc.).
    pub kind: String,
    /// Primary analyst label (callsign, ship name, station name, cell ID).
    pub label: String,
    /// Secondary subtitle or classification (airline/type, destination, operator).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<String>,
    /// Geographic position when localized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub position: Option<GeoPoint>,
    /// Distance in kilometers from the query area center when a `bbox` was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distance_km: Option<f64>,
    /// Primary numeric metric value (e.g. altitude in m, speed in kt, magnitude, anomaly %).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub primary_metric: Option<f64>,
    /// Unit for `primary_metric` (e.g. `"m"`, `"kt"`, `"M"`, `"%"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub primary_unit: Option<String>,
}

/// Complete response from executing an analyst query pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryPackExecution {
    /// Identifier of the executed query pack.
    pub pack_id: String,
    /// Human-readable title of the executed query pack.
    pub title: String,
    /// Operational domain.
    pub domain: QueryPackDomain,
    /// Timestamp when the query was evaluated.
    pub executed_at: Timestamp,
    /// Bounding box applied, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bbox: Option<BBox>,
    /// Total matched records before row capping.
    pub total_matches: usize,
    /// Whether the returned `rows` were truncated by `limit`.
    pub truncated: bool,
    /// Concise natural-language analyst summary sentence.
    pub summary: String,
    /// Parameterized `DuckDB` Spatial SQL statement corresponding to this query pack.
    pub duckdb_sql: String,
    /// Category breakdown across all matches.
    pub breakdown: Vec<QueryPackBreakdownBucket>,
    /// Optional numeric summary statistics across all matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stats: Option<QueryPackNumericStats>,
    /// Capped result rows ordered by relevance or proximity to area center.
    pub rows: Vec<QueryPackRow>,
}
