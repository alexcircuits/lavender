//! Feed identifiers, queries, results and freshness states shared by both transports.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::entities::{Aircraft, Cyclone, Earthquake, SatelliteElements};
use crate::{BBox, DomainError};

/// A live data feed, which the interface shows as a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum FeedId {
    Aircraft,
    Earthquakes,
    Satellites,
    Cyclones,
}

impl FeedId {
    /// Every feed, in catalog order.
    pub const ALL: [Self; 4] =
        [Self::Aircraft, Self::Earthquakes, Self::Satellites, Self::Cyclones];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aircraft => "aircraft",
            Self::Earthquakes => "earthquakes",
            Self::Satellites => "satellites",
            Self::Cyclones => "cyclones",
        }
    }
}

impl std::fmt::Display for FeedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for FeedId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|f| f.as_str() == s).ok_or_else(|| DomainError::Parse {
            what: "feed",
            reason: format!("unknown feed '{s}'"),
        })
    }
}

/// Optional filters. Each feed applies the filters that make sense for it and ignores
/// the rest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct FeedFilters {
    /// Earthquakes: smallest magnitude to include.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_magnitude: Option<f64>,
    /// Aircraft: only military contacts.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub military_only: Option<bool>,
    /// Satellites: catalogue groups to include, for example `stations` or `gps-ops`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub groups: Option<Vec<String>>,
}

/// What a subscriber wants from a feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FeedQuery {
    pub feed: FeedId,
    /// The viewport. `None` means the whole world.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bbox: Option<BBox>,
    #[serde(default)]
    pub filters: FeedFilters,
}

impl FeedQuery {
    #[must_use]
    pub fn new(feed: FeedId) -> Self {
        Self { feed, bbox: None, filters: FeedFilters::default() }
    }

    #[must_use]
    pub fn with_bbox(mut self, bbox: BBox) -> Self {
        self.bbox = Some(bbox);
        self
    }
}

/// How current a snapshot is. The interface never shows stale data as live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Freshness {
    /// Fetched within the feed's expected cadence.
    Live,
    /// The last successful fetch is older than twice the cadence.
    Stale { age_s: u32 },
    /// Data is shown from the last success, but the latest attempt failed.
    Degraded { reason: String, age_s: u32 },
    /// No data is available.
    Unavailable { reason: String },
}

/// How much of the requested area the snapshot covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Coverage {
    /// Everything the source knows inside the requested area.
    Complete,
    /// Only part of the area is covered; `reason` says why and what to do.
    Partial { reason: String },
}

/// The items of a snapshot, tagged by kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "items", rename_all = "camelCase")]
#[ts(export)]
pub enum FeedItems {
    Aircraft(Vec<Aircraft>),
    Earthquakes(Vec<Earthquake>),
    Satellites(Vec<SatelliteElements>),
    Cyclones(Vec<Cyclone>),
}

impl FeedItems {
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Aircraft(v) => v.len(),
            Self::Earthquakes(v) => v.len(),
            Self::Satellites(v) => v.len(),
            Self::Cyclones(v) => v.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The empty item list for a feed.
    #[must_use]
    pub fn empty(feed: FeedId) -> Self {
        match feed {
            FeedId::Aircraft => Self::Aircraft(Vec::new()),
            FeedId::Earthquakes => Self::Earthquakes(Vec::new()),
            FeedId::Satellites => Self::Satellites(Vec::new()),
            FeedId::Cyclones => Self::Cyclones(Vec::new()),
        }
    }
}

/// One consistent answer for a feed query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FeedSnapshot {
    pub feed: FeedId,
    /// Source ids that contributed, in registry order.
    pub sources: Vec<String>,
    /// When the data was last fetched successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fetched_at: Option<Timestamp>,
    /// When this snapshot was assembled.
    pub generated_at: Timestamp,
    pub freshness: Freshness,
    pub coverage: Coverage,
    /// Number of items before spatial and attribute filtering.
    pub source_count: u32,
    /// Regions of this query that have not reported yet (regional feeds load cell by cell).
    pub pending_regions: u32,
    pub items: FeedItems,
    /// Short sentences for the analyst, for example why coverage is partial.
    pub notices: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_ids_round_trip() {
        for feed in FeedId::ALL {
            assert_eq!(feed.as_str().parse::<FeedId>().unwrap(), feed);
            assert_eq!(serde_json::to_string(&feed).unwrap(), format!("\"{}\"", feed.as_str()));
        }
        assert!("ships".parse::<FeedId>().is_err());
    }

    #[test]
    fn freshness_is_internally_tagged() {
        let s = serde_json::to_string(&Freshness::Stale { age_s: 90 }).unwrap();
        assert_eq!(s, r#"{"state":"stale","ageS":90}"#);
        let s = serde_json::to_string(&Freshness::Live).unwrap();
        assert_eq!(s, r#"{"state":"live"}"#);
    }

    #[test]
    fn items_are_adjacently_tagged() {
        let s = serde_json::to_string(&FeedItems::Earthquakes(vec![])).unwrap();
        assert_eq!(s, r#"{"kind":"earthquakes","items":[]}"#);
    }

    #[test]
    fn query_deserialises_with_defaults_and_validates_bbox() {
        let q: FeedQuery = serde_json::from_str(r#"{"feed":"earthquakes"}"#).unwrap();
        assert_eq!(q, FeedQuery::new(FeedId::Earthquakes));
        let bad = serde_json::from_str::<FeedQuery>(
            r#"{"feed":"aircraft","bbox":{"west":0,"south":95,"east":1,"north":96}}"#,
        );
        assert!(bad.is_err());
    }
}
