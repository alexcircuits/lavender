//! Geofence alerting contracts: geofence boundaries, rules, and alert events.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{FeedId, GeoPoint};

/// An alert event kind: entry into a geofence, exit from a geofence, or satellite overflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    /// Entity entered the geofence polygon.
    Enter,
    /// Entity exited the geofence polygon.
    Exit,
    /// Satellite predicted to pass over the geofence centroid.
    Overflight,
}

/// A geographic boundary polygon for geofence alerting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Geofence {
    /// Unique identifier for the geofence.
    pub id: String,
    /// Human-readable label for display.
    pub name: String,
    /// Ordered boundary vertices forming a closed or open ring.
    pub polygon: Vec<GeoPoint>,
}

const fn default_true() -> bool {
    true
}

/// A geofence alerting rule containing one or more boundary polygons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeofenceRule {
    /// Unique identifier for the alerting rule.
    pub id: String,
    /// Human-readable rule title.
    pub name: String,
    /// Geofences evaluated by this rule.
    #[serde(default)]
    pub geofences: Vec<Geofence>,
    /// Whether evaluation is currently active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A triggered geofence or overflight alert event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    /// Unique identifier for this alert event.
    pub id: String,
    /// The rule that triggered the event.
    #[serde(alias = "rule_id")]
    pub rule_id: String,
    /// The specific geofence within the rule, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geofence_id: Option<String>,
    /// Name of the geofence for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geofence_name: Option<String>,
    /// The originating feed of the entity.
    pub feed: FeedId,
    /// Feed-specific entity identifier (e.g. ICAO hex for aircraft, MMSI for vessels, NORAD ID for satellites).
    #[serde(alias = "entity_id")]
    pub entity_id: String,
    /// Alert kind: enter, exit, or overflight.
    pub kind: AlertKind,
    /// Timestamp when the transition or overflight occurred.
    pub at: Timestamp,
    /// Detection confidence score in range 0.0..=1.0.
    pub confidence: f64,
    /// Position of the entity when alerted, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPoint>,
    /// Human-readable summary or telemetry description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
