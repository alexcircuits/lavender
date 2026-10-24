//! Re:Earth terrain point-heights upstream client and resolver engine.

use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use lavender_domain::{TerrainHeightsResponse, TerrainPointResult};
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use serde::Deserialize;
use tokio::time::sleep;

use super::cache::TerrainLruCache;
use super::coords::{canonical_key, canonical_point, validate_coordinates};
use super::error::TerrainError;
use crate::config::TerrainConfig;
use crate::fetch::Fetcher;

/// Default maximum points per request.
pub const TERRAIN_DEFAULT_MAX_POINTS: usize = 2000;
/// Default points per upstream Re:Earth chunk request.
pub const TERRAIN_DEFAULT_CHUNK_SIZE: usize = 64;
/// Default LRU cache capacity in number of point entries.
pub const TERRAIN_DEFAULT_CACHE_CAPACITY: usize = 50_000;
/// Default cache time-to-live in days (30 days).
pub const TERRAIN_DEFAULT_TTL_DAYS: u64 = 30;

/// Maximum time retries may add after the first attempt settles.
pub const TERRAIN_RETRY_BUDGET_MS: u64 = 10_000;
/// One initial attempt plus up to three bounded retries.
pub const TERRAIN_MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
struct UpstreamResponse {
    results: Option<Vec<UpstreamPointRow>>,
}

#[derive(Debug, Deserialize)]
struct UpstreamPointRow {
    id: Option<String>,
    ellipsoid: Option<f64>,
    elevation: Option<f64>,
    geoid: Option<f64>,
}

impl UpstreamPointRow {
    /// Transient absent datum: Re:Earth returned a row with elevation but null ellipsoid/geoid.
    fn is_transient_absent(&self) -> bool {
        self.ellipsoid.is_none()
            && self.geoid.is_none()
            && self.elevation.is_some_and(f64::is_finite)
    }

    fn to_result(&self) -> Option<TerrainPointResult> {
        self.ellipsoid.filter(|e| e.is_finite()).map(|ellipsoid| TerrainPointResult {
            id: self.id.clone(),
            ellipsoid,
            elevation: self.elevation.filter(|e| e.is_finite()),
            geoid: self.geoid.filter(|g| g.is_finite()),
        })
    }
}

/// Core engine managing cached terrain-height lookups and upstream Re:Earth queries.
#[derive(Debug)]
pub struct TerrainEngine {
    config: TerrainConfig,
    cache: Arc<TerrainLruCache>,
    fetcher: Arc<Fetcher>,
}

impl TerrainEngine {
    /// Create a new `TerrainEngine` with given configuration and shared fetcher.
    #[must_use]
    pub fn new(config: TerrainConfig, fetcher: Arc<Fetcher>) -> Self {
        let cache = Arc::new(TerrainLruCache::new(config.cache_capacity));
        Self { config, cache, fetcher }
    }

    /// Access the underlying LRU cache for metrics or testing.
    #[must_use]
    pub fn cache(&self) -> &Arc<TerrainLruCache> {
        &self.cache
    }

    /// Resolve ground heights for a slice of `[longitude, latitude]` points.
    ///
    /// Preserves exact input order and duplicates, querying upstream for unique missing points.
    ///
    /// # Errors
    /// Returns [`TerrainError::EmptyPoints`] if `points` is empty,
    /// [`TerrainError::TooManyPoints`] if points exceeds capacity,
    /// [`TerrainError::InvalidCoordinate`] if coordinates are out of range,
    /// or [`TerrainError::MissingHeight`] if upstream fails and no cached datum exists.
    pub async fn resolve_heights(
        &self,
        points: &[[f64; 2]],
    ) -> Result<TerrainHeightsResponse, TerrainError> {
        if !self.config.enabled {
            return Err(TerrainError::Disabled);
        }

        validate_coordinates(points, self.config.max_points)?;

        let now = Timestamp::now();
        let epoch_seconds = now.as_second();
        #[allow(clippy::cast_possible_truncation)]
        let epoch_millis =
            (epoch_seconds * 1000) + (i64::from(now.subsec_nanosecond()) / 1_000_000);
        let ttl_ms =
            i64::try_from(self.config.cache_ttl_days * 24 * 3600 * 1000).unwrap_or(i64::MAX);

        // Map inputs to canonical keys
        let canonical_keys: Vec<String> = points.iter().copied().map(canonical_key).collect();

        // Collect unique missing points
        let mut unique_missing = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();

        for (i, key) in canonical_keys.iter().enumerate() {
            if seen_keys.insert(key.clone()) {
                let cached_entry = self.cache.get(key, epoch_millis, ttl_ms);
                // If not cached or stale, add to missing list
                if cached_entry.is_none() || cached_entry.as_ref().is_some_and(|(_, stale)| *stale)
                {
                    unique_missing.push((key.clone(), canonical_point(points[i])));
                }
            }
        }

        // Fetch missing points in chunks
        if !unique_missing.is_empty() {
            let chunk_size = self.config.chunk_size.max(1);
            for chunk in unique_missing.chunks(chunk_size) {
                let _ = self.fetch_chunk_with_retry(chunk, epoch_millis).await;
            }
        }

        // Reconstruct results in exact input order
        let mut results = Vec::with_capacity(points.len());
        let mut missing_count = 0;

        for key in &canonical_keys {
            if let Some((res, _)) = self.cache.get(key, epoch_millis, ttl_ms) {
                results.push(res);
            } else {
                missing_count += 1;
            }
        }

        if missing_count > 0 {
            return Err(TerrainError::MissingHeight { missing_count });
        }

        Ok(TerrainHeightsResponse { results })
    }

    async fn fetch_chunk_with_retry(
        &self,
        chunk: &[(String, [f64; 2])],
        now_ms: i64,
    ) -> Result<(), TerrainError> {
        let points_param: Vec<String> = chunk.iter().map(|(_, pt)| canonical_key(*pt)).collect();
        let query_str = points_param.join(";");
        let encoded: String = url::form_urlencoded::byte_serialize(query_str.as_bytes()).collect();
        let url = format!("{}?points={}", self.config.upstream_url, encoded);

        let client = self.fetcher.client();
        let mut retry_started_at: Option<i64> = None;

        for attempt in 0..TERRAIN_MAX_ATTEMPTS {
            let mut timeout = Duration::from_secs(30);
            if attempt > 0 {
                let elapsed = now_ms_clock() - retry_started_at.unwrap_or(now_ms_clock());
                let elapsed_u64 = u64::try_from(elapsed.max(0)).unwrap_or(0);
                let remaining_ms = TERRAIN_RETRY_BUDGET_MS.saturating_sub(elapsed_u64);
                if remaining_ms == 0 {
                    break;
                }
                timeout = Duration::from_millis(remaining_ms.min(30_000));
            }

            let request = client
                .get(&url)
                .timeout(timeout)
                .header(
                    USER_AGENT,
                    "lavender-terrain-proxy/1.0 (+https://github.com/bilawalsidhu/gods-eye-view)",
                )
                .header(ACCEPT, "application/json");

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body_bytes = response
                            .bytes()
                            .await
                            .map_err(|e| TerrainError::Network(e.to_string()))?;
                        let parsed: UpstreamResponse = serde_json::from_slice(&body_bytes)
                            .map_err(|e| {
                                TerrainError::Network(format!("malformed upstream response: {e}"))
                            })?;

                        let rows = parsed.results.ok_or_else(|| {
                            TerrainError::Network(
                                "malformed upstream response (no results array)".to_string(),
                            )
                        })?;

                        // Ingest valid points; omit transient nulls
                        for (i, row) in rows.iter().enumerate() {
                            if i < chunk.len() {
                                if let Some(res) = row.to_result() {
                                    self.cache.insert(chunk[i].0.clone(), res, now_ms);
                                } else if row.is_transient_absent() {
                                    // Deliberately left uncached
                                    tracing::debug!(
                                        key = %chunk[i].0,
                                        "transient absent terrain height from upstream; will retry next poll"
                                    );
                                }
                            }
                        }
                        return Ok(());
                    }

                    let is_retryable =
                        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                    if !is_retryable || attempt >= TERRAIN_MAX_ATTEMPTS - 1 {
                        return Err(TerrainError::Upstream {
                            status: status.as_u16(),
                            message: format!("HTTP {status}"),
                        });
                    }

                    if retry_started_at.is_none() {
                        retry_started_at = Some(now_ms_clock());
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let delay = calculate_backoff(attempt, retry_after);
                    sleep(delay).await;
                }
                Err(err) => {
                    if attempt >= TERRAIN_MAX_ATTEMPTS - 1 {
                        return Err(TerrainError::Network(err.to_string()));
                    }
                    if retry_started_at.is_none() {
                        retry_started_at = Some(now_ms_clock());
                    }
                    let delay = calculate_backoff(attempt, None);
                    sleep(delay).await;
                }
            }
        }

        Err(TerrainError::MissingHeight { missing_count: chunk.len() })
    }
}

#[allow(clippy::cast_possible_truncation)]
fn now_ms_clock() -> i64 {
    let now = Timestamp::now();
    let now_s = now.as_second();
    (now_s * 1000) + (i64::from(now.subsec_nanosecond()) / 1_000_000)
}

fn parse_retry_after(headers: &HeaderMap<HeaderValue>) -> Option<Duration> {
    let header_val = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = header_val.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    None
}

fn calculate_backoff(attempt: usize, retry_after: Option<Duration>) -> Duration {
    if let Some(ra) = retry_after {
        return ra;
    }
    let base_ms = 350 * (1 << attempt);
    let jitter_ratio = 0.75 + (fastrand::f64() * 0.5);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let delay_ms = (f64::from(base_ms) * jitter_ratio) as u64;
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_engine(upstream_url: String) -> TerrainEngine {
        let config = TerrainConfig {
            enabled: true,
            upstream_url,
            max_points: 100,
            chunk_size: 2,
            cache_capacity: 50,
            cache_ttl_days: 30,
        };
        let fetch_config = crate::config::FetchConfig {
            allow_http_hosts: vec!["127.0.0.1".into(), "localhost".into()],
            ..Default::default()
        };
        let fetcher = Arc::new(Fetcher::new(fetch_config).unwrap());
        TerrainEngine::new(config, fetcher)
    }

    #[tokio::test]
    async fn preserves_exact_order_and_handles_duplicates() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/heights.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    { "id": "p1", "ellipsoid": 100.0, "elevation": 90.0, "geoid": 10.0 },
                    { "id": "p2", "ellipsoid": 200.0, "elevation": 180.0, "geoid": 20.0 }
                ]
            })))
            .mount(&server)
            .await;

        let engine = test_engine(format!("{}/heights.json", server.uri()));

        // Points with duplicates: [P1, P2, P1, P2]
        let points = vec![[10.0, 20.0], [30.0, 40.0], [10.0, 20.0], [30.0, 40.0]];

        let resp = engine.resolve_heights(&points).await.unwrap();
        assert_eq!(resp.results.len(), 4);
        assert_eq!(resp.results[0].ellipsoid, 100.0);
        assert_eq!(resp.results[1].ellipsoid, 200.0);
        assert_eq!(resp.results[2].ellipsoid, 100.0);
        assert_eq!(resp.results[3].ellipsoid, 200.0);
    }

    #[tokio::test]
    async fn stale_cached_entries_serve_on_upstream_failure() {
        let server = MockServer::start().await;

        // Upstream always returns 503
        Mock::given(method("GET"))
            .and(path("/heights.json"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let engine = test_engine(format!("{}/heights.json", server.uri()));

        // Prime cache with stale entry
        let key = canonical_key([10.0, 20.0]);
        engine.cache().insert(
            key,
            TerrainPointResult {
                id: Some("cached-stale".into()),
                ellipsoid: 123.45,
                elevation: None,
                geoid: None,
            },
            1_000, // Very old timestamp -> stale
        );

        let resp = engine.resolve_heights(&[[10.0, 20.0]]).await.unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].id.as_deref(), Some("cached-stale"));
        assert_eq!(resp.results[0].ellipsoid, 123.45);
    }

    #[tokio::test]
    async fn transient_absent_points_are_not_cached_and_return_502() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/heights.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    { "elevation": 450.0, "ellipsoid": null, "geoid": null }
                ]
            })))
            .mount(&server)
            .await;

        let engine = test_engine(format!("{}/heights.json", server.uri()));

        let err = engine.resolve_heights(&[[10.0, 20.0]]).await.unwrap_err();
        assert_eq!(err, TerrainError::MissingHeight { missing_count: 1 });

        // Ensure it was NOT cached
        let key = canonical_key([10.0, 20.0]);
        assert!(engine.cache().get(&key, 1_000_000, 100_000).is_none());
    }
}
