//! Satellite orbital elements from CelesTrak's GP API in OMM JSON.
//!
//! CelesTrak asks clients to download each group at most once every two hours and blocks
//! abusive clients. So each group is cached (on disk when a cache directory is configured,
//! and in memory), reused while fresh, and reused when a refresh fails. Requests are
//! throttled per group: after a download the group is not requested again for a full
//! cadence, after a failure not for a twelfth of it, and a fetch never retries a group
//! within the same attempt. Cached data carries its real age, so a snapshot built from an
//! old cache is reported as old (and as degraded when a refresh failed), never as live.
//! Licence status is `Unknown` (ENG-938).

mod cache;
mod parsing;
use cache::{load_cached_group, save_cached_group};
pub use parsing::parse;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use jiff::Timestamp;
use lavender_domain::text::clean;
use lavender_domain::{FeedId, FeedItems, ProviderScope, SatelliteElements};
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::CelestrakConfig;
use crate::fetch::{CacheValidators, FetchError, FetchOptions, FetchResponse, HostPolicy};
use crate::sources;

const MAX_BODY: usize = 20 * 1024 * 1024;
/// After a failed request, a group waits `cadence / FAILURE_RETRY_DIVISOR` (10 minutes at
/// the default two-hour cadence) before it is requested again.
const FAILURE_RETRY_DIVISOR: u32 = 12;

/// The outcome of the last upstream request for a group.
#[derive(Debug, Clone, Copy)]
struct Attempt {
    at: Instant,
    succeeded: bool,
}

/// A group's validated representation and its separate download/revalidation times.
#[derive(Debug, Clone)]
struct GroupData {
    elements: Vec<SatelliteElements>,
    dropped: usize,
    body: Bytes,
    downloaded_at: Timestamp,
    validated_at: Timestamp,
    validators: Option<CacheValidators>,
}

#[derive(Debug)]
enum GroupOutcome {
    /// Downloaded now, or cached and still within the cadence.
    Fresh(GroupData),
    /// The refresh failed or is throttled; older data is served instead.
    Stale(GroupData, String),
    /// Nothing is available for the group.
    Missing(String),
}

#[derive(Debug)]
pub struct CelestrakProvider {
    config: CelestrakConfig,
    policy: HostPolicy,
    attempts: Mutex<HashMap<String, Attempt>>,
    memory: Mutex<HashMap<String, GroupData>>,
}

impl CelestrakProvider {
    #[must_use]
    pub fn new(config: CelestrakConfig) -> Self {
        let policy = HostPolicy::for_url(&config.base_url);
        Self {
            config,
            policy,
            attempts: Mutex::new(HashMap::new()),
            memory: Mutex::new(HashMap::new()),
        }
    }

    fn group_url(&self, group: &str) -> Result<Url, ProviderError> {
        if group.is_empty()
            || group.len() > 64
            || !group.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(ProviderError::Unconfigured("CelesTrak group name is invalid.".into()));
        }
        let mut url = self
            .config
            .base_url
            .join("gp.php")
            .map_err(|e| ProviderError::Unconfigured(format!("CelesTrak URL: {e}")))?;
        url.query_pairs_mut().append_pair("GROUP", group).append_pair("FORMAT", "json");
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ProviderError::Unconfigured(
                "CelesTrak URLs must not contain credentials.".into(),
            ));
        }
        Ok(url)
    }

    fn cadence_duration(&self) -> Duration {
        Duration::from_secs(self.config.cadence_s)
    }

    /// How long until `group` may be requested again, if it may not be requested now.
    fn throttled_for(&self, group: &str) -> Option<Duration> {
        let attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let last = attempts.get(group)?;
        let floor = if last.succeeded {
            self.cadence_duration()
        } else {
            self.cadence_duration() / FAILURE_RETRY_DIVISOR
        };
        floor.checked_sub(last.at.elapsed()).filter(|d| !d.is_zero())
    }

    /// Records a request for `group`; returns the previous record so a cancelled request
    /// can be forgotten.
    fn record_attempt(&self, group: &str, succeeded: bool) -> Option<Attempt> {
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        attempts.insert(group.to_owned(), Attempt { at: Instant::now(), succeeded })
    }

    fn restore_attempt(&self, group: &str, previous: Option<Attempt>) {
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        match previous {
            Some(p) => attempts.insert(group.to_owned(), p),
            None => attempts.remove(group),
        };
    }

    fn remember(&self, group: &str, data: &GroupData) {
        let mut memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
        memory.insert(group.to_owned(), data.clone());
    }

    /// The newest copy of `group` held in memory or on disk, whatever its age.
    async fn cached(&self, dir: Option<&Path>, group: &str, url: &Url) -> Option<GroupData> {
        let in_memory = {
            let memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
            memory.get(group).cloned()
        };
        let on_disk = load_cached_group(dir, group, url).await;
        match (in_memory, on_disk) {
            (Some(m), Some(d)) => Some(if d.validated_at > m.validated_at { d } else { m }),
            (m, d) => m.or(d),
        }
    }

    async fn load_group(&self, ctx: &FetchContext<'_>, group: &str) -> GroupOutcome {
        let url = match self.group_url(group) {
            Ok(url) => url,
            Err(error) => return GroupOutcome::Missing(error.public_message()),
        };
        let cached = self.cached(ctx.cache_dir, group, &url).await;
        let now = Timestamp::now();
        if let Some(data) = &cached
            && age(now, data.validated_at) <= self.cadence_duration()
        {
            return GroupOutcome::Fresh(data.clone());
        }
        if let Some(wait) = self.throttled_for(group) {
            let reason = format!("next request allowed in {}", format_duration(wait));
            return stale_or_missing(cached, reason);
        }
        // One request per group per attempt: the throttle above, not fetcher retries,
        // decides when CelesTrak is asked again.
        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            cancel: Some(ctx.cancel.clone()),
            retries: Some(0),
            conditional: cached.as_ref().and_then(|data| data.validators.clone()),
            ..FetchOptions::default()
        };
        let previous = self.record_attempt(group, false);
        let reason = match ctx.fetcher.get(&url, &self.policy, &opts).await {
            Ok(response) => match refreshed_group(response, cached.as_ref(), group) {
                Ok(data) => {
                    self.record_attempt(group, true);
                    save_cached_group(ctx.cache_dir, group, &url, &data).await;
                    self.remember(group, &data);
                    return GroupOutcome::Fresh(data);
                }
                Err(e) => e.public_message(),
            },
            Err(FetchError::Cancelled) => {
                self.restore_attempt(group, previous);
                "the request was cancelled".to_owned()
            }
            Err(e) => ProviderError::from(e).public_message(),
        };
        tracing::warn!(group, error = %reason, "CelesTrak group refresh failed; using cache if present");
        stale_or_missing(cached, reason)
    }
}

/// A 304 validates the existing bytes; it never feeds an empty response into OMM parsing.
fn refreshed_group(
    response: FetchResponse,
    cached: Option<&GroupData>,
    group: &str,
) -> Result<GroupData, ProviderError> {
    let now = Timestamp::now();
    if response.status == 304 {
        let mut data = cached.cloned().ok_or_else(|| {
            ProviderError::Unavailable(
                "CelesTrak returned 304 without a cached representation.".into(),
            )
        })?;
        let new = response.validators.as_ref();
        let old = data.validators.as_ref();
        data.validators = CacheValidators::new(
            new.and_then(CacheValidators::etag).or_else(|| old.and_then(CacheValidators::etag)),
            new.and_then(CacheValidators::last_modified)
                .or_else(|| old.and_then(CacheValidators::last_modified)),
        )
        .ok();
        data.validated_at = now;
        return Ok(data);
    }
    let (elements, dropped) = parse(&response.body, group)?;
    Ok(GroupData {
        elements,
        dropped,
        body: response.body,
        downloaded_at: now,
        validated_at: now,
        validators: response.validators,
    })
}

fn stale_or_missing(cached: Option<GroupData>, reason: String) -> GroupOutcome {
    match cached {
        Some(data) => GroupOutcome::Stale(data, reason),
        None => GroupOutcome::Missing(reason),
    }
}

/// How long ago `then` was (zero for future times).
fn age(now: Timestamp, then: Timestamp) -> Duration {
    Duration::try_from(now.duration_since(then)).unwrap_or(Duration::ZERO)
}

/// A rough, readable duration: "45 s", "12 min", "5 h", "3 days".
fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..120 => format!("{s} s"),
        120..7_200 => format!("{} min", s / 60),
        7_200..172_800 => format!("{} h", s / 3_600),
        _ => format!("{} days", s / 86_400),
    }
}

/// Sentence describing groups that could not be refreshed, for `Degraded` freshness.
fn degraded_reason(problems: &[(String, String)], oldest_stale: Option<Duration>) -> String {
    let names: Vec<&str> = problems.iter().map(|(g, _)| g.as_str()).collect();
    let noun = if names.len() == 1 { "group" } else { "groups" };
    let first = problems.first().map_or("", |(_, r)| r.as_str());
    let mut reason =
        format!("CelesTrak {noun} {} could not be refreshed ({first}).", names.join(", "));
    if let Some(age) = oldest_stale {
        let _ = write!(reason, " Showing elements last validated {} ago.", format_duration(age));
    }
    reason
}

#[async_trait]
impl FeedProvider for CelestrakProvider {
    fn feed(&self) -> FeedId {
        FeedId::Satellites
    }
    fn source_id(&self) -> &'static str {
        sources::CELESTRAK
    }
    fn cadence(&self) -> Duration {
        self.cadence_duration()
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        self.config.enabled
    }
    fn retry_floor(&self) -> Duration {
        self.cadence_duration() / FAILURE_RETRY_DIVISOR
    }

    async fn fetch(
        &self,
        ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        let now = Timestamp::now();
        let mut seen = HashSet::new();
        let mut all = Vec::new();
        let mut as_of: Option<Timestamp> = None;
        let mut dropped = 0;
        let mut problems: Vec<(String, String)> = Vec::new();
        let mut oldest_stale: Option<Duration> = None;
        for group in &self.config.groups {
            let data = match self.load_group(ctx, group).await {
                GroupOutcome::Fresh(data) => data,
                GroupOutcome::Stale(data, reason) => {
                    problems.push((group.clone(), reason));
                    let stale_for = age(now, data.validated_at);
                    oldest_stale = Some(oldest_stale.map_or(stale_for, |o| o.max(stale_for)));
                    data
                }
                GroupOutcome::Missing(reason) => {
                    problems.push((group.clone(), reason));
                    continue;
                }
            };
            as_of = Some(as_of.map_or(data.validated_at, |t| t.min(data.validated_at)));
            dropped += data.dropped;
            all.extend(data.elements.into_iter().filter(|e| seen.insert(e.norad_id)));
        }
        if as_of.is_none() {
            return Err(ProviderError::Unavailable(clean(
                &degraded_reason(&problems, None),
                super::MAX_ERROR_CHARS,
            )));
        }
        let degraded = (!problems.is_empty()).then(|| degraded_reason(&problems, oldest_stale));
        Ok(Fetched { items: FeedItems::Satellites(all), as_of, degraded, notices: Vec::new() }
            .with_dropped("CelesTrak", dropped))
    }
}

#[cfg(test)]
mod tests;
