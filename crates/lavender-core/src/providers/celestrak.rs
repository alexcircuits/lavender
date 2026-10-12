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

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use lavender_domain::text::{clean, clean_opt};
use lavender_domain::{FeedId, FeedItems, ProviderScope, SatelliteElements};
use serde::Deserialize;
use url::Url;

use super::{FeedProvider, FetchContext, Fetched, ProviderError, Region, lenient, parse_rows};
use crate::config::CelestrakConfig;
use crate::fetch::{FetchError, FetchOptions, HostPolicy};
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

/// A group's elements and when they were downloaded.
#[derive(Debug, Clone)]
struct GroupData {
    elements: Vec<SatelliteElements>,
    dropped: usize,
    as_of: Timestamp,
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
        let mut url = self
            .config
            .base_url
            .join("gp.php")
            .map_err(|e| ProviderError::Unconfigured(format!("CelesTrak URL: {e}")))?;
        url.query_pairs_mut().append_pair("GROUP", group).append_pair("FORMAT", "json");
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
    async fn cached(&self, dir: Option<&Path>, group: &str) -> Option<GroupData> {
        let in_memory = {
            let memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
            memory.get(group).cloned()
        };
        let on_disk = read_cache(dir, group).await;
        match (in_memory, on_disk) {
            (Some(m), Some(d)) => Some(if d.as_of > m.as_of { d } else { m }),
            (m, d) => m.or(d),
        }
    }

    async fn load_group(&self, ctx: &FetchContext<'_>, group: &str) -> GroupOutcome {
        let cached = self.cached(ctx.cache_dir, group).await;
        let now = Timestamp::now();
        if let Some(data) = &cached
            && age(now, data.as_of) <= self.cadence_duration()
        {
            return GroupOutcome::Fresh(data.clone());
        }
        if let Some(wait) = self.throttled_for(group) {
            let reason = format!("next request allowed in {}", human(wait));
            return stale_or_missing(cached, reason);
        }
        let url = match self.group_url(group) {
            Ok(url) => url,
            Err(e) => return stale_or_missing(cached, e.public_message()),
        };
        // One request per group per attempt: the throttle above, not fetcher retries,
        // decides when CelesTrak is asked again.
        let opts = FetchOptions {
            max_body_bytes: Some(MAX_BODY),
            cancel: Some(ctx.cancel.clone()),
            retries: Some(0),
            ..FetchOptions::default()
        };
        let previous = self.record_attempt(group, false);
        let reason = match ctx.fetcher.get(&url, &self.policy, &opts).await {
            Ok(response) => match parse(&response.body, group) {
                Ok((elements, dropped)) => {
                    self.record_attempt(group, true);
                    store(ctx.cache_dir, group, &response.body).await;
                    let data = GroupData { elements, dropped, as_of: Timestamp::now() };
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
fn human(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..120 => format!("{s} s"),
        120..7_200 => format!("{} min", s / 60),
        7_200..172_800 => format!("{} h", s / 3_600),
        _ => format!("{} days", s / 86_400),
    }
}

// Every field is lenient: a wrong type reads as missing, and `normalise` drops the element
// set when a required field is missing (for example `"BSTAR": null`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Omm {
    #[serde(default, deserialize_with = "lenient")]
    object_name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    object_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    epoch: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    eccentricity: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    inclination: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    ra_of_asc_node: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    arg_of_pericenter: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_anomaly: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    classification_type: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    norad_cat_id: Option<u32>,
    #[serde(default, deserialize_with = "lenient")]
    rev_at_epoch: Option<u32>,
    #[serde(default, deserialize_with = "lenient")]
    bstar: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion_dot: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    mean_motion_ddot: Option<f64>,
}

fn parse_epoch(s: &str) -> Option<Timestamp> {
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts);
    }
    s.parse::<DateTime>().ok()?.to_zoned(TimeZone::UTC).ok().map(|z| z.timestamp())
}

fn normalise(o: &Omm, group: &str) -> Option<SatelliteElements> {
    let finite = |v: Option<f64>| v.filter(|x| x.is_finite());
    let mean_motion = finite(o.mean_motion).filter(|m| *m > 0.0)?;
    let eccentricity = finite(o.eccentricity).filter(|e| (0.0..1.0).contains(e))?;
    let inclination = finite(o.inclination).filter(|i| (0.0..=180.0).contains(i))?;
    let norad_id = o.norad_cat_id.filter(|n| *n != 0)?;
    Some(SatelliteElements {
        norad_id,
        name: clean(o.object_name.as_deref().unwrap_or("Unnamed object"), 48),
        intl_designator: clean_opt(o.object_id.as_deref(), 16),
        group: group.to_owned(),
        epoch: parse_epoch(o.epoch.as_deref()?)?,
        mean_motion,
        eccentricity,
        inclination_deg: inclination,
        raan_deg: finite(o.ra_of_asc_node)?,
        arg_of_pericenter_deg: finite(o.arg_of_pericenter)?,
        mean_anomaly_deg: finite(o.mean_anomaly)?,
        bstar: finite(o.bstar)?,
        mean_motion_dot: finite(o.mean_motion_dot)?,
        mean_motion_ddot: finite(o.mean_motion_ddot)?,
        rev_at_epoch: o.rev_at_epoch,
        classification: clean_opt(o.classification_type.as_deref(), 1),
    })
}

/// Parses one group's OMM JSON array. Invalid element sets are skipped; returns the valid
/// ones and how many were skipped.
///
/// # Errors
/// Returns an error when the document is not a JSON array (CelesTrak answers plain text
/// for unknown groups and rate-limit refusals).
pub fn parse(body: &[u8], group: &str) -> Result<(Vec<SatelliteElements>, usize), ProviderError> {
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(body).map_err(|e| ProviderError::parse("CelesTrak", &e))?;
    let total = rows.len();
    let (records, _) = parse_rows::<Omm>(rows);
    let elements: Vec<SatelliteElements> =
        records.iter().filter_map(|o| normalise(o, group)).collect();
    let dropped = total - elements.len();
    Ok((elements, dropped))
}

fn cache_path(dir: &Path, group: &str) -> PathBuf {
    dir.join("celestrak").join(format!("{group}.json"))
}

/// The cached document for `group` with its modification time as its age.
async fn read_cache(dir: Option<&Path>, group: &str) -> Option<GroupData> {
    let path = cache_path(dir?, group);
    let meta = tokio::fs::metadata(&path).await.ok()?;
    let modified = meta.modified().ok().unwrap_or(SystemTime::UNIX_EPOCH);
    let as_of = Timestamp::try_from(modified).unwrap_or(Timestamp::UNIX_EPOCH);
    let body = tokio::fs::read(&path).await.ok()?;
    let (elements, dropped) = parse(&body, group).ok()?;
    Some(GroupData { elements, dropped, as_of })
}

async fn store(dir: Option<&Path>, group: &str, body: &[u8]) {
    let Some(dir) = dir else { return };
    let path = cache_path(dir, group);
    let tmp = path.with_extension("json.tmp");
    let result = async {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&tmp, body).await?;
        tokio::fs::rename(&tmp, &path).await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(group, error = %e, "could not cache CelesTrak group");
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
        let _ = write!(reason, " Showing elements downloaded {} ago.", human(age));
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
                    let stale_for = age(now, data.as_of);
                    oldest_stale = Some(oldest_stale.map_or(stale_for, |o| o.max(stale_for)));
                    data
                }
                GroupOutcome::Missing(reason) => {
                    problems.push((group.clone(), reason));
                    continue;
                }
            };
            as_of = Some(as_of.map_or(data.as_of, |t| t.min(data.as_of)));
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
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"OBJECT_NAME":"TEST STATION","OBJECT_ID":"1998-067A","EPOCH":"2026-10-10T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0007,"INCLINATION":51.64,"RA_OF_ASC_NODE":247.46,"ARG_OF_PERICENTER":130.53,"MEAN_ANOMALY":325.03,"EPHEMERIS_TYPE":0,"CLASSIFICATION_TYPE":"U","NORAD_CAT_ID":25544,"ELEMENT_SET_NO":999,"REV_AT_EPOCH":44393,"BSTAR":0.0003,"MEAN_MOTION_DOT":0.00016,"MEAN_MOTION_DDOT":0},
      {"OBJECT_NAME":"BROKEN","EPOCH":"2026-10-10T12:00:00","MEAN_MOTION":-1,"ECCENTRICITY":0.1,"INCLINATION":10,"RA_OF_ASC_NODE":0,"ARG_OF_PERICENTER":0,"MEAN_ANOMALY":0,"NORAD_CAT_ID":2,"BSTAR":0,"MEAN_MOTION_DOT":0,"MEAN_MOTION_DDOT":0}
    ]"#;

    #[test]
    fn parses_omm_and_drops_invalid_sets() {
        let (sats, dropped) = parse(SAMPLE.as_bytes(), "stations").unwrap();
        assert_eq!((sats.len(), dropped), (1, 1));
        let s = &sats[0];
        assert_eq!(s.norad_id, 25544);
        assert_eq!(s.group, "stations");
        assert_eq!(s.epoch.to_string(), "2026-10-10T12:00:00Z");
        lavender_geo::Propagator::new(s).unwrap();
    }

    #[test]
    fn refuses_plain_text_answers() {
        assert!(parse(b"Invalid query: GROUP", "x").is_err());
    }

    #[test]
    fn a_null_field_drops_only_its_element_set() {
        // Review finding 10: `"BSTAR": null` used to reject the whole group.
        let body = SAMPLE.replacen(
            r"[
      {",
            r#"[
      {"OBJECT_NAME":"NO DRAG","EPOCH":"2026-10-10T12:00:00","MEAN_MOTION":15.0,"ECCENTRICITY":0.001,"INCLINATION":50,"RA_OF_ASC_NODE":0,"ARG_OF_PERICENTER":0,"MEAN_ANOMALY":0,"NORAD_CAT_ID":3,"BSTAR":null,"MEAN_MOTION_DOT":0,"MEAN_MOTION_DDOT":0},
      "garbage",
      {"#,
            1,
        );
        let (sats, dropped) = parse(body.as_bytes(), "stations").unwrap();
        assert_eq!(sats.iter().map(|s| s.norad_id).collect::<Vec<_>>(), vec![25544]);
        assert_eq!(dropped, 3);
    }

    #[test]
    fn builds_group_urls() {
        let p = CelestrakProvider::new(CelestrakConfig::default());
        assert_eq!(
            p.group_url("gps-ops").unwrap().as_str(),
            "https://celestrak.org/NORAD/elements/gp.php?GROUP=gps-ops&FORMAT=json"
        );
    }

    #[test]
    fn failed_groups_wait_a_twelfth_of_the_cadence_and_downloads_a_full_cadence() {
        let p = CelestrakProvider::new(CelestrakConfig::default());
        assert_eq!(p.retry_floor(), Duration::from_secs(600));
        assert!(p.throttled_for("stations").is_none());
        p.record_attempt("stations", false);
        let wait = p.throttled_for("stations").unwrap();
        assert!(wait > Duration::from_secs(590) && wait <= Duration::from_secs(600), "{wait:?}");
        p.record_attempt("stations", true);
        assert!(p.throttled_for("stations").unwrap() > Duration::from_secs(7_000));
    }

    #[test]
    fn readable_ages() {
        assert_eq!(human(Duration::from_secs(45)), "45 s");
        assert_eq!(human(Duration::from_secs(600)), "10 min");
        assert_eq!(human(Duration::from_hours(5)), "5 h");
        assert_eq!(human(Duration::from_hours(72)), "3 days");
    }
}
