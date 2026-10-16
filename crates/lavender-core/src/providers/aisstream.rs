//! Live vessels from AISStream (aisstream.io), a WebSocket relay of public AIS broadcasts.
//!
//! AISStream pushes reports instead of answering polls, so this provider keeps a
//! persistent connection ([`link`], supervised by the [`watchdog`] policy) that writes into
//! a bounded [`VesselStore`]. The feed hub is unchanged in spirit: every map cell is a
//! regional slot whose `fetch` is an R-tree query of the store at the update interval (no
//! upstream request per fetch), and `snapshot_interval` throttles each subscription to
//! one snapshot per interval however many cells it covers.
//!
//! AISStream publishes no terms of use (ENG-938): the source needs an operator's key and
//! makes no connection without one.

mod link;
mod parsing;
mod watchdog;

pub use link::{LinkState, LinkStatus};
pub use parsing::{Frame, parse_frame};
pub use watchdog::Failure;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::{BBox, FeedId, FeedItems, ProviderScope, VesselTrackResponse};

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::AisStreamConfig;
use crate::fetch::{Fetcher, HostPolicy};
use crate::secrets::SecretKey;
use crate::sources;
use crate::vessel_store::{Mmsi, VesselLimits, VesselStore};
use link::{Link, LinkSettings};

/// Vessels sent for a view too large for area queries.
pub const WORLD_VIEW_LIMIT: usize = 10_000;
const WORLD_VIEW_NOTICE: &str =
    "Showing the most recently heard vessels worldwide. Zoom in to see every vessel in view.";
const CONNECTING: &str = "Connecting to AISStream; vessels appear as their reports arrive.";
const AREA_NOTICE: &str =
    "Vessels are received only for the AISStream areas this server subscribes to.";

#[derive(Debug)]
pub struct AisStreamProvider {
    config: AisStreamConfig,
    store: Arc<VesselStore>,
    link: Link,
}

impl AisStreamProvider {
    /// A provider whose connection starts on the first fetch with a configured key.
    #[must_use]
    pub fn new(config: AisStreamConfig, fetcher: Arc<Fetcher>) -> Self {
        let store = Arc::new(VesselStore::new(store_limits(&config)));
        let settings = LinkSettings {
            url: config.url.clone(),
            policy: HostPolicy::for_url(&config.url),
            boxes: subscription_boxes(&config.bounding_boxes),
            read_timeout: Duration::from_secs(config.read_timeout_s),
            steady_reports: covers_world(&config),
            idle_disconnect: (config.idle_disconnect_s > 0)
                .then(|| Duration::from_secs(config.idle_disconnect_s)),
            reconnect_min: Duration::from_millis(config.reconnect_min_ms),
            reconnect_max: Duration::from_secs(config.reconnect_max_s),
        };
        let link = Link::new(settings, fetcher, store.clone());
        Self { config, store, link }
    }

    #[must_use]
    pub fn store(&self) -> &Arc<VesselStore> {
        &self.store
    }

    /// What the connection is doing (for diagnostics and tests).
    #[must_use]
    pub fn link_status(&self) -> LinkStatus {
        self.link.status()
    }
}

/// Whether the subscription includes the whole world.
fn covers_world(config: &AisStreamConfig) -> bool {
    config.bounding_boxes.contains(&BBox::WORLD)
}

impl Drop for AisStreamProvider {
    fn drop(&mut self) {
        self.link.stop();
    }
}

/// What a replay of recorded frames did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayCounts {
    pub stored: usize,
    /// Reports the store refused or ignored (too old, or not newer than what it holds).
    pub not_stored: usize,
    pub unavailable_positions: usize,
    pub ignored: usize,
    pub malformed: usize,
}

/// Replays recorded AISStream frames, one JSON message per line, through the real parser
/// into `store`. Each report's own receiver time drives the store clock, so a recording
/// keeps its recorded times (and ages out ships exactly as live data would) instead of
/// being presented as current.
pub fn replay(recording: &str, store: &VesselStore) -> ReplayCounts {
    let mut counts = ReplayCounts::default();
    for line in recording.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let recorded = match parse_frame(line.as_bytes()) {
            Frame::Position(report) => {
                let at = report.observed_at.unwrap_or(Timestamp::UNIX_EPOCH);
                store.record_position(report, at)
            }
            Frame::Details(report) => {
                let at = report.observed_at.unwrap_or(Timestamp::UNIX_EPOCH);
                store.record_details(report, at)
            }
            Frame::PositionUnavailable(_) => {
                counts.unavailable_positions += 1;
                continue;
            }
            Frame::Confirmation | Frame::Ignored => {
                counts.ignored += 1;
                continue;
            }
            Frame::Malformed | Frame::UpstreamError(_) => {
                counts.malformed += 1;
                continue;
            }
        };
        if recorded == crate::vessel_store::Recorded::Stored {
            counts.stored += 1;
        } else {
            counts.not_stored += 1;
        }
    }
    counts
}

/// The retained track of `mmsi` in `store`, as the track endpoint returns it.
#[must_use]
pub fn track_response(
    store: &VesselStore,
    mmsi: Mmsi,
    source_id: &str,
) -> Option<VesselTrackResponse> {
    Some(VesselTrackResponse {
        mmsi: mmsi.to_string(),
        samples: store.track(mmsi)?,
        source: source_id.to_owned(),
        retained_sec: store.limits().retention.as_secs(),
        voyage: store.voyage(mmsi),
    })
}

/// The notice for frames skipped in the current session, if any.
fn malformed_notice(status: &LinkStatus) -> Option<String> {
    match status.malformed_this_session {
        0 => None,
        1 => Some("AISStream: 1 malformed message skipped since connecting.".to_owned()),
        n => Some(format!("AISStream: {n} malformed messages skipped since connecting.")),
    }
}

/// The store bounds an AISStream configuration asks for.
#[must_use]
pub fn store_limits(config: &AisStreamConfig) -> VesselLimits {
    VesselLimits {
        max_vessels: config.max_vessels,
        max_track_points: config.max_track_points,
        max_total_track_points: config.max_total_track_points,
        retention: Duration::from_secs(config.retention_s),
    }
}

/// AISStream `BoundingBoxes` (`[[south, west], [north, east]]`); an area crossing the
/// antimeridian becomes two boxes.
fn subscription_boxes(areas: &[BBox]) -> Vec<[[f64; 2]; 2]> {
    areas
        .iter()
        .flat_map(|b| {
            b.lon_ranges().into_iter().map(|(west, east)| [[b.south, west], [b.north, east]])
        })
        .collect()
}

/// Why the provider has no current answer, phrased for health and freshness.
fn unavailable(status: &LinkStatus, now: Timestamp) -> Option<String> {
    match &status.state {
        LinkState::Streaming { .. } => None,
        LinkState::Waiting { failure, retry_at } => {
            let wait = u64::try_from(retry_at.duration_since(now).as_secs().max(0)).unwrap_or(0);
            Some(format!("{}; next attempt in {}.", failure.describe(), human(wait)))
        }
        LinkState::Idle | LinkState::Connecting { .. } => {
            status.last_failure.as_ref().map(|f| format!("{}; reconnecting.", f.describe()))
        }
        LinkState::Stopped => Some("The AISStream connection is shut down.".to_owned()),
    }
}

fn human(seconds: u64) -> String {
    match seconds {
        0..120 => format!("{seconds} s"),
        120..7_200 => format!("{} min", seconds.div_ceil(60)),
        _ => format!("{} h", seconds.div_ceil(3_600)),
    }
}

#[async_trait]
impl FeedProvider for AisStreamProvider {
    fn feed(&self) -> FeedId {
        FeedId::Vessels
    }
    fn source_id(&self) -> &'static str {
        sources::AISSTREAM
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.update_interval_s)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: self.config.cell_deg, max_cells: self.config.max_cells }
    }
    fn enabled(&self) -> bool {
        self.config.enabled
    }
    fn global_fallback_notice(&self) -> Option<&'static str> {
        Some(WORLD_VIEW_NOTICE)
    }
    fn snapshot_interval(&self) -> Option<Duration> {
        Some(self.cadence())
    }
    /// Reads the store at the current time. Its clock otherwise moves only with frames
    /// and fetches, so once nobody watches the map (and the link has gone idle) old
    /// samples would still look retained.
    fn vessel_track(&self, mmsi: Mmsi) -> Option<VesselTrackResponse> {
        self.store.advance(Timestamp::now());
        track_response(&self.store, mmsi, self.source_id())
    }
    fn shutdown(&self) {
        self.link.stop();
    }

    /// Reads the store; starts (or keeps alive) the connection as a side effect. Without
    /// a key nothing connects and the answer is "unconfigured".
    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        region: Region,
    ) -> Result<Fetched, ProviderError> {
        let Some(key) = ctx.secrets.get(SecretKey::AisStreamApiKey) else {
            self.link.forget_key();
            return Err(ProviderError::Unconfigured(format!(
                "Set {} to receive vessels from AISStream.",
                SecretKey::AisStreamApiKey.env_var()
            )));
        };
        self.link.demand(key);
        let now = Timestamp::now();
        self.store.advance(now);
        let status = self.link.status();
        if let Some(reason) = unavailable(&status, now) {
            return Err(ProviderError::Unavailable(reason));
        }
        let vessels = match region {
            Region::Global => self.store.most_recent(WORLD_VIEW_LIMIT),
            Region::Cell(cell) => self.store.in_area(&cell.bbox(), self.config.max_vessels),
        };
        let mut fetched = Fetched::fresh(FeedItems::Vessels(vessels));
        if !matches!(status.state, LinkState::Streaming { .. }) {
            fetched.degraded = Some(CONNECTING.to_owned());
        }
        if !covers_world(&self.config) {
            fetched.notices.push(AREA_NOTICE.to_owned());
        }
        fetched.notices.extend(malformed_notice(&status));
        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_become_aisstream_corner_pairs() {
        let boxes = [BBox::WORLD, BBox::new(170.0, -20.0, -170.0, -10.0).unwrap()];
        assert_eq!(
            subscription_boxes(&boxes),
            vec![
                [[-90.0, -180.0], [90.0, 180.0]],
                [[-20.0, 170.0], [-10.0, 180.0]],
                [[-20.0, -180.0], [-10.0, -170.0]],
            ]
        );
    }

    #[test]
    fn failures_read_as_sentences_with_the_next_attempt() {
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        let waiting = LinkStatus {
            state: LinkState::Waiting {
                failure: Failure::Rejected { status: Some(401) },
                retry_at: "2026-10-15T13:00:00Z".parse().unwrap(),
            },
            ..LinkStatus::default()
        };
        assert_eq!(
            unavailable(&waiting, now).unwrap(),
            "AISStream rejected the API key (HTTP 401); next attempt in 60 min."
        );
        let reconnecting = LinkStatus {
            state: LinkState::Connecting { attempt: 2 },
            last_failure: Some(Failure::Transport("AISStream closed the connection".to_owned())),
            ..LinkStatus::default()
        };
        assert_eq!(
            unavailable(&reconnecting, now).unwrap(),
            "AISStream connection failed: AISStream closed the connection; reconnecting."
        );
        assert_eq!(unavailable(&LinkStatus::default(), now), None, "first connection pending");
        assert_eq!(human(5), "5 s");
        assert_eq!(human(7_201), "3 h");
    }
}
