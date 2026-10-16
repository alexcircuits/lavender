//! NOAA NHC/CPHC status positions and same-advisory forecast geometry.
//! Network is confined to the supplied Fetcher and configured fixed resource roots.

mod parsing;
#[cfg(test)]
mod tests;

pub use parsing::{attach_geometry, parse_status};

use std::time::{Duration, Instant};

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::{Cyclone, CycloneGeometryStatus, FeedId, FeedItems, ProviderScope};
use tokio::sync::Mutex;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::CyclonesConfig;
use crate::fetch::{FetchError, FetchOptions, HostPolicy};
use crate::sources;

pub const COVERAGE_NOTICE: &str =
    "Atlantic and eastern/central North Pacific; not worldwide cyclone coverage.";
pub const CONE_NOTICE: &str = "Forecast cones describe centre-track uncertainty, not the extent of wind, rain or other hazards.";
const CADENCE_FLOOR: Duration = Duration::from_secs(600);
const CACHE_MAX_AGE_S: i64 = 43_200;

#[derive(Debug, Clone)]
struct Cached {
    storms: Vec<Cyclone>,
    fetched_at: Timestamp,
    degraded: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    cache: Option<Cached>,
    attempted_at: Option<Instant>,
}

/// Roll back unfinished attempts even when a caller drops the future before its
/// cancellation branch can run. The mutex guard keeps this state exclusive.
struct Attempt<'a> {
    state: &'a mut State,
    previous: Option<Instant>,
    finished: bool,
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.state.attempted_at = self.previous;
        }
    }
}

#[derive(Debug)]
pub struct CyclonesProvider {
    config: CyclonesConfig,
    status_policy: HostPolicy,
    gis_policy: HostPolicy,
    state: Mutex<State>,
}

impl CyclonesProvider {
    #[must_use]
    pub fn new(config: CyclonesConfig) -> Self {
        Self {
            status_policy: HostPolicy::for_url(&config.status_base_url),
            gis_policy: HostPolicy::for_url(&config.gis_base_url),
            config,
            state: Mutex::new(State::default()),
        }
    }

    async fn document(
        &self,
        ctx: &FetchContext<'_>,
        url: &Url,
        policy: &HostPolicy,
        cap: usize,
    ) -> Result<Vec<u8>, ProviderError> {
        let response = ctx
            .fetcher
            .get(
                url,
                policy,
                &FetchOptions {
                    max_body_bytes: Some(cap),
                    retries: Some(0),
                    cancel: Some(ctx.cancel.clone()),
                    ..FetchOptions::default()
                },
            )
            .await?;
        Ok(response.body.to_vec())
    }

    fn gis_url(&self, index: usize) -> Result<Url, ProviderError> {
        let mut url = self
            .config
            .gis_base_url
            .join(&format!("{}/query", index + 5))
            .map_err(|_| ProviderError::Unconfigured("invalid NHC GIS root".to_owned()))?;
        url.query_pairs_mut()
            .append_pair("where", "1=1")
            .append_pair(
                "outFields",
                if index == 0 {
                    "idp_source,advisnum,tau,maxwind,gust"
                } else {
                    "idp_source,advisnum"
                },
            )
            .append_pair("outSR", "4326")
            .append_pair("resultRecordCount", if index == 0 { "500" } else { "32" })
            .append_pair("geometryPrecision", "4")
            .append_pair("f", "geojson");
        Ok(url)
    }

    async fn refresh(&self, ctx: &FetchContext<'_>) -> Result<Cached, ProviderError> {
        let status_url = self
            .config
            .status_base_url
            .join("CurrentStorms.json")
            .map_err(|_| ProviderError::Unconfigured("invalid NHC status root".to_owned()))?;
        let status = self.document(ctx, &status_url, &self.status_policy, 128 * 1024).await?;
        let mut storms = parse_status(&status, Timestamp::now())?;
        let mut degraded = None;
        if !storms.is_empty() {
            let urls = [self.gis_url(0)?, self.gis_url(1)?, self.gis_url(2)?];
            let (points, tracks, cones) = tokio::join!(
                self.document(ctx, &urls[0], &self.gis_policy, 512 * 1024),
                self.document(ctx, &urls[1], &self.gis_policy, 512 * 1024),
                self.document(ctx, &urls[2], &self.gis_policy, 2 * 1024 * 1024),
            );
            if ctx.cancel.is_cancelled() {
                return Err(FetchError::Cancelled.into());
            }
            match (points, tracks, cones) {
                (Ok(p), Ok(t), Ok(c)) if attach_geometry(&mut storms, [&p, &t, &c]).is_ok() => {
                    if storms.iter().any(|s| s.geometry_status != CycloneGeometryStatus::Current) {
                        degraded = Some("Some forecast geometry has not reported the current advisory; current positions remain available.".to_owned());
                    }
                }
                _ => {
                    for storm in &mut storms {
                        storm.geometry_status = CycloneGeometryStatus::Unavailable;
                    }
                    degraded = Some("Forecast geometry unavailable or invalid; current advisory positions remain available.".to_owned());
                }
            }
        }
        if ctx.cancel.is_cancelled() {
            return Err(FetchError::Cancelled.into());
        }
        Ok(Cached { storms, fetched_at: Timestamp::now(), degraded })
    }

    fn answer(value: &Cached, failure: Option<String>) -> Fetched {
        Fetched {
            items: FeedItems::Cyclones(value.storms.clone()),
            as_of: Some(value.fetched_at),
            degraded: failure.or_else(|| value.degraded.clone()),
            notices: vec![COVERAGE_NOTICE.to_owned(), CONE_NOTICE.to_owned()],
        }
    }

    fn usable(value: &Cached) -> bool {
        let now = Timestamp::now();
        now.duration_since(value.fetched_at).as_secs() <= CACHE_MAX_AGE_S
            && value.storms.iter().all(|s| {
                now.duration_since(s.issued_at).as_secs() <= CACHE_MAX_AGE_S
                    && now.duration_since(s.position_at).as_secs() <= CACHE_MAX_AGE_S
            })
    }
}

#[async_trait]
impl FeedProvider for CyclonesProvider {
    fn feed(&self) -> FeedId {
        FeedId::Cyclones
    }
    fn source_id(&self) -> &'static str {
        sources::NOAA_CYCLONES
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s).max(CADENCE_FLOOR)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        self.config.enabled
    }
    fn retry_floor(&self) -> Duration {
        // The provider's cooldown follows the configured cadence after both success
        // and failure. Hub jitter/backoff must not wake before that same interval.
        self.cadence()
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let mut state = tokio::select! {
            () = ctx.cancel.cancelled() => return Err(FetchError::Cancelled.into()),
            state = self.state.lock() => state,
        };
        if ctx.cancel.is_cancelled() {
            return Err(FetchError::Cancelled.into());
        }
        if state.attempted_at.is_some_and(|t| t.elapsed() < self.cadence()) {
            return state
                .cache
                .as_ref()
                .filter(|c| Self::usable(c))
                .map(|c| Self::answer(c, None))
                .ok_or_else(|| {
                    ProviderError::Unavailable(
                        "NOAA cyclone refresh is cooling down after an unavailable advisory."
                            .to_owned(),
                    )
                });
        }
        // Keep the provider's budget even when it is called outside the hub; cancellation
        // restores the previous attempt time so an abandoned request does not consume it.
        let previous = state.attempted_at.replace(Instant::now());
        let mut attempt = Attempt { state: &mut state, previous, finished: false };
        match self.refresh(ctx).await {
            Ok(value) => {
                attempt.finished = true;
                let answer = Self::answer(&value, None);
                attempt.state.cache = Some(value);
                Ok(answer)
            }
            Err(error) if ctx.cancel.is_cancelled() => Err(error),
            Err(error) => {
                attempt.finished = true;
                if let Some(cache) = attempt.state.cache.as_mut().filter(|c| Self::usable(c)) {
                    cache.degraded =
                        Some("Cached cyclone advisory; upstream unavailable.".to_owned());
                    Ok(Self::answer(cache, None))
                } else {
                    Err(error)
                }
            }
        }
    }
}
