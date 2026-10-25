//! Air quality domain entities (ECCC AQHI and multi-network air quality readings).

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Freshness;

/// Severity band for air quality indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum AirQualityBand {
    /// Low health risk (AQHI 1-3).
    Low,
    /// Moderate health risk (AQHI 4-6).
    Moderate,
    /// High health risk (AQHI 7-10).
    High,
    /// Very high health risk (AQHI > 10).
    #[serde(rename = "very-high", alias = "very_high")]
    VeryHigh,
}

impl AirQualityBand {
    /// Categorise a published integer AQHI value into its severity band.
    #[must_use]
    pub const fn from_aqhi(value: u32) -> Self {
        match value {
            0..=3 => Self::Low,
            4..=6 => Self::Moderate,
            7..=10 => Self::High,
            _ => Self::VeryHigh,
        }
    }

    /// Accent colour for the band.
    #[must_use]
    pub const fn color(&self) -> &'static str {
        match self {
            Self::Low => "#4cc9f0",
            Self::Moderate => "#ffd43b",
            Self::High => "#ff6b35",
            Self::VeryHigh => "#c1121f",
        }
    }

    /// Plain-language risk text for the band.
    #[must_use]
    pub const fn risk(&self) -> &'static str {
        match self {
            Self::Low => "Low health risk",
            Self::Moderate => "Moderate health risk",
            Self::High => "High health risk",
            Self::VeryHigh => "Very High health risk",
        }
    }

    /// Display label for the band.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Moderate => "Moderate",
            Self::High => "High",
            Self::VeryHigh => "Very High",
        }
    }
}

/// A normalized air quality station reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AirQualityReading {
    /// Unique identifier for the reading (e.g. `eccc:{station_id}`).
    pub id: String,
    /// Data provider (e.g. "eccc").
    pub provider: String,
    /// Air quality scale (e.g. "AQHI").
    pub scale: String,
    /// Published integer index value (floored to 1).
    pub value: u32,
    /// Health risk band.
    pub band: AirQualityBand,
    /// Station identifier within the provider feed.
    pub station_id: String,
    /// Human-readable station name.
    pub name: String,
    /// Administrative or forecast zone (e.g. "atl", "que", "ont", "pnr", "pyr").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub zone: Option<String>,
    /// Station latitude in degrees.
    pub latitude: f64,
    /// Station longitude in degrees.
    pub longitude: f64,
    /// Time when the observation was taken.
    pub observed_at: Timestamp,
    /// Color hex code corresponding to the risk band.
    pub color: String,
    /// Value label (1..10 or "10+").
    pub label: String,
    /// Health risk summary text.
    pub risk: String,
    /// Optional special advisory or measurement note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

/// Current snapshot of air quality station observations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AirQualitySnapshot {
    /// List of current station readings.
    pub readings: Vec<AirQualityReading>,
    /// Evaluated feed freshness.
    pub freshness: Freshness,
    /// Timestamp when this snapshot was assembled or refreshed.
    pub updated_at: Timestamp,
}
