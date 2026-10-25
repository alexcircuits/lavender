//! Saved search contracts: persistent search filters, feed associations, and match events.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::FeedId;

/// A saved persistent search query that tracks live entity matches across feeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSearch {
    /// Unique identifier for the saved search.
    pub id: String,
    /// Human-readable title or label for the saved search.
    pub name: String,
    /// Search query string matched against entity labels, callsigns, identifiers, and types.
    pub query: String,
    /// Target feeds to filter. If `None` or empty, all feeds are evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub feed_filter: Option<Vec<FeedId>>,
    /// Timestamp when this saved search was created.
    pub created_at: Timestamp,
    /// Whether new matches should emit an alert event into the alert inbox.
    #[serde(default)]
    pub notify_on_match: bool,
}

/// A match event recorded when a live entity matches a saved search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSearchMatch {
    /// Identifier of the matching saved search.
    pub search_id: String,
    /// Name of the matching saved search for display.
    pub search_name: String,
    /// Originating feed of the matched entity.
    pub feed: FeedId,
    /// Feed-specific entity identifier (e.g. ICAO hex, MMSI, NORAD ID).
    pub entity_id: String,
    /// Human-readable label of the matched entity (e.g. callsign, vessel name).
    pub label: String,
    /// Timestamp when the entity match occurred.
    pub matched_at: Timestamp,
}
