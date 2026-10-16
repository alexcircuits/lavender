//! Provider adapters: each turns one upstream source into validated domain items.
//! Providers fetch and normalise; the feed hub schedules, shares and filters.
//!
//! Upstream documents are parsed row by row: a malformed row is dropped and counted, never
//! the whole feed (security spec §2), and every error message that leaves a provider is
//! cleaned and length-capped, because parser errors can quote upstream content.

pub mod adsb_lol;
pub mod aisstream;
pub mod celestrak;
pub mod cyclones;
pub mod firms;
pub mod fixture;
pub mod launches;
pub mod perimeters;
pub mod usgs;
pub mod wind;

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::text::clean;
use lavender_domain::{FeedId, FeedItems, ProviderScope, VesselTrackResponse};
use lavender_geo::Cell;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use tokio_util::sync::CancellationToken;

use crate::fetch::{FetchError, Fetcher};
use crate::secrets::SecretStore;
use crate::vessel_store::Mmsi;

/// Longest provider error message, in characters, that reaches health, logs and frames.
pub const MAX_ERROR_CHARS: usize = 200;

/// What part of the world a fetch covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Global,
    Cell(Cell),
}

/// Everything a provider may use while fetching.
#[derive(Debug)]
pub struct FetchContext<'a> {
    pub fetcher: &'a Fetcher,
    pub secrets: &'a dyn SecretStore,
    pub cancel: CancellationToken,
    pub cache_dir: Option<&'a Path>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("{source_name} sent data in an unexpected shape: {reason}")]
    Parse { source_name: &'static str, reason: String },
    #[error("{0}")]
    Unconfigured(String),
    #[error("{0}")]
    Unsupported(String),
    /// The upstream answered nothing usable (for example every CelesTrak group failed).
    #[error("{0}")]
    Unavailable(String),
}

impl ProviderError {
    /// A parse failure whose reason is cleaned and capped (serde errors quote the input).
    #[must_use]
    pub fn parse(source_name: &'static str, reason: &impl std::fmt::Display) -> Self {
        Self::Parse { source_name, reason: clean(&reason.to_string(), MAX_ERROR_CHARS) }
    }

    /// The message shown to operators and analysts: control characters removed, at most
    /// [`MAX_ERROR_CHARS`] characters, whatever the variant carries.
    #[must_use]
    pub fn public_message(&self) -> String {
        clean(&self.to_string(), MAX_ERROR_CHARS)
    }
}

/// A provider's answer: the items, how current they are, and what went partly wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub items: FeedItems,
    /// When the oldest part of `items` was obtained upstream. `None` means "just now";
    /// a provider that serves a cache reports the cache's age here.
    pub as_of: Option<Timestamp>,
    /// Set when the answer is incomplete or older than it should be because part of the
    /// upstream failed (for example one CelesTrak group). The hub reports it as
    /// `Degraded` freshness and in health.
    pub degraded: Option<String>,
    /// Informational sentences for the analyst, for example how many malformed records
    /// were skipped.
    pub notices: Vec<String>,
}

impl Fetched {
    /// A complete, current answer.
    #[must_use]
    pub fn fresh(items: FeedItems) -> Self {
        Self { items, as_of: None, degraded: None, notices: Vec::new() }
    }

    /// Adds the standard notice for skipped records (when any were skipped).
    #[must_use]
    pub fn with_dropped(mut self, source_name: &str, dropped: usize) -> Self {
        if dropped > 0 {
            let noun = if dropped == 1 { "record" } else { "records" };
            self.notices.push(format!("{source_name}: {dropped} malformed {noun} skipped."));
        }
        self
    }
}

impl From<FeedItems> for Fetched {
    fn from(items: FeedItems) -> Self {
        Self::fresh(items)
    }
}

/// Deserialises an optional field leniently: a value of the wrong type becomes `None`
/// instead of failing the whole record. Required fields are then checked by the
/// provider's normaliser, which drops the record when one is missing.
pub(crate) fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// Parses each element of a JSON array on its own. Returns the rows that parsed and how
/// many did not.
pub(crate) fn parse_rows<T: DeserializeOwned>(rows: Vec<serde_json::Value>) -> (Vec<T>, usize) {
    let total = rows.len();
    let parsed: Vec<T> = rows.into_iter().filter_map(|r| serde_json::from_value(r).ok()).collect();
    let dropped = total - parsed.len();
    (parsed, dropped)
}

/// One implemented data source. Providers fetch and normalize records; the hub owns
/// refresh scheduling, shared results, viewport filtering and subscription cleanup.
///
/// Use the supplied fetcher and cancellation token for every upstream request. Return
/// cached data with its original `as_of` time and a `degraded` reason after a failed
/// refresh, so callers can tell cached records from current data.
#[async_trait]
pub trait FeedProvider: Send + Sync + std::fmt::Debug {
    fn feed(&self) -> FeedId;
    fn source_id(&self) -> &'static str;
    fn cadence(&self) -> Duration;
    fn scope(&self) -> ProviderScope;
    fn enabled(&self) -> bool;

    /// The shortest wait before retrying after a failed or degraded fetch. The hub's
    /// exponential backoff never goes below it, so providers with strict usage policies
    /// (CelesTrak) are not hammered while they fail.
    /// Healthy refresh jitter also respects this floor as minimum source spacing.
    fn retry_floor(&self) -> Duration {
        Duration::ZERO
    }

    /// For regional providers: the notice shown when a viewport is too large for regional
    /// queries and the provider's global view (`Region::Global`) is used instead. `None`
    /// means large viewports are truncated to the cells nearest the centre.
    fn global_fallback_notice(&self) -> Option<&'static str> {
        None
    }

    /// For push-based providers: the shortest interval between the snapshots one
    /// subscription sends once its view is complete. Slot updates inside it are
    /// coalesced, so a subscriber gets updates at this rate however many cells it covers.
    fn snapshot_interval(&self) -> Option<Duration> {
        None
    }

    /// For vessels providers: the track retained for `mmsi`, read from the provider's
    /// own store with no network call. `None` when nothing is retained.
    fn vessel_track(&self, _mmsi: Mmsi) -> Option<VesselTrackResponse> {
        None
    }

    /// Stops background work the provider runs outside `fetch`, such as a persistent
    /// upstream connection. Called when the hub shuts down.
    fn shutdown(&self) {}

    /// Fetches and normalises the items for `region`.
    async fn fetch(&self, ctx: &FetchContext<'_>, region: Region)
    -> Result<Fetched, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_messages_are_cleaned_and_capped() {
        let huge = format!("bad\u{0}\n{}", "x".repeat(2_000_000));
        let err = ProviderError::Unsupported(huge);
        let msg = err.public_message();
        assert!(msg.chars().count() <= MAX_ERROR_CHARS, "{}", msg.len());
        assert!(!msg.contains('\u{0}') && !msg.contains('\n'));
        let parse = ProviderError::parse("X", &"y".repeat(10_000));
        assert!(parse.to_string().len() < MAX_ERROR_CHARS + 60);
    }

    #[test]
    fn dropped_notice_is_only_added_when_something_was_dropped() {
        let f = Fetched::fresh(FeedItems::empty(FeedId::Earthquakes));
        assert_eq!(f.clone().with_dropped("USGS", 0).notices, Vec::<String>::new());
        assert_eq!(f.with_dropped("USGS", 2).notices, vec!["USGS: 2 malformed records skipped."]);
    }
}
