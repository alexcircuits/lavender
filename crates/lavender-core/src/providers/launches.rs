//! Space launches provider: recent and upcoming launches from Launch Library 2 (LL2),
//! matched to the satellites they carried, within LL2's free-tier request budget.
//!
//! One request per refresh asks `/launches/` for every launch 30 days either side of now,
//! newest first (a single page of at most 100: a larger window loses its oldest launches,
//! never the upcoming ones, and is reported in a notice). The free tier allows 15 requests
//! per hour: each request is counted against that budget before it is sent, the fetcher
//! neither retries nor follows redirects, and the budget and the last response are kept on
//! disk so a restart neither repeats a recent request nor resets the count (ENG-957).

mod budget;
mod cache;
pub mod parsing;

pub use parsing::{LaunchPage, parse};

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use lavender_domain::{FeedId, FeedItems, ProviderScope};
use secrecy::ExposeSecret;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::LaunchesConfig;
use crate::fetch::{FetchError, FetchOptions, FetchResponse, HostPolicy};
use crate::satellite_catalog::SatelliteCatalog;
use crate::secrets::SecretKey;
use crate::sources;
use budget::Budget;

/// Maximum upstream body size (12 MiB, matching the legacy limit).
const MAX_BODY_BYTES: usize = 12 * 1024 * 1024;
/// How far back and ahead of now the feed reaches.
const WINDOW: SignedDuration = SignedDuration::from_hours(30 * 24);
/// The largest page LL2 serves.
const PAGE_LIMIT: &str = "100";
const SOURCE_NAME: &str = "Launch Library 2";

/// The last response, as parsed.
#[derive(Debug, Clone)]
struct CachedLaunches {
    page: LaunchPage,
    downloaded_at: Timestamp,
}

#[derive(Debug, Default)]
struct LaunchesState {
    budget: Budget,
    /// Whether the budget an earlier process stored on disk has been read.
    budget_loaded: bool,
    cache: Option<CachedLaunches>,
}

#[derive(Debug)]
pub struct LaunchesProvider {
    config: LaunchesConfig,
    policy: HostPolicy,
    state: Mutex<LaunchesState>,
    /// Held while the budget is written to disk, so overlapping fetches write in turn.
    budget_writes: tokio::sync::Mutex<()>,
    satellite_catalog: Arc<SatelliteCatalog>,
}

impl LaunchesProvider {
    #[must_use]
    pub fn new(config: LaunchesConfig) -> Self {
        Self::with_satellite_catalog(config, Arc::new(SatelliteCatalog::new()))
    }

    #[must_use]
    pub fn with_satellite_catalog(
        config: LaunchesConfig,
        satellite_catalog: Arc<SatelliteCatalog>,
    ) -> Self {
        let policy = HostPolicy::for_url(&config.base_url);
        Self {
            config,
            policy,
            state: Mutex::new(LaunchesState::default()),
            budget_writes: tokio::sync::Mutex::new(()),
            satellite_catalog,
        }
    }

    #[must_use]
    pub fn with_catalog(mut self, satellite_catalog: Arc<SatelliteCatalog>) -> Self {
        self.satellite_catalog = satellite_catalog;
        self
    }

    /// Access the provider's satellite catalogue reference.
    #[must_use]
    pub fn satellite_catalog(&self) -> &Arc<SatelliteCatalog> {
        &self.satellite_catalog
    }

    /// The LL2 request for every launch from 30 days before `now` to 30 days after it,
    /// newest first.
    ///
    /// # Errors
    /// Returns [`ProviderError::Unconfigured`] when the base URL cannot be joined.
    pub fn build_url(&self, now: Timestamp) -> Result<Url, ProviderError> {
        let mut url = self
            .config
            .base_url
            .join("launches/")
            .map_err(|e| ProviderError::Unconfigured(format!("Launch Library 2 URL: {e}")))?;
        let start = now.checked_sub(WINDOW).unwrap_or(now);
        let end = now.checked_add(WINDOW).unwrap_or(now);
        url.query_pairs_mut()
            .append_pair("net__gte", &start.to_string())
            .append_pair("net__lte", &end.to_string())
            .append_pair("ordering", "-net")
            .append_pair("limit", PAGE_LIMIT)
            .append_pair("mode", "detailed");
        Ok(url)
    }

    fn lock(&self) -> MutexGuard<'_, LaunchesState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `cached` may answer without a request. The hub refreshes after the cadence
    /// give or take a tenth, so a response is reused only while younger than four fifths of
    /// the cadence: an early refresh still downloads rather than serving the same response
    /// for a second cycle, and a quick restart still reuses it.
    fn is_fresh(&self, cached: &CachedLaunches, now: Timestamp) -> bool {
        let age = now.duration_since(cached.downloaded_at);
        let reuse_for =
            SignedDuration::try_from(self.cadence() / 5 * 4).unwrap_or(SignedDuration::MAX);
        // A download "in the future" means the clock moved back: refresh rather than trust it.
        !age.is_negative() && age < reuse_for
    }

    /// The cached launches (memory, else the copy an earlier process saved), if any.
    async fn cached(&self, dir: Option<&Path>, url: &Url) -> Option<CachedLaunches> {
        let in_memory = self.lock().cache.clone();
        if in_memory.is_some() {
            return in_memory;
        }
        let (page, downloaded_at) = cache::load_response(dir, url).await?;
        let mut state = self.lock();
        Some(state.cache.get_or_insert(CachedLaunches { page, downloaded_at }).clone())
    }

    /// Reserves one request from the hourly budget and records it on disk before it is
    /// sent, or says why no request may be sent now.
    async fn reserve_request(&self, dir: Option<&Path>, now: Timestamp) -> Result<(), String> {
        self.load_stored_budget(dir).await;
        {
            let mut state = self.lock();
            if let Some(reason) = state.budget.refusal(now, self.config.max_requests_per_hour) {
                return Err(reason);
            }
            state.budget.record_attempt(now);
        }
        self.store_budget(dir).await;
        Ok(())
    }

    /// Adds the attempts an earlier process recorded, once.
    async fn load_stored_budget(&self, dir: Option<&Path>) {
        let Some(dir) = dir else { return };
        let loaded = self.lock().budget_loaded;
        if loaded {
            return;
        }
        let stored = cache::load_budget(dir).await;
        let mut state = self.lock();
        if !state.budget_loaded {
            state.budget_loaded = true;
            if let Some(stored) = stored {
                state.budget.absorb(stored);
            }
        }
    }

    /// Applies `change` to the budget, stores the result and returns what `change` returned.
    async fn update_budget<T>(
        &self,
        dir: Option<&Path>,
        change: impl FnOnce(&mut Budget) -> T,
    ) -> T {
        let result = change(&mut self.lock().budget);
        self.store_budget(dir).await;
        result
    }

    /// Writes the budget to disk. It is copied only once this write's turn has come, so of
    /// overlapping fetches the one that writes last writes every attempt recorded so far.
    async fn store_budget(&self, dir: Option<&Path>) {
        let Some(dir) = dir else { return };
        let _turn = self.budget_writes.lock().await;
        let budget = self.lock().budget.clone();
        cache::save_budget(dir, budget).await;
    }

    fn fetch_options(ctx: &FetchContext<'_>) -> FetchOptions {
        FetchOptions {
            max_body_bytes: Some(MAX_BODY_BYTES),
            accept: "application/json",
            cancel: Some(ctx.cancel.clone()),
            // One reserved request is exactly one HTTP request: the hourly budget, not the
            // fetcher, decides when Launch Library 2 is asked again.
            retries: Some(0),
            redirects: Some(0),
            auth_header: ctx
                .secrets
                .get(SecretKey::Ll2ApiToken)
                .map(|token| format!("Token {}", token.expose_secret())),
            ..FetchOptions::default()
        }
    }

    /// Turns the upstream answer into the feed answer, keeping the cache and budget current.
    async fn answer_from(
        &self,
        response: Result<FetchResponse, FetchError>,
        cached: Option<&CachedLaunches>,
        dir: Option<&Path>,
        url: &Url,
        now: Timestamp,
    ) -> Result<Fetched, ProviderError> {
        let response = match response {
            Ok(response) => response,
            Err(FetchError::Status { status: 429, .. }) => {
                let cooldown = self.update_budget(dir, |b| b.record_throttled(now)).await;
                let reason = format!(
                    "Launch Library 2 returned HTTP 429; backing off for {} s",
                    cooldown.as_secs()
                );
                return self.stale_or(
                    cached,
                    ProviderError::Unavailable(reason.clone()),
                    reason,
                    now,
                );
            }
            Err(err) => {
                let reason = format!("Launch Library 2 fetch failed: {err}");
                return self.stale_or(cached, ProviderError::Fetch(err), reason, now);
            }
        };
        self.update_budget(dir, Budget::record_success).await;
        let page = match parse(&response.body) {
            Ok(page) => page,
            Err(err) => {
                let reason = err.public_message();
                return self.stale_or(cached, err, reason, now);
            }
        };
        let fresh = CachedLaunches { page, downloaded_at: now };
        self.lock().cache = Some(fresh.clone());
        cache::save_response(dir, url, &response.body, now).await;
        Ok(self.answer(&fresh, None, now))
    }

    /// The cached answer marked degraded by `reason`, or `error` when nothing is cached.
    fn stale_or(
        &self,
        cached: Option<&CachedLaunches>,
        error: ProviderError,
        reason: String,
        now: Timestamp,
    ) -> Result<Fetched, ProviderError> {
        cached.map(|c| self.answer(c, Some(reason), now)).ok_or(error)
    }

    /// The feed answer for `cached`, matched against the current satellite catalogue.
    fn answer(&self, cached: &CachedLaunches, degraded: Option<String>, now: Timestamp) -> Fetched {
        let page = &cached.page;
        let mut launches = page.launches.clone();
        self.satellite_catalog.match_launches(&mut launches, now);
        let mut notices = Vec::new();
        if page.not_returned > 0 {
            notices.push(format!(
                "Launch Library 2 has {} launches in this window; this page holds only the latest {} by launch time, so the oldest are missing.",
                page.rows + page.not_returned,
                page.rows
            ));
        }
        Fetched {
            items: FeedItems::Launches(launches),
            as_of: Some(cached.downloaded_at),
            degraded,
            notices,
        }
        .with_dropped(SOURCE_NAME, page.dropped)
    }
}

#[async_trait]
impl FeedProvider for LaunchesProvider {
    fn feed(&self) -> FeedId {
        FeedId::Launches
    }

    fn source_id(&self) -> &'static str {
        sources::LAUNCH_LIBRARY_2
    }

    fn cadence(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s)
    }

    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    fn retry_floor(&self) -> Duration {
        Duration::from_secs(self.config.retry_floor_s)
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        if !self.enabled() {
            return Err(ProviderError::Unconfigured(
                "Launch Library 2 provider is disabled in configuration.".into(),
            ));
        }
        let now = Timestamp::now();
        let url = self.build_url(now)?;
        let cached = self.cached(ctx.cache_dir, &url).await;
        if let Some(cached) = cached.as_ref().filter(|c| self.is_fresh(c, now)) {
            return Ok(self.answer(cached, None, now));
        }
        if let Err(reason) = self.reserve_request(ctx.cache_dir, now).await {
            return self.stale_or(
                cached.as_ref(),
                ProviderError::Unavailable(reason.clone()),
                reason,
                now,
            );
        }
        let response = ctx.fetcher.get(&url, &self.policy, &Self::fetch_options(ctx)).await;
        self.answer_from(response, cached.as_ref(), ctx.cache_dir, &url, now).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downloaded(seconds_ago: i64, now: Timestamp) -> CachedLaunches {
        let page = LaunchPage { launches: Vec::new(), dropped: 0, rows: 0, not_returned: 0 };
        CachedLaunches { page, downloaded_at: now - SignedDuration::from_secs(seconds_ago) }
    }

    #[test]
    fn a_refresh_that_comes_up_to_a_tenth_early_still_downloads() {
        // The hub refreshes after the cadence give or take 10 %. A cache that is still
        // "fresh" at an early refresh would be served for a second cycle, so data could be
        // almost twice the cadence old.
        let provider = LaunchesProvider::new(LaunchesConfig::default());
        assert_eq!(provider.cadence(), Duration::from_mins(15));
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        assert!(provider.is_fresh(&downloaded(0, now), now));
        assert!(provider.is_fresh(&downloaded(600, now), now), "a quick restart reuses it");
        assert!(!provider.is_fresh(&downloaded(810, now), now), "the earliest hub refresh");
        assert!(!provider.is_fresh(&downloaded(900, now), now));
        assert!(!provider.is_fresh(&downloaded(-60, now), now), "the clock moved back");
    }
}
