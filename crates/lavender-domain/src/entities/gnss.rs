//! GNSS navigation integrity and GPS interference detection contracts.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{BBox, GeoPoint};

/// Navigation integrity anomaly level for a geographic cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GnssLevel {
    /// Nominal navigation accuracy: fewer than 2% degraded aircraft.
    Low,
    /// Medium severity navigation integrity anomaly: 2% to 10% degraded aircraft.
    Medium,
    /// High severity navigation integrity anomaly: greater than 10% degraded aircraft.
    High,
}

/// A binned geographic cell reporting aircraft navigation accuracy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GnssCell {
    /// Cell identifier in "row:col" format (0.5 degree grid).
    pub id: String,
    /// Bounding box of the cell.
    pub bbox: BBox,
    /// Center coordinates of the cell.
    pub center: GeoPoint,
    /// Total distinct aircraft observed in this cell within the rolling window.
    pub aircraft: usize,
    /// Number of distinct aircraft reporting degraded navigation accuracy.
    pub degraded: usize,
    /// Percentage of degraded aircraft after discounting 1 false positive: 100 * max(0, degraded - 1) / aircraft.
    pub percent_degraded: f64,
    /// Anomaly severity level.
    pub level: GnssLevel,
    /// Primary cell state ("navigation-integrity-anomaly" or "nominal").
    pub state: String,
    /// Secondary unvalidated interpretation (Some("suspected-gnss-interference") when level != Low).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpretation: Option<String>,
}

/// Current GNSS integrity snapshot aggregated across all active cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GnssSnapshot {
    /// Timestamp when this snapshot was evaluated.
    pub as_of: Timestamp,
    /// Rolling window duration in minutes (typically 30).
    pub window_minutes: u32,
    /// Cell resolution in degrees (typically 0.5).
    pub cell_deg: f64,
    /// Minimum aircraft required for a cell to be reported (typically 3).
    pub min_aircraft: usize,
    /// Total active cells meeting the minimum aircraft threshold.
    pub total_cells: usize,
    /// Number of cells classified as anomalies (medium or high).
    pub anomaly_cells: usize,
    /// List of evaluated cells.
    pub cells: Vec<GnssCell>,
}

/// Machine-readable provenance and classifier specifications for GNSS integrity analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GnssProvenance {
    /// Identifier of the per-aircraft classifier rule.
    pub classifier_id: String,
    /// Logical rule for degraded aircraft classification.
    pub rule: String,
    /// Regulatory or empirical basis for the classification.
    pub basis: String,
    /// Aggregation formula used for cell percentage calculation.
    pub aggregation_formula: String,
    /// Percentage threshold for medium severity anomaly.
    pub medium_percent_threshold: f64,
    /// Percentage threshold for high severity anomaly.
    pub high_percent_threshold: f64,
    /// Rolling evaluation window in minutes.
    pub window_minutes: u32,
    /// Cell resolution in degrees.
    pub cell_deg: f64,
    /// Minimum aircraft required per cell.
    pub min_aircraft: usize,
}
