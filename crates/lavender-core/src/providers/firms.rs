//! NASA FIRMS active fire detections and map aggregation (public domain / CC0).
//! Upstream: `https://firms.modaps.eosdis.nasa.gov/api/area/csv/{MAP_KEY}/{SOURCE}/world/2`
//! (`world/2` is today and yesterday in UTC, the least that covers the trailing 24 hours).
//!
//! Quota: FIRMS allows 5,000 transactions per 10 minutes per `MAP_KEY`. A refresh downloads
//! each configured source (at most 10) once, and a refresh that finishes is not repeated
//! for a cadence (at least 10 minutes), whoever asks and however often. A refresh whose
//! caller gave up does not count as one: the next caller restarts it, at least a minute
//! after it began, and downloads only the sources it had not finished.
//!
//! Each source keeps the detections of its last good download, so a source that fails
//! leaves its earlier detections on the map (reported as degraded) for up to a day.

mod aggregate;
mod parsing;
#[cfg(test)]
mod tests;

pub use aggregate::{aggregate_fires, distinct_cells};
pub use parsing::{filter_trailing_24h, is_likely_csv, parse_acquisition_time, parse_csv};

use std::time::{Duration, Instant};

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::{FeedId, FeedItems, FireCell, ProviderScope};
use secrecy::ExposeSecret;
use tokio::sync::Mutex;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::FirmsConfig;
use crate::fetch::{FetchError, FetchOptions, HostPolicy, MAX_BODY_CEILING};
use crate::secrets::SecretKey;
use crate::sources;

/// Largest CSV response read per source. Two days of one VIIRS source measured about
/// 19 MB on 2026-10-15; the ceiling leaves room for peak fire seasons. Sources are
/// downloaded one after another, so one body is held at a time.
const MAX_BODY: usize = MAX_BODY_CEILING;
/// Shortest interval between finished refreshes, after success or failure.
const CADENCE_FLOOR: Duration = Duration::from_secs(600);
/// Shortest interval between the starts of two refreshes when the first did not
/// finish: its requests may already have counted against the key.
const RESTART_SPACING: Duration = Duration::from_secs(60);

/// The last good download of one source: its detections from the trailing day.
struct SourceData {
    cells: Vec<FireCell>,
    dropped: usize,
    /// When the refresh that downloaded it began, reported as the data's age.
    as_of: Timestamp,
    /// The same moment on the monotonic clock.
    started: Instant,
}

impl std::fmt::Debug for SourceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A source holds up to hundreds of thousands of detections; debug output of the
        // core (which reaches this state) shows how many, not each one.
        f.debug_struct("SourceData")
            .field("cells", &self.cells.len())
            .field("dropped", &self.dropped)
            .field("as_of", &self.as_of)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct State {
    /// One entry per configured source, in configuration order.
    sources: Vec<Option<SourceData>>,
    /// Why the last finished refresh was incomplete.
    degraded: Option<String>,
    /// When the last refresh began, finished or not.
    last_start: Option<Instant>,
    /// When the last refresh that finished began.
    last_refresh: Option<Instant>,
}

impl State {
    /// The detections held for the trailing day, aged by the oldest download they come
    /// from; `None` when no source has any.
    fn answer(&self) -> Option<Fetched> {
        let window = parsing::trailing_window(Timestamp::now());
        let held: Vec<&SourceData> =
            self.sources.iter().flatten().filter(|data| data.as_of >= *window.start()).collect();
        let as_of = held.iter().map(|data| data.as_of).min()?;
        let cells = held
            .iter()
            .flat_map(|data| &data.cells)
            .filter(|cell| window.contains(&cell.acquired_at))
            .cloned()
            .collect();
        let answer = Fetched {
            items: FeedItems::Fires(cells),
            as_of: Some(as_of),
            degraded: self.degraded.clone(),
            notices: Vec::new(),
        };
        Some(answer.with_dropped("NASA FIRMS", held.iter().map(|data| data.dropped).sum()))
    }
}

#[derive(Debug)]
pub struct FirmsProvider {
    config: FirmsConfig,
    policy: HostPolicy,
    state: Mutex<State>,
}

impl FirmsProvider {
    #[must_use]
    pub fn new(config: FirmsConfig) -> Self {
        let policy = HostPolicy::for_url(&config.base_url);
        let sources = config.sources.iter().map(|_| None).collect();
        Self { config, policy, state: Mutex::new(State { sources, ..State::default() }) }
    }

    fn source_url(&self, key: &str, source: &str) -> Result<Url, ProviderError> {
        let mut url = self.config.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| {
                ProviderError::Unconfigured("FIRMS URL requires a hierarchical base".to_owned())
            })?
            .pop_if_empty()
            .push(key)
            .push(source)
            .push("world")
            .push("2");
        Ok(url)
    }

    /// Downloads every source, keeping each good download as soon as it arrives, so a
    /// refresh that is interrupted loses nothing it already received.
    async fn refresh(
        &self,
        ctx: &FetchContext<'_>,
        key: &str,
        state: &mut State,
    ) -> Result<Fetched, ProviderError> {
        let started = Instant::now();
        let as_of = Timestamp::now();
        state.last_start = Some(started);
        let window_start = *parsing::trailing_window(as_of).start();
        let (mut failed, mut reused) = (0, 0);
        for (source, held) in self.config.sources.iter().zip(&mut state.sources) {
            // Finished by an interrupted refresh within this cadence: not asked again.
            if held.as_ref().is_some_and(|data| data.started.elapsed() < self.cadence()) {
                continue;
            }
            match self.download(ctx, key, source, as_of, started).await {
                Some(data) => *held = Some(data),
                None if ctx.cancel.is_cancelled() => return Err(FetchError::Cancelled.into()),
                None => {
                    failed += 1;
                    reused += usize::from(held.as_ref().is_some_and(|d| d.as_of >= window_start));
                }
            }
        }
        state.last_refresh = Some(started);
        state.degraded =
            (failed > 0).then(|| degraded_reason(failed, reused, self.config.sources.len()));
        state
            .answer()
            .ok_or_else(|| ProviderError::Unavailable("all FIRMS sources failed".to_owned()))
    }

    /// One source's detections from the trailing day, or `None` when the download or
    /// its parsing failed (logged by source name only: the URL holds the key).
    async fn download(
        &self,
        ctx: &FetchContext<'_>,
        key: &str,
        source: &str,
        as_of: Timestamp,
        started: Instant,
    ) -> Option<SourceData> {
        let url = self
            .source_url(key, source)
            .inspect_err(|err| tracing::warn!(source, error = %err, "FIRMS source URL failed"))
            .ok()?;
        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            timeout: Some(Duration::from_secs(self.config.request_timeout_s)),
            // The cadence is the retry schedule; the fetcher must not add requests.
            retries: Some(0),
            cancel: Some(ctx.cancel.clone()),
            ..FetchOptions::default()
        };
        let body = match ctx.fetcher.get(&url, &self.policy, &opts).await {
            Ok(response) => response.body,
            Err(FetchError::Cancelled) => return None,
            Err(err) => {
                tracing::warn!(source, error = %err, "FIRMS fetch failed");
                return None;
            }
        };
        let (detections, dropped) = parse_csv(&body)
            .inspect_err(|err| tracing::warn!(source, error = %err, "FIRMS parse failed"))
            .ok()?;
        let cells = filter_trailing_24h(&detections, as_of).into_iter().map(Into::into).collect();
        Some(SourceData { cells, dropped, as_of, started })
    }
}

/// The degradation reason of a refresh in which `failed` of `total` sources failed,
/// `reused` of which still have detections from an earlier download on the map.
fn degraded_reason(failed: usize, reused: usize, total: usize) -> String {
    let summary = format!("NASA FIRMS: {failed} of {total} sources failed to refresh.");
    if reused == 0 {
        summary
    } else {
        format!("{summary} Earlier detections are shown for {reused} of them.")
    }
}

fn map_key(ctx: &FetchContext<'_>) -> Result<String, ProviderError> {
    ctx.secrets
        .get(SecretKey::FirmsMapKey)
        .map(|s| s.expose_secret().trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ProviderError::Unconfigured("NASA FIRMS map key is not configured".to_owned())
        })
}

#[async_trait]
impl FeedProvider for FirmsProvider {
    fn feed(&self) -> FeedId {
        FeedId::Fires
    }

    fn source_id(&self) -> &'static str {
        sources::NASA_FIRMS
    }

    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s).max(CADENCE_FLOOR)
    }

    fn retry_floor(&self) -> Duration {
        self.cadence()
    }

    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let key = map_key(ctx)?;
        // A caller that arrives during a refresh waits for it (cancellably) and then
        // shares its result (the cooldown below), so it never starts a second download.
        let mut state = tokio::select! {
            () = ctx.cancel.cancelled() => return Err(FetchError::Cancelled.into()),
            state = self.state.lock() => state,
        };
        if ctx.cancel.is_cancelled() {
            return Err(FetchError::Cancelled.into());
        }
        if state.last_refresh.is_some_and(|t| t.elapsed() < self.cadence()) {
            return state.answer().ok_or_else(|| {
                ProviderError::Unavailable(
                    "NASA FIRMS refresh is cooling down after an unavailable response.".to_owned(),
                )
            });
        }
        // Only a refresh that did not finish can have begun less than a cadence ago.
        let spacing = state
            .last_start
            .map_or(Duration::ZERO, |t| RESTART_SPACING.saturating_sub(t.elapsed()));
        if !spacing.is_zero() {
            tokio::select! {
                () = ctx.cancel.cancelled() => return Err(FetchError::Cancelled.into()),
                () = tokio::time::sleep(spacing) => {}
            }
        }
        self.refresh(ctx, &key, &mut state).await
    }
}
