//! HTTP client and caching engine for GTFS-Realtime transit feeds.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use jiff::Timestamp;
use lavender_domain::{BBox, TransitFeed, TransitTrail, TransitVehicle};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
    LOCATION, USER_AGENT,
};

use super::decoder::decode_vehicle_positions;
use super::error::TransitError;
use super::history::{TransitHistoryStore, valid_transit_identifier};
use super::redirect::{TRANSIT_MAX_REDIRECTS, transit_redirect_decision};
use super::registry::{default_approved_feeds, valid_transit_feed_id};
use crate::fetch::Fetcher;

/// Freshness TTL for cached transit snapshots (15 seconds).
pub const TRANSIT_TTL_SECS: u64 = 15;

/// Maximum age of a snapshot that can be served stale during an upstream outage (10 minutes).
pub const TRANSIT_STALE_MAX_SECS: u64 = 600;

/// Maximum body size for GTFS-RT feed payloads (8 MiB).
pub const TRANSIT_MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Backoff cooldown ladder in milliseconds: [5s, 15s, 60s, 300s].
pub const TRANSIT_BACKOFF_LADDER_MS: [i64; 4] = [5_000, 15_000, 60_000, 300_000];

/// Maximum requests allowed per feed per minute.
pub const TRANSIT_ADMISSION_MAX_PER_FEED: usize = 8;

/// Maximum requests allowed across all feeds per minute.
pub const TRANSIT_ADMISSION_MAX_GLOBAL: usize = 40;

#[derive(Debug, Clone)]
struct CachedSnapshot {
    fetched_at: Timestamp,
    #[allow(dead_code)]
    contacted_at: Timestamp,
    etag: Option<String>,
    last_modified: Option<String>,
    vehicles: Vec<TransitVehicle>,
}

#[derive(Debug, Clone)]
struct CooldownState {
    failures: usize,
    next_attempt_at_ms: i64,
}

#[derive(Debug)]
struct AdmissionLimiter {
    feed_requests: HashMap<String, Vec<i64>>,
    global_requests: Vec<i64>,
}

impl AdmissionLimiter {
    fn new() -> Self {
        Self { feed_requests: HashMap::new(), global_requests: Vec::new() }
    }

    fn check_and_record(&mut self, feed_id: &str, now_ms: i64) -> Result<(), u64> {
        let window_start = now_ms - 60_000;
        self.global_requests.retain(|&t| t > window_start);
        if self.global_requests.len() >= TRANSIT_ADMISSION_MAX_GLOBAL {
            return Err(TRANSIT_TTL_SECS);
        }

        let feed_entry = self.feed_requests.entry(feed_id.to_string()).or_default();
        feed_entry.retain(|&t| t > window_start);
        if feed_entry.len() >= TRANSIT_ADMISSION_MAX_PER_FEED {
            return Err(TRANSIT_TTL_SECS);
        }

        self.global_requests.push(now_ms);
        feed_entry.push(now_ms);
        Ok(())
    }
}

/// Core engine managing transit feed polling, caching, history tracking, and routes.
#[derive(Debug)]
pub struct TransitEngine {
    fetcher: Arc<Fetcher>,
    history: Arc<TransitHistoryStore>,
    feeds: Vec<TransitFeed>,
    feeds_by_id: HashMap<String, TransitFeed>,
    snapshots: moka::future::Cache<String, CachedSnapshot>,
    cooldown: Arc<RwLock<HashMap<String, CooldownState>>>,
    admission: Arc<RwLock<AdmissionLimiter>>,
}

impl TransitEngine {
    /// Create a new transit engine with configured approved feeds.
    #[must_use]
    pub fn new(fetcher: Arc<Fetcher>, history: Arc<TransitHistoryStore>) -> Self {
        Self::with_feeds(fetcher, history, default_approved_feeds())
    }

    /// Create a new transit engine with explicit approved feeds.
    #[must_use]
    pub fn with_feeds(
        fetcher: Arc<Fetcher>,
        history: Arc<TransitHistoryStore>,
        feeds: Vec<TransitFeed>,
    ) -> Self {
        let feeds_by_id = feeds.iter().map(|f| (f.id.clone(), f.clone())).collect();
        let snapshots = moka::future::Cache::builder()
            .time_to_live(Duration::from_secs(TRANSIT_STALE_MAX_SECS))
            .build();

        Self {
            fetcher,
            history,
            feeds,
            feeds_by_id,
            snapshots,
            cooldown: Arc::new(RwLock::new(HashMap::new())),
            admission: Arc::new(RwLock::new(AdmissionLimiter::new())),
        }
    }

    /// Returns the catalog of approved feeds.
    #[must_use]
    pub fn feeds(&self) -> Vec<TransitFeed> {
        self.feeds.clone()
    }

    /// Access the underlying historical vehicle trail store.
    #[must_use]
    pub fn history(&self) -> &Arc<TransitHistoryStore> {
        &self.history
    }

    /// Retrieve live vehicles for a feed, optionally filtered by viewport bounding box.
    ///
    /// # Errors
    /// Returns [`TransitError::UnknownFeed`] if `feed_id` is not registered,
    /// [`TransitError::RateLimited`] if rate limits are exceeded, or [`TransitError::Network`]
    /// on upstream failure when no stale cache entry is available.
    pub async fn get_vehicles(
        &self,
        feed_id: &str,
        bbox: Option<&BBox>,
    ) -> Result<Vec<TransitVehicle>, TransitError> {
        let feed = self
            .feeds_by_id
            .get(feed_id)
            .ok_or_else(|| TransitError::UnknownFeed(feed_id.to_string()))?;

        let now = Timestamp::now();
        let epoch_seconds = now.as_second();
        #[allow(clippy::cast_possible_truncation)]
        let epoch_millis =
            (epoch_seconds * 1000) + (i64::from(now.subsec_nanosecond()) / 1_000_000);

        let cached = self.snapshots.get(feed_id).await;
        if let Some(entry) = &cached {
            #[allow(clippy::cast_sign_loss)]
            let age_s = (epoch_seconds - entry.fetched_at.as_second()).max(0) as u64;
            if age_s < TRANSIT_TTL_SECS {
                return Ok(filter_bbox(&entry.vehicles, bbox));
            }
        }

        // Check backoff cooldown
        if let Ok(cd) = self.cooldown.read()
            && let Some(state) = cd.get(feed_id)
            && state.next_attempt_at_ms > epoch_millis
        {
            #[allow(clippy::cast_sign_loss)]
            let retry_after_s =
                ((state.next_attempt_at_ms - epoch_millis + 999) / 1000).max(1) as u64;
            if let Some(entry) = cached {
                return Ok(filter_bbox(&entry.vehicles, bbox));
            }
            return Err(TransitError::Cooldown { feed_id: feed_id.to_string(), retry_after_s });
        }

        // Check admission limiter
        if let Ok(mut adm) = self.admission.write()
            && let Err(retry_after_s) = adm.check_and_record(feed_id, epoch_millis)
        {
            if let Some(entry) = cached {
                return Ok(filter_bbox(&entry.vehicles, bbox));
            }
            return Err(TransitError::RateLimited { feed_id: feed_id.to_string(), retry_after_s });
        }

        // Fetch upstream with hop-by-hop redirects and conditional headers
        match self.fetch_feed(feed, cached.as_ref(), now, epoch_seconds, epoch_millis).await {
            Ok(snapshot) => {
                if let Ok(mut cd) = self.cooldown.write() {
                    cd.remove(feed_id);
                }
                self.snapshots.insert(feed_id.to_string(), snapshot.clone()).await;
                Ok(filter_bbox(&snapshot.vehicles, bbox))
            }
            Err(e) => {
                self.record_failure(feed_id, epoch_millis);
                if let Some(entry) = cached {
                    Ok(filter_bbox(&entry.vehicles, bbox))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Retrieve the historical position trail for a vehicle in a feed.
    ///
    /// # Errors
    /// Returns [`TransitError::InvalidIdentifier`] if `vehicle_id` is malformed,
    /// [`TransitError::UnknownFeed`] if `feed_id` is unknown, or [`TransitError::TrailNotFound`]
    /// if no fixes exist.
    pub fn get_trail(&self, feed_id: &str, vehicle_id: &str) -> Result<TransitTrail, TransitError> {
        if !valid_transit_feed_id(feed_id) {
            return Err(TransitError::UnknownFeed(feed_id.to_string()));
        }
        if !valid_transit_identifier(vehicle_id) {
            return Err(TransitError::InvalidIdentifier(vehicle_id.to_string()));
        }
        if !self.feeds_by_id.contains_key(feed_id) {
            return Err(TransitError::UnknownFeed(feed_id.to_string()));
        }

        let now = Timestamp::now();
        let epoch_seconds = now.as_second();
        #[allow(clippy::cast_possible_truncation)]
        let epoch_millis =
            (epoch_seconds * 1000) + (i64::from(now.subsec_nanosecond()) / 1_000_000);

        self.history.get(feed_id, vehicle_id, epoch_millis).ok_or_else(|| {
            TransitError::TrailNotFound {
                feed_id: feed_id.to_string(),
                vehicle_id: vehicle_id.to_string(),
            }
        })
    }

    async fn fetch_feed(
        &self,
        feed: &TransitFeed,
        cached: Option<&CachedSnapshot>,
        now: Timestamp,
        epoch_seconds: i64,
        epoch_millis: i64,
    ) -> Result<CachedSnapshot, TransitError> {
        let client = self.fetcher.client();
        let mut current_url = feed.url.clone();
        let timeout = Duration::from_secs(15);

        for hop in 0..=TRANSIT_MAX_REDIRECTS {
            let mut request = client
                .get(&current_url)
                .timeout(timeout)
                .header(
                    USER_AGENT,
                    "lavender-transit-proxy/1.0 (+https://github.com/bilawalsidhu/gods-eye-view)",
                )
                .header(ACCEPT, "application/x-protobuf, application/octet-stream;q=0.9, */*;q=0.1")
                .header(ACCEPT_ENCODING, "gzip");

            // Feed-specific headers (e.g. Entur)
            if feed.id == "entur-norway" {
                request = request.header("ET-Client-Name", "gods-eye-view-transit");
            }

            // Conditional validators if cached
            if let Some(entry) = cached {
                if let Some(etag) = &entry.etag {
                    request = request.header(IF_NONE_MATCH, etag);
                }
                if let Some(lm) = &entry.last_modified {
                    request = request.header(IF_MODIFIED_SINCE, lm);
                }
            }

            let response =
                request.send().await.map_err(|e| TransitError::Network(e.to_string()))?;

            let status = response.status();

            // 304 Not Modified: Reuse cached snapshot, update contacted_at
            if status == StatusCode::NOT_MODIFIED
                && let Some(entry) = cached
            {
                return Ok(CachedSnapshot {
                    fetched_at: now,
                    contacted_at: now,
                    etag: entry.etag.clone(),
                    last_modified: entry.last_modified.clone(),
                    vehicles: entry.vehicles.clone(),
                });
            }

            // Manual redirect handling
            if status.is_redirection() {
                if hop == TRANSIT_MAX_REDIRECTS {
                    return Err(TransitError::TooManyRedirects);
                }
                let loc = response.headers().get(LOCATION).and_then(|v| v.to_str().ok());
                let next_url = transit_redirect_decision(&feed.url, &current_url, loc)?;
                current_url = next_url;
                continue;
            }

            if !status.is_success() {
                return Err(TransitError::Network(format!("upstream HTTP {status}")));
            }

            if let Some(len_header) = response.headers().get(CONTENT_LENGTH)
                && let Ok(declared) = len_header.to_str().unwrap_or("0").parse::<usize>()
                && declared > TRANSIT_MAX_BODY_BYTES
            {
                return Err(TransitError::BodyTooLarge { limit: TRANSIT_MAX_BODY_BYTES });
            }

            let etag =
                response.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_string);
            let last_modified = response
                .headers()
                .get(LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);

            let bytes = response.bytes().await.map_err(|e| TransitError::Network(e.to_string()))?;

            if bytes.len() > TRANSIT_MAX_BODY_BYTES {
                return Err(TransitError::BodyTooLarge { limit: TRANSIT_MAX_BODY_BYTES });
            }

            let snapshot = decode_vehicle_positions(&bytes, feed, epoch_seconds)?;
            self.history.ingest(feed, &snapshot.vehicles, epoch_millis);

            return Ok(CachedSnapshot {
                fetched_at: now,
                contacted_at: now,
                etag,
                last_modified,
                vehicles: snapshot.vehicles,
            });
        }

        Err(TransitError::TooManyRedirects)
    }

    fn record_failure(&self, feed_id: &str, now_ms: i64) {
        if let Ok(mut cd) = self.cooldown.write() {
            let entry = cd
                .entry(feed_id.to_string())
                .or_insert(CooldownState { failures: 0, next_attempt_at_ms: 0 });
            entry.failures += 1;
            let index = entry.failures.saturating_sub(1).min(TRANSIT_BACKOFF_LADDER_MS.len() - 1);
            let wait_ms = TRANSIT_BACKOFF_LADDER_MS[index];
            entry.next_attempt_at_ms = now_ms + wait_ms;
        }
    }
}

fn filter_bbox(vehicles: &[TransitVehicle], bbox: Option<&BBox>) -> Vec<TransitVehicle> {
    match bbox {
        None => vehicles.to_vec(),
        Some(b) => vehicles.iter().filter(|v| b.contains(&v.position)).cloned().collect(),
    }
}
