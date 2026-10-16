//! NIFC WFIGS current wildland fire perimeters: validated, cleaned, simplified once per
//! detail level, and linked to their InciWeb incident pages when those are verified.
//!
//! Network is confined to the supplied Fetcher, the configured WFIGS layer's `query`
//! resource and InciWeb's index and incident pages. InciWeb is optional enrichment: its
//! failures, slowness or markup changes only cost the links, never the perimeters.
//! InciWeb requests are rare: the index is reused for hours, each page is read at most
//! once a day, a few pages per refresh, one request per second. InciWeb never holds a
//! refresh's perimeters back by more than a few seconds.

mod detail;
mod inciweb;
#[cfg(test)]
mod tests;
mod wfigs;

pub use detail::{
    LEVELS, Level, MAX_RESPONSE_BYTES, MAX_RESPONSE_VERTICES, all_levels, select_for_view,
};
pub use inciweb::{
    IndexEntry, PageFacts, find_entry, is_current, parse_index, parse_page as parse_incident_page,
};
pub use wfigs::{Page, Snapshot, keep_largest, parse_page as parse_wfigs_page, query_url};

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use jiff::Timestamp;
use lavender_domain::{FeedId, FeedItems, FirePerimeter, ProviderScope};
use tokio::sync::Mutex;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use crate::config::PerimetersConfig;
use crate::fetch::{FetchError, FetchOptions, HostPolicy};
use crate::sources;

pub const COVERAGE_NOTICE: &str =
    "United States wildland fires only; WFIGS does not publish a perimeter for every incident.";
pub const LINKS_UNAVAILABLE_NOTICE: &str =
    "InciWeb incident links are unavailable right now; perimeters are unaffected.";
pub const TRUNCATED_NOTICE: &str =
    "Showing the largest fires that fit this view's size limits. Zoom in to see the rest.";
/// Why the previous perimeters are still shown after one empty WFIGS answer.
pub const EMPTY_UNCONFIRMED: &str = "NIFC WFIGS answered with no perimeters; showing the previous ones until the next refresh confirms it.";
const CADENCE_FLOOR: Duration = Duration::from_secs(300);
/// The provider stops offering cached perimeters this old while WFIGS stays unavailable.
/// (The hub, as for every feed, may keep showing its own last copy, marked degraded with
/// its age.)
const CACHE_MAX_AGE_S: i64 = 43_200;
/// The most time one refresh spends on InciWeb. The refresh's perimeters wait for it (a
/// layer's first snapshot waits for its first refresh), so it is short: the index and
/// about four pages at one request a second. Pages left unread wait for a later refresh.
const INCIWEB_BUDGET: Duration = Duration::from_secs(6);
/// A whole refresh aims to finish within this, inside the hub's 25 s wait for a first
/// snapshot: a slow WFIGS answer leaves less time for InciWeb.
const REFRESH_BUDGET: Duration = Duration::from_secs(20);
/// An InciWeb request starts only with this much of the refresh's InciWeb time left: one
/// second for the request spacing and one for the answer. A refresh left with less skips
/// InciWeb.
const MIN_INCIWEB_TIME: Duration = Duration::from_secs(2);
const INCIWEB_SPACING: Duration = Duration::from_secs(1);
const INDEX_RETRY: Duration = Duration::from_mins(30);
const PAGE_TTL: Duration = Duration::from_hours(24);
const PAGE_RETRY: Duration = Duration::from_hours(1);
const MAX_CACHED_PAGES: usize = 512;

#[derive(Debug, Clone)]
struct Cached {
    /// Every detail level of every perimeter (see [`all_levels`]).
    perimeters: Vec<FirePerimeter>,
    fetched_at: Timestamp,
    notices: Vec<String>,
}

/// The InciWeb index and the incident pages read so far, by page slug.
#[derive(Debug, Default)]
struct InciwebState {
    index: Option<Vec<IndexEntry>>,
    /// When the index was last requested, and whether that request failed.
    index_attempt: Option<(Instant, bool)>,
    pages: HashMap<String, PageCheck>,
}

#[derive(Debug, Clone, Copy)]
struct PageCheck {
    facts: Option<PageFacts>,
    checked: Instant,
}

impl InciwebState {
    fn index_due(&self, ttl: Duration) -> bool {
        self.index_attempt.is_none_or(|(at, failed)| {
            at.elapsed() >= if failed || self.index.is_none() { INDEX_RETRY } else { ttl }
        })
    }

    fn page_due(&self, slug: &str) -> bool {
        self.pages.get(slug).is_none_or(|check| {
            check.checked.elapsed() >= if check.facts.is_some() { PAGE_TTL } else { PAGE_RETRY }
        })
    }

    fn remember(&mut self, slug: String, facts: Option<PageFacts>) {
        if self.pages.len() >= MAX_CACHED_PAGES && !self.pages.contains_key(&slug) {
            let oldest = self.pages.iter().min_by_key(|(_, c)| c.checked).map(|(s, _)| s.clone());
            if let Some(oldest) = oldest {
                self.pages.remove(&oldest);
            }
        }
        self.pages.insert(slug, PageCheck { facts, checked: Instant::now() });
    }

    /// The fire's verified InciWeb page, if any.
    fn link(&self, fire: &FirePerimeter, base: &Url, now: Timestamp) -> Option<String> {
        let entry = find_entry(self.index.as_deref()?, fire)?;
        let facts = self.pages.get(&entry.slug)?.facts?;
        is_current(&facts, fire.discovered_at, now)
            .then(|| page_url(base, &entry.slug).ok().map(String::from))
            .flatten()
    }
}

/// Time left for InciWeb after WFIGS took `elapsed` of a refresh meant to last `refresh`;
/// zero when too little remains.
fn inciweb_budget(elapsed: Duration, refresh: Duration) -> Duration {
    let left = refresh.saturating_sub(elapsed).min(INCIWEB_BUDGET);
    if left < MIN_INCIWEB_TIME { Duration::ZERO } else { left }
}

fn inciweb_url(base: &Url, path: &str) -> Result<Url, ProviderError> {
    base.join(path).map_err(|_| ProviderError::Unconfigured("invalid InciWeb root".to_owned()))
}

fn page_url(base: &Url, slug: &str) -> Result<Url, ProviderError> {
    inciweb_url(base, &format!("incident-information/{slug}"))
}

/// How one InciWeb request ended.
enum Reply {
    /// The body, or why there is none.
    Done(Result<Vec<u8>, ProviderError>),
    /// The refresh's InciWeb time ran out first.
    OutOfTime,
    /// The caller left.
    Cancelled,
}

/// Orders fires largest first by reported acres (an unknown size counts as zero), then
/// by id.
fn largest_first(a: &FirePerimeter, b: &FirePerimeter) -> Ordering {
    b.acres.unwrap_or(0.0).total_cmp(&a.acres.unwrap_or(0.0)).then_with(|| a.id.cmp(&b.id))
}

/// The outline positions a perimeter carries.
fn vertices(perimeter: &FirePerimeter) -> usize {
    perimeter.polygons.iter().flatten().map(Vec::len).sum()
}

/// The WFIGS pages of one snapshot read so far.
#[derive(Debug)]
struct WfigsRead {
    /// Outline positions the snapshot may keep in all.
    budget: usize,
    /// Valid perimeters at full detail.
    perimeters: Vec<FirePerimeter>,
    /// Features skipped for a missing, malformed or oversized shape or a repeated id.
    skipped: usize,
    /// The smallest valid fires, left out because the outline budget was spent.
    over_budget: usize,
    /// Whether WFIGS listed more pages than Lavender reads.
    unread_pages: bool,
}

impl WfigsRead {
    fn new(budget: usize) -> Self {
        Self { budget, perimeters: Vec::new(), skipped: 0, over_budget: 0, unread_pages: false }
    }

    /// Adds one page. Only the largest fires that fit the budget stay, so memory holds
    /// at most the budget plus one page.
    fn add(&mut self, page: Page) {
        self.skipped += page.dropped;
        let mut fires = std::mem::take(&mut self.perimeters);
        fires.extend(page.perimeters);
        let (kept, left_out) = keep_largest(fires, self.budget);
        self.perimeters = kept;
        self.over_budget += left_out;
    }

    /// What the analyst is told about this snapshot's coverage and what it left out.
    fn notices(&self) -> Vec<String> {
        let source = wfigs::SOURCE_NAME;
        let mut notices = vec![COVERAGE_NOTICE.to_owned()];
        if self.skipped > 0 {
            notices.push(format!(
                "{source}: {} perimeter record(s) skipped: missing, invalid or oversized shape, or a repeated fire id.",
                self.skipped
            ));
        }
        if self.over_budget > 0 {
            notices.push(format!(
                "{source}: {} smaller fire(s) left out: this snapshot reached Lavender's limit of {} outline positions.",
                self.over_budget, self.budget
            ));
        }
        if self.unread_pages {
            notices.push(format!(
                "{source}: more perimeters are listed than Lavender reads ({} pages of {}).",
                wfigs::MAX_PAGES,
                wfigs::PAGE_SIZE
            ));
        }
        notices
    }
}

/// Every detail level of every fire, each carrying the fire's verified InciWeb link.
fn linked_levels(
    fires: Vec<FirePerimeter>,
    inciweb: &InciwebState,
    base: &Url,
    now: Timestamp,
) -> Vec<FirePerimeter> {
    fires
        .into_iter()
        .flat_map(|fire| {
            let inciweb_url = inciweb.link(&fire, base, now);
            all_levels(FirePerimeter { inciweb_url, ..fire })
        })
        .collect()
}

#[derive(Debug, Default)]
struct State {
    cache: Option<Cached>,
    /// Why the latest refresh failed, while cached perimeters are still served.
    failure: Option<String>,
    attempted_at: Option<Instant>,
    /// Whether the latest WFIGS answer listed no perimeters at all.
    answered_empty: bool,
    inciweb: InciwebState,
}

/// Rolls back an unfinished attempt even when a caller drops the future before its
/// cancellation branch runs. The mutex guard keeps this state exclusive.
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
pub struct PerimetersProvider {
    config: PerimetersConfig,
    wfigs_policy: HostPolicy,
    inciweb_policy: HostPolicy,
    /// How long one refresh aims to take ([`REFRESH_BUDGET`]; tests shorten it).
    refresh_budget: Duration,
    state: Mutex<State>,
}

impl PerimetersProvider {
    #[must_use]
    pub fn new(config: PerimetersConfig) -> Self {
        Self {
            wfigs_policy: HostPolicy::for_url(&config.wfigs_base_url),
            inciweb_policy: HostPolicy::for_url(&config.inciweb_base_url)
                .with_min_interval(INCIWEB_SPACING),
            config,
            refresh_budget: REFRESH_BUDGET,
            state: Mutex::new(State::default()),
        }
    }

    async fn document(
        ctx: &FetchContext<'_>,
        url: &Url,
        policy: &HostPolicy,
        cap: usize,
        accept: &'static str,
    ) -> Result<Vec<u8>, ProviderError> {
        let options = FetchOptions {
            max_body_bytes: Some(cap),
            accept,
            retries: Some(0),
            cancel: Some(ctx.cancel.clone()),
            ..FetchOptions::default()
        };
        Ok(ctx.fetcher.get(url, policy, &options).await?.body.to_vec())
    }

    /// Every current perimeter at full detail, following WFIGS's paging, within the
    /// snapshot's outline budget.
    async fn fetch_wfigs(
        &self,
        ctx: &FetchContext<'_>,
        now: Timestamp,
    ) -> Result<WfigsRead, ProviderError> {
        let mut ids = Snapshot::default();
        let mut read = WfigsRead::new(wfigs::MAX_SNAPSHOT_VERTICES);
        let mut offset = 0;
        for _ in 0..wfigs::MAX_PAGES {
            let url = query_url(&self.config.wfigs_base_url, offset)?;
            let body = Self::document(
                ctx,
                &url,
                &self.wfigs_policy,
                wfigs::MAX_PAGE_BYTES,
                "application/geo+json, application/json",
            )
            .await?;
            let page = parse_wfigs_page(&body, now, &mut ids)?;
            let (listed, more) = (page.features, page.more);
            read.add(page);
            offset += listed;
            if !more || listed == 0 {
                return Ok(read);
            }
        }
        read.unread_pages = true;
        Ok(read)
    }

    /// One InciWeb request, cut off at `deadline`.
    async fn inciweb_get(
        &self,
        ctx: &FetchContext<'_>,
        url: Result<Url, ProviderError>,
        deadline: tokio::time::Instant,
        cap: usize,
        accept: &'static str,
    ) -> Reply {
        let url = match url {
            Ok(url) => url,
            Err(error) => return Reply::Done(Err(error)),
        };
        let request = Self::document(ctx, &url, &self.inciweb_policy, cap, accept);
        match tokio::time::timeout_at(deadline, request).await {
            _ if ctx.cancel.is_cancelled() => Reply::Cancelled,
            Ok(body) => Reply::Done(body),
            Err(_) => Reply::OutOfTime,
        }
    }

    /// Refreshes the InciWeb index and reads a few due incident pages within `budget`.
    /// Never fails: problems leave the links missing. A request starts only with
    /// [`MIN_INCIWEB_TIME`] left, so pages there is no time for wait, unmarked, for the
    /// next refresh. Each request is recorded once it has ended: a failed one, or one
    /// still unanswered when the budget runs out, waits for its retry interval, while a
    /// refresh its caller abandons leaves no mark, so the next refresh asks again at once.
    async fn refresh_inciweb(
        &self,
        ctx: &FetchContext<'_>,
        state: &mut InciwebState,
        fires: &[FirePerimeter],
        budget: Duration,
    ) {
        if !self.config.inciweb_enabled || budget < MIN_INCIWEB_TIME {
            return;
        }
        let deadline = tokio::time::Instant::now() + budget;
        let time_left = || deadline.saturating_duration_since(tokio::time::Instant::now());
        let base = &self.config.inciweb_base_url;
        if state.index_due(Duration::from_secs(self.config.inciweb_index_ttl_s)) {
            let url = inciweb_url(base, "api/map_data");
            let reply =
                self.inciweb_get(ctx, url, deadline, inciweb::MAX_INDEX_BYTES, "application/json");
            let (index, out_of_time) = match reply.await {
                Reply::Cancelled => return,
                Reply::OutOfTime => (None, true),
                Reply::Done(body) => (body.and_then(|body| parse_index(&body)).ok(), false),
            };
            state.index_attempt = Some((Instant::now(), index.is_none()));
            if let Some((entries, _dropped)) = index {
                state.index = Some(entries);
            }
            if out_of_time {
                return;
            }
        }
        for slug in Self::pages_due(state, fires, self.config.inciweb_pages_per_refresh) {
            if time_left() < MIN_INCIWEB_TIME {
                return;
            }
            let url = page_url(base, &slug);
            let reply = self.inciweb_get(ctx, url, deadline, inciweb::MAX_PAGE_BYTES, "text/html");
            // Dates already read stay usable when a re-read fails.
            let known = state.pages.get(&slug).and_then(|check| check.facts);
            match reply.await {
                Reply::Cancelled => return,
                Reply::OutOfTime => {
                    // The pages read so far are kept and the rest wait.
                    state.remember(slug, known);
                    return;
                }
                Reply::Done(body) => {
                    let facts = body.and_then(|body| parse_incident_page(&body)).ok();
                    state.remember(slug, facts.or(known));
                }
            }
        }
    }

    /// Slugs of matched incident pages to read now: the largest fires first.
    fn pages_due(state: &InciwebState, fires: &[FirePerimeter], limit: usize) -> Vec<String> {
        let Some(index) = state.index.as_deref() else { return Vec::new() };
        let mut by_size: Vec<&FirePerimeter> = fires.iter().collect();
        by_size.sort_by(|a, b| largest_first(a, b));
        let mut slugs: Vec<String> = Vec::new();
        for fire in by_size {
            if slugs.len() >= limit {
                break;
            }
            if let Some(entry) = find_entry(index, fire)
                && state.page_due(&entry.slug)
                && !slugs.contains(&entry.slug)
            {
                slugs.push(entry.slug.clone());
            }
        }
        slugs
    }

    async fn refresh(
        &self,
        ctx: &FetchContext<'_>,
        inciweb: &mut InciwebState,
    ) -> Result<Cached, ProviderError> {
        let (started, now) = (Instant::now(), Timestamp::now());
        let read = self.fetch_wfigs(ctx, now).await?;
        let fetched_at = Timestamp::now();
        let budget = inciweb_budget(started.elapsed(), self.refresh_budget);
        self.refresh_inciweb(ctx, inciweb, &read.perimeters, budget).await;
        if ctx.cancel.is_cancelled() {
            return Err(FetchError::Cancelled.into());
        }
        let mut notices = read.notices();
        if self.config.inciweb_enabled && inciweb.index.is_none() {
            notices.push(LINKS_UNAVAILABLE_NOTICE.to_owned());
        }
        let perimeters =
            linked_levels(read.perimeters, inciweb, &self.config.inciweb_base_url, now);
        Ok(Cached { perimeters, fetched_at, notices })
    }

    fn answer(value: &Cached, failure: Option<&String>) -> Fetched {
        Fetched {
            items: FeedItems::Perimeters(value.perimeters.clone()),
            as_of: Some(value.fetched_at),
            degraded: failure.cloned(),
            notices: value.notices.clone(),
        }
    }

    fn usable(value: &Cached) -> bool {
        Timestamp::now().duration_since(value.fetched_at).as_secs() <= CACHE_MAX_AGE_S
    }
}

#[async_trait]
impl FeedProvider for PerimetersProvider {
    fn feed(&self) -> FeedId {
        FeedId::Perimeters
    }
    fn source_id(&self) -> &'static str {
        sources::NIFC_WFIGS
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
        // The provider cools down for one cadence after every attempt; the hub must not
        // wake it sooner.
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
                .map(|c| Self::answer(c, state.failure.as_ref()))
                .ok_or_else(|| {
                    ProviderError::Unavailable(
                        "NIFC WFIGS refresh is cooling down after an unavailable snapshot."
                            .to_owned(),
                    )
                });
        }
        // Concurrent and direct callers share one refresh per cadence; cancellation
        // restores the previous attempt time so an abandoned request costs nothing.
        let previous = state.attempted_at.replace(Instant::now());
        let mut attempt = Attempt { state: &mut state, previous, finished: false };
        let result = self.refresh(ctx, &mut attempt.state.inciweb).await;
        match result {
            Ok(value) => {
                attempt.finished = true;
                let state = &mut *attempt.state;
                let was_empty =
                    std::mem::replace(&mut state.answered_empty, value.perimeters.is_empty());
                // One empty answer (for example while ArcGIS republishes the layer) does
                // not wipe a populated map; a second in a row is believed.
                if value.perimeters.is_empty()
                    && !was_empty
                    && let Some(cache) =
                        state.cache.as_ref().filter(|c| !c.perimeters.is_empty() && Self::usable(c))
                {
                    state.failure = Some(EMPTY_UNCONFIRMED.to_owned());
                    return Ok(Self::answer(cache, state.failure.as_ref()));
                }
                let answer = Self::answer(&value, None);
                state.cache = Some(value);
                state.failure = None;
                Ok(answer)
            }
            Err(error) if ctx.cancel.is_cancelled() => Err(error),
            Err(error) => {
                attempt.finished = true;
                let failure = format!("Cached perimeters; {}", error.public_message());
                attempt.state.failure = Some(failure);
                match attempt.state.cache.as_ref().filter(|c| Self::usable(c)) {
                    Some(cache) => Ok(Self::answer(cache, attempt.state.failure.as_ref())),
                    None => Err(error),
                }
            }
        }
    }
}

/// Builds the served items and notices from recorded documents (fixture replay): the
/// same parsers, budget, matching and verification as live refreshes, without any
/// network.
///
/// # Errors
/// Fails when the WFIGS document is unusable; a missing or unreadable index or page only
/// leaves the links out.
pub fn from_recorded(
    wfigs_page: &[u8],
    index: Option<&[u8]>,
    page: impl Fn(&str) -> Option<Vec<u8>>,
    inciweb_base: &Url,
    now: Timestamp,
) -> Result<(Vec<FirePerimeter>, Vec<String>), ProviderError> {
    let mut read = WfigsRead::new(wfigs::MAX_SNAPSHOT_VERTICES);
    read.add(parse_wfigs_page(wfigs_page, now, &mut Snapshot::default())?);
    let mut inciweb = InciwebState {
        index: index.and_then(|body| parse_index(body).ok()).map(|(entries, _)| entries),
        ..InciwebState::default()
    };
    let slugs: Vec<String> = inciweb.index.as_deref().map_or_else(Vec::new, |entries| {
        read.perimeters
            .iter()
            .filter_map(|f| find_entry(entries, f))
            .map(|e| e.slug.clone())
            .collect()
    });
    for slug in slugs {
        if let Some(html) = page(&slug) {
            inciweb.remember(slug, parse_incident_page(&html).ok());
        }
    }
    let notices = read.notices();
    Ok((linked_levels(read.perimeters, &inciweb, inciweb_base, now), notices))
}
