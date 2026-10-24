//! Traffic provider engine coordinating TomTom flow tiles, OpenFreeMap roads,
//! daily budget governance, disk cache sweeping, and geometric matching.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::collapsible_if,
    clippy::duration_suboptimal_units
)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lavender_domain::{TrafficStatus, TrafficTileResponse};

use crate::config::TrafficConfig;
use crate::fetch::Fetcher;
use crate::secrets::{SecretKey, SecretStore};

use super::budget::BudgetGovernor;
use super::cache::sweep_disk_cache;
use super::error::TrafficError;
use super::matcher::match_flow_to_roads;
use super::mvt::{decode_openfreemap_roads, decode_tomtom_flow_tile};
use super::tiles::{MAX_TILE_ZOOM, MIN_TILE_ZOOM, is_valid_tile_coord};

/// Time-to-live for cached TomTom flow tiles (120 seconds).
pub const TILE_TTL: Duration = Duration::from_secs(120);

/// Cache hit/miss/stale status for flow tile responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// Fresh in-memory or disk cache hit.
    Hit,
    /// Upstream network fetch (freshly cached).
    Miss,
    /// Over daily budget; serving stale cached data.
    StaleBudget,
    /// Upstream fetch failed; serving stale cached data as fallback.
    StaleError,
}

impl CacheStatus {
    /// Value for the `x-tomtom-cache` response header.
    #[must_use]
    pub const fn as_header_str(self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::Miss => "MISS",
            Self::StaleBudget => "STALE-BUDGET",
            Self::StaleError => "STALE-ERROR",
        }
    }
}

#[derive(Debug, Clone)]
struct MemEntry {
    bytes: Vec<u8>,
    fetched_at: Instant,
}

/// The traffic engine manages daily request budget, caching, upstream proxying, and matching.
#[derive(Debug)]
pub struct TrafficEngine {
    fetcher: Arc<Fetcher>,
    secrets: Arc<dyn SecretStore>,
    config: TrafficConfig,
    budget: Mutex<BudgetGovernor>,
    mem_flow: Mutex<HashMap<String, MemEntry>>,
    mem_roads: Mutex<HashMap<String, Vec<u8>>>,
}

impl TrafficEngine {
    /// Create a new `TrafficEngine`.
    #[must_use]
    pub fn new(
        fetcher: Arc<Fetcher>,
        secrets: Arc<dyn SecretStore>,
        config: TrafficConfig,
    ) -> Self {
        let budget_file = config.cache_dir.join("budget.json");
        let now_s =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        let budget = BudgetGovernor::load_or_init(budget_file, now_s);

        Self {
            fetcher,
            secrets,
            config,
            budget: Mutex::new(budget),
            mem_flow: Mutex::new(HashMap::new()),
            mem_roads: Mutex::new(HashMap::new()),
        }
    }

    /// Whether an upstream TomTom API key is configured.
    #[must_use]
    pub fn has_key(&self) -> bool {
        self.secrets.get(SecretKey::TomTomApiKey).is_some()
    }

    /// Read the current traffic provider and budget status.
    pub fn status(&self) -> TrafficStatus {
        let now_s =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        let mut gov = self.budget.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = gov.current(now_s);

        TrafficStatus {
            has_key: self.has_key(),
            daily_count: current.count,
            budget: self.config.daily_budget,
            date: current.date,
        }
    }

    fn flow_cache_path(&self, z: u32, x: u32, y: u32) -> PathBuf {
        self.config.cache_dir.join(format!("flow-{z}-{x}-{y}.pbf"))
    }

    fn road_cache_path(&self, z: u32, x: u32, y: u32) -> PathBuf {
        self.config.cache_dir.join(format!("road-{z}-{x}-{y}.pbf"))
    }

    fn read_disk_flow(&self, z: u32, x: u32, y: u32) -> Option<(Vec<u8>, bool)> {
        let path = self.flow_cache_path(z, x, y);
        let meta = fs::metadata(&path).ok()?;
        let bytes = fs::read(&path).ok()?;
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        let age = SystemTime::now().duration_since(mtime).unwrap_or(Duration::ZERO);
        let is_fresh = age < TILE_TTL;
        Some((bytes, is_fresh))
    }

    fn write_disk_flow(&self, z: u32, x: u32, y: u32, bytes: &[u8]) {
        if let Err(e) = fs::create_dir_all(&self.config.cache_dir) {
            tracing::warn!("Failed to create traffic cache dir: {e}");
            return;
        }
        let path = self.flow_cache_path(z, x, y);
        let _ = fs::write(&path, bytes);
        // Sweep disk cache if needed
        let _ = sweep_disk_cache(
            &self.config.cache_dir,
            self.config.max_disk_bytes,
            Duration::from_secs(86_400),
        );
    }

    /// Retrieve raw TomTom flow tile bytes (or cached copy) with cache status.
    pub async fn get_flow_tile(
        &self,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<(Vec<u8>, CacheStatus), TrafficError> {
        if !is_valid_tile_coord(z, x, y, MIN_TILE_ZOOM, MAX_TILE_ZOOM) {
            return Err(TrafficError::InvalidTile { z, x, y });
        }

        let key = format!("{z}/{x}/{y}");
        let now = Instant::now();

        // 1. Check in-memory cache
        let (mem_bytes, mem_fresh) = {
            let mem = self.mem_flow.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = mem.get(&key) {
                let fresh = now.duration_since(entry.fetched_at) < TILE_TTL;
                (Some(entry.bytes.clone()), fresh)
            } else {
                (None, false)
            }
        };

        if mem_fresh {
            if let Some(bytes) = mem_bytes {
                return Ok((bytes, CacheStatus::Hit));
            }
        }

        // 2. Check disk cache
        let disk_entry = self.read_disk_flow(z, x, y);
        if let Some((bytes, is_fresh)) = &disk_entry {
            if *is_fresh {
                let mut mem =
                    self.mem_flow.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                mem.insert(key, MemEntry { bytes: bytes.clone(), fetched_at: now });
                return Ok((bytes.clone(), CacheStatus::Hit));
            }
        }

        // Stale candidate from memory or disk
        let stale_bytes = mem_bytes.or_else(|| disk_entry.map(|(b, _)| b));

        // 3. Key check
        let Some(api_key) = self.secrets.get(SecretKey::TomTomApiKey) else {
            return Err(TrafficError::NoKey);
        };

        let now_s =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        // 4. Budget governor check
        {
            let mut gov = self.budget.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if gov.is_over_budget(now_s, self.config.daily_budget) {
                if let Some(stale) = stale_bytes {
                    return Ok((stale, CacheStatus::StaleBudget));
                }
                return Err(TrafficError::BudgetExceeded(gov.current(now_s).count));
            }
            gov.record_fetch(now_s);
        }

        // 5. Upstream fetch
        let secret_val = secrecy::ExposeSecret::expose_secret(&api_key);
        let url = format!(
            "{}/{}/{}/{}.pbf?key={}",
            self.config.tomtom_base_url.trim_end_matches('/'),
            z,
            x,
            y,
            url::form_urlencoded::byte_serialize(secret_val.as_bytes()).collect::<String>()
        );

        let client = self.fetcher.client();
        let resp = client.get(&url).timeout(Duration::from_secs(15)).send().await;

        match resp {
            Ok(res) if res.status().is_success() => {
                match res.bytes().await {
                    Ok(body_bytes) if !body_bytes.is_empty() => {
                        let vec_bytes = body_bytes.to_vec();
                        // Cache in memory and on disk
                        {
                            let mut mem = self
                                .mem_flow
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            mem.insert(key, MemEntry { bytes: vec_bytes.clone(), fetched_at: now });
                        }
                        self.write_disk_flow(z, x, y, &vec_bytes);
                        Ok((vec_bytes, CacheStatus::Miss))
                    }
                    _ => {
                        if let Some(stale) = stale_bytes {
                            Ok((stale, CacheStatus::StaleError))
                        } else {
                            Err(TrafficError::Upstream("empty or unreadable tile body".into()))
                        }
                    }
                }
            }
            Ok(res) => {
                let status = res.status();
                if let Some(stale) = stale_bytes {
                    Ok((stale, CacheStatus::StaleError))
                } else {
                    Err(TrafficError::Upstream(format!("HTTP {status}")))
                }
            }
            Err(e) => {
                if let Some(stale) = stale_bytes {
                    Ok((stale, CacheStatus::StaleError))
                } else {
                    Err(TrafficError::Upstream(e.to_string()))
                }
            }
        }
    }

    /// Retrieve OpenFreeMap road vector tile bytes (from memory/disk or upstream).
    pub async fn get_road_tile(&self, z: u32, x: u32, y: u32) -> Result<Vec<u8>, TrafficError> {
        let key = format!("{z}/{x}/{y}");
        {
            let mem = self.mem_roads.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = mem.get(&key) {
                return Ok(cached.clone());
            }
        }

        let disk_path = self.road_cache_path(z, x, y);
        if let Ok(bytes) = fs::read(&disk_path) {
            let mut mem = self.mem_roads.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            mem.insert(key, bytes.clone());
            return Ok(bytes);
        }

        let url = format!(
            "{}/{}/{}/{}.pbf",
            self.config.openfreemap_tile_url.trim_end_matches('/'),
            z,
            x,
            y
        );

        let client = self.fetcher.client();
        let resp = client
            .get(&url)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| TrafficError::Upstream(format!("Road tile fetch failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(TrafficError::Upstream(format!(
                "Road tile upstream HTTP {}",
                resp.status()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TrafficError::Upstream(format!("Road tile body error: {e}")))?
            .to_vec();

        if let Err(e) = fs::create_dir_all(&self.config.cache_dir) {
            tracing::warn!("Failed to create road cache dir: {e}");
        }
        let _ = fs::write(&disk_path, &bytes);

        let mut mem = self.mem_roads.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        mem.insert(key, bytes.clone());

        Ok(bytes)
    }

    /// Retrieve compact matched roads with traffic flow for a tile.
    pub async fn get_traffic_tile(
        &self,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<TrafficTileResponse, TrafficError> {
        let (flow_bytes, _) = self.get_flow_tile(z, x, y).await?;
        let flow_segments =
            decode_tomtom_flow_tile(&flow_bytes, z, x, y).map_err(TrafficError::Decode)?;

        let road_bytes = self.get_road_tile(z, x, y).await?;
        let roads = decode_openfreemap_roads(&road_bytes, z, x, y).map_err(TrafficError::Decode)?;

        let matched = match_flow_to_roads(&roads, &flow_segments);

        Ok(TrafficTileResponse { z, x, y, roads: matched.matched_roads })
    }
}
