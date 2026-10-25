//! Canonical entity resolution contracts across federated data feeds.
//!
//! Provides types for resolved canonical entities, alias identifiers, and relation
//! links between entities or federated observations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Classification of a canonical entity in the operational ontology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EntityKind {
    /// An aircraft or airborne contact.
    Aircraft,
    /// A maritime vessel or surface vessel.
    Vessel,
    /// An artificial satellite or space object.
    Satellite,
    /// A physical facility, infrastructure site, or installation.
    Facility,
    /// An operational event, incident, or occurrence.
    Event,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aircraft => f.write_str("aircraft"),
            Self::Vessel => f.write_str("vessel"),
            Self::Satellite => f.write_str("satellite"),
            Self::Facility => f.write_str("facility"),
            Self::Event => f.write_str("event"),
        }
    }
}

/// A federated identifier with an explicit naming scheme and identifier value.
///
/// Schemes identify domain namespaces such as `"icao24"`, `"registration"`,
/// `"callsign"`, `"mmsi"`, `"imo"`, or `"norad_cat_id"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityIdentifier {
    /// Scheme or authority defining this identifier (e.g. `"icao24"`, `"mmsi"`).
    pub scheme: String,
    /// Unique identifier value within the scheme.
    pub value: String,
}

impl EntityIdentifier {
    /// Constructs a new entity identifier.
    #[must_use]
    pub fn new(scheme: impl Into<String>, value: impl Into<String>) -> Self {
        Self { scheme: scheme.into(), value: value.into() }
    }
}

/// A resolved canonical entity consolidating federated observations and aliases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CanonicalEntity {
    /// Unique canonical identifier (e.g. `"canonical:aircraft:4ca1b2"`).
    pub id: String,
    /// Operational kind of entity.
    pub kind: EntityKind,
    /// Human-readable primary display label (e.g. callsign, vessel name, or satellite name).
    pub primary_label: String,
    /// Federated identifiers associated with this entity.
    #[serde(default)]
    pub identifiers: Vec<EntityIdentifier>,
    /// Domain-specific attributes and telemetry key-value pairs.
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    /// Unix timestamp (seconds) when this entity was first observed.
    pub first_seen: i64,
    /// Unix timestamp (seconds) when this entity was most recently updated.
    pub last_seen: i64,
    /// Confidence score of entity resolution in the range 0.0..=1.0.
    pub confidence: f32,
}

/// A directed or associative relation between entities or federated records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityLink {
    /// Identifier of the source entity or subject.
    pub source_id: String,
    /// Identifier of the target entity, object, or federated record.
    pub target_id: String,
    /// Nature of the relationship (e.g. `"alias_of"`, `"co_located"`, `"flag_state"`).
    pub relation: String,
    /// Confidence score of the link in the range 0.0..=1.0.
    pub confidence: f32,
    /// Unix timestamp (seconds) when the link was created.
    pub created_at: i64,
}
