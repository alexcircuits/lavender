//! Client engine and caching layer for bike-share GBFS providers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use lavender_domain::{BBox, BikeshareStation, BikeshareSystem};
use moka::future::Cache;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, warn};

use super::parser::{
    DiscoveredFeeds, merge_stations, parse_gbfs_discovery, parse_station_information,
    parse_station_status,
};
use crate::fetch::{FetchError, Fetcher};

/// Maximum response body for GBFS feeds (5 MiB).
pub const GBFS_MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

/// Upstream request timeout (12s).
pub const GBFS_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// Default cache TTL for stations (60s).
pub const STATIONS_CACHE_TTL: Duration = Duration::from_secs(60);

/// Default cache TTL for discovery feeds (300s).
pub const DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(300);

/// Errors encountered during GBFS bike-share processing.
#[derive(Debug, Error)]
pub enum BikeshareError {
    #[error("unknown bike-share system '{0}'")]
    NotFound(String),

    #[error("arbitrary URL parameters are not accepted; use a registered system ID")]
    UrlParameterNotAllowed,

    #[error("failed to fetch upstream GBFS feed: {0}")]
    Upstream(#[from] FetchError),

    #[error("failed to parse JSON response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("discovery feed missing station endpoints")]
    MissingEndpoints,
}

/// The bike-share engine managing registries, caching, and upstream calls.
#[derive(Debug, Clone)]
pub struct BikeshareEngine {
    fetcher: Arc<Fetcher>,
    systems: Arc<HashMap<String, BikeshareSystem>>,
    discovery_cache: Cache<String, DiscoveredFeeds>,
    stations_cache: Cache<String, Arc<Vec<BikeshareStation>>>,
}

impl BikeshareEngine {
    /// Create a new bike-share engine with the provided fetcher and systems.
    #[must_use]
    pub fn new(fetcher: Arc<Fetcher>, systems_list: Vec<BikeshareSystem>) -> Self {
        let mut map = HashMap::with_capacity(systems_list.len());
        for sys in systems_list {
            map.insert(sys.id.clone(), sys);
        }

        let discovery_cache =
            Cache::builder().time_to_live(DISCOVERY_CACHE_TTL).max_capacity(128).build();

        let stations_cache =
            Cache::builder().time_to_live(STATIONS_CACHE_TTL).max_capacity(128).build();

        Self { fetcher, systems: Arc::new(map), discovery_cache, stations_cache }
    }

    /// List all configured bike-share systems.
    #[must_use]
    pub fn systems(&self) -> Vec<BikeshareSystem> {
        let mut list: Vec<_> = self.systems.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Look up a bike-share system by identifier.
    #[must_use]
    pub fn find_system(&self, system_id: &str) -> Option<&BikeshareSystem> {
        self.systems.get(system_id)
    }

    /// Reject any attempt to supply a URL from the client.
    ///
    /// # Errors
    /// Returns `BikeshareError::UrlParameterNotAllowed` if `raw` looks like a URL or query param.
    pub fn validate_no_arbitrary_url(raw: &str) -> Result<(), BikeshareError> {
        let s = raw.trim().to_ascii_lowercase();
        if s.contains("http://")
            || s.contains("https://")
            || s.contains("url=")
            || s.contains("%3a%2f%2f")
            || s.contains("://")
        {
            return Err(BikeshareError::UrlParameterNotAllowed);
        }
        Ok(())
    }

    /// Fetch and return stations for a registered bike-share system, optionally filtered by `bbox`.
    ///
    /// # Errors
    /// Returns `BikeshareError::NotFound` if `system_id` is not registered, or upstream errors.
    pub async fn get_stations(
        &self,
        system_id: &str,
        bbox: Option<&BBox>,
    ) -> Result<Vec<BikeshareStation>, BikeshareError> {
        Self::validate_no_arbitrary_url(system_id)?;

        let system = self
            .find_system(system_id)
            .ok_or_else(|| BikeshareError::NotFound(system_id.to_string()))?;

        // Check station cache
        let all_stations = if let Some(cached) = self.stations_cache.get(system_id).await {
            cached
        } else {
            let fetched = self.fetch_system_stations(system).await?;
            let arc = Arc::new(fetched);
            self.stations_cache.insert(system_id.to_string(), Arc::clone(&arc)).await;
            arc
        };

        // Filter by bbox if requested
        let result = if let Some(box_filter) = bbox {
            all_stations.iter().filter(|s| box_filter.contains(&s.position)).cloned().collect()
        } else {
            (*all_stations).clone()
        };

        Ok(result)
    }

    async fn fetch_json(&self, url: &str) -> Result<Value, BikeshareError> {
        debug!(url, "fetching GBFS JSON payload");
        let req = self
            .fetcher
            .client()
            .get(url)
            .header(reqwest::header::USER_AGENT, "Lavender/0.1.0 (bikeshare)")
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(GBFS_REQUEST_TIMEOUT);

        let res = req.send().await.map_err(|e| crate::fetch::FetchError::Network {
            host: url.to_string(),
            reason: e.to_string(),
        })?;

        if !res.status().is_success() {
            return Err(BikeshareError::NotFound(format!(
                "upstream returned status {}",
                res.status()
            )));
        }

        let mut stream = res.bytes_stream();
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk_res) = futures::StreamExt::next(&mut stream).await {
            let chunk = chunk_res.map_err(|e| crate::fetch::FetchError::Network {
                host: url.to_string(),
                reason: e.to_string(),
            })?;
            if buf.len() + chunk.len() > GBFS_MAX_BODY_BYTES {
                return Err(BikeshareError::Upstream(crate::fetch::FetchError::BodyTooLarge {
                    host: url.to_string(),
                    limit: GBFS_MAX_BODY_BYTES,
                }));
            }
            buf.extend_from_slice(&chunk);
        }

        let json: Value = serde_json::from_slice(&buf)?;
        Ok(json)
    }

    async fn discover_feeds(
        &self,
        system: &BikeshareSystem,
    ) -> Result<DiscoveredFeeds, BikeshareError> {
        if let Some(cached) = self.discovery_cache.get(&system.id).await {
            return Ok(cached);
        }

        // Fetch gbfs.json discovery
        let discovery_json = self.fetch_json(&system.gbfs_url).await?;
        let discovered = parse_gbfs_discovery(&discovery_json);

        // Fallback endpoint inference if discovery index lacks direct links
        let info_url = discovered.station_information_url.clone().or_else(|| {
            let base = system.gbfs_url.trim_end_matches("gbfs.json").trim_end_matches('/');
            Some(format!("{base}/station_information.json"))
        });

        let status_url = discovered.station_status_url.clone().or_else(|| {
            let base = system.gbfs_url.trim_end_matches("gbfs.json").trim_end_matches('/');
            Some(format!("{base}/station_status.json"))
        });

        let resolved = DiscoveredFeeds {
            station_information_url: info_url,
            station_status_url: status_url,
            ttl: discovered.ttl,
        };

        self.discovery_cache.insert(system.id.clone(), resolved.clone()).await;

        Ok(resolved)
    }

    async fn fetch_system_stations(
        &self,
        system: &BikeshareSystem,
    ) -> Result<Vec<BikeshareStation>, BikeshareError> {
        let feeds = self.discover_feeds(system).await?;

        let info_url = feeds.station_information_url.ok_or(BikeshareError::MissingEndpoints)?;
        let status_url = feeds.station_status_url.ok_or(BikeshareError::MissingEndpoints)?;

        // Fetch station information and status concurrently
        let (info_res, status_res) =
            tokio::join!(self.fetch_json(&info_url), self.fetch_json(&status_url));

        let info_json = info_res?;
        let status_json = status_res.unwrap_or_else(|e| {
            warn!(error = %e, system = %system.id, "station_status fetch failed; rendering static info");
            Value::Null
        });

        let info_map = parse_station_information(&info_json);
        let status_map = parse_station_status(&status_json);

        let stations = merge_stations(&system.id, &info_map, &status_map);
        Ok(stations)
    }
}
