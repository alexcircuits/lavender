//! Mapillary coverage vector tiles and Graph API provider.
//!
//! Exposes:
//! - Coverage vector tile proxy (layer `coverage`, upstream `mly1_public`) with zoom validation (0..=5, 11..=14).
//! - Vector tile protobuf trimmer stripping the voluminous `image` layer in zoom 14 tiles.
//! - Multi-tier caching: 96 MiB in-memory LRU cache and size-capped disk cache (max 1 GiB) with periodic sweep.
//! - Upstream request coalescing (single-flight) and bounded upstream concurrency (6 permits).
//! - Upstream error hold for key rejection (401/403) and rate limits (429 with `Retry-After`).
//! - Graph API relay for street-level image searches.
//! - Server-held credentials: the access token is injected server-side and never leaks to responses or errors.

#![allow(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::struct_excessive_bools
)]

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use bytes::Bytes;
use reqwest::header::{self, HeaderMap, HeaderValue};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, broadcast};
use url::Url;

use crate::fetch::{FetchOptions, FetchResponse, Fetcher, HostPolicy};
use crate::secrets::{SecretKey, SecretStore};
use lavender_domain::geo::BBox;

/// Mapillary vector tile root host.
pub const MAPILLARY_TILE_HOST: &str = "https://tiles.mapillary.com/maps/vtp";

/// Mapillary Graph API root host.
pub const MAPILLARY_GRAPH_HOST: &str = "https://graph.mapillary.com";

/// Public tile layer names exposed.
pub const LAYER_COVERAGE: &str = "coverage";
pub const UPSTREAM_LAYER_COVERAGE: &str = "mly1_public";

/// Allowed zoom ranges for coverage tiles: z0..=5 and z11..=14.
pub const COVERAGE_ZOOM_RANGES: &[(u32, u32)] = &[(0, 5), (11, 14)];

/// Layer names to drop from z14 tiles.
pub const DROP_LAYERS_Z14: &[&str] = &["image"];

/// Disk cache TTL: 24 hours.
pub const TILE_DISK_TTL: Duration = Duration::from_hours(24);

/// Minimum interval between disk cache sweeps: 15 minutes.
pub const TILE_DISK_SWEEP_INTERVAL: Duration = Duration::from_mins(15);

/// Maximum disk cache size: 1 GiB.
pub const TILE_DISK_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Per-file disk overhead charged for directory/inode tracking.
pub const TILE_DISK_FILE_OVERHEAD_BYTES: u64 = 4096;

/// Hard size ceiling for a single vector tile: 48 MiB.
pub const TILE_MAX_BYTES: usize = 48 * 1024 * 1024;

/// In-memory LRU budget: 96 MiB.
pub const TILE_MEMORY_BUDGET_BYTES: usize = 96 * 1024 * 1024;

/// Per-entry overhead for memory tracking.
pub const TILE_MEMORY_ENTRY_OVERHEAD_BYTES: usize = 1024;

/// Upstream concurrency limit: 6 simultaneous requests.
pub const TILE_UPSTREAM_CONCURRENCY: usize = 6;

/// Time to hold misses after a 401 or 403 token rejection: 5 minutes.
pub const TILE_KEY_REJECTED_HOLD: Duration = Duration::from_mins(5);

/// Default rate limit hold if Retry-After is absent or unparseable: 60 seconds.
pub const TILE_RATE_LIMIT_HOLD: Duration = Duration::from_secs(60);

/// Maximum Retry-After hold honoured: 10 minutes.
pub const TILE_RATE_LIMIT_MAX_HOLD: Duration = Duration::from_mins(10);

/// Host policy allowing only Mapillary hosts.
static MAPILLARY_HOST_POLICY: LazyLock<HostPolicy> =
    LazyLock::new(|| HostPolicy::new(&["tiles.mapillary.com", "graph.mapillary.com"]));

// ---------------------------------------------------------------------------
// Protobuf MVT Wire Trimming
// ---------------------------------------------------------------------------

fn decode_varint(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;
    while *pos < bytes.len() {
        let b = bytes[*pos];
        *pos += 1;
        result |= u64::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

fn encode_varint(mut val: u64, out: &mut Vec<u8>) {
    while val >= 0x80 {
        out.push(((val & 0x7F) as u8) | 0x80);
        val >>= 7;
    }
    out.push(val as u8);
}

fn skip_field(wire_type: u8, bytes: &[u8], pos: &mut usize) -> bool {
    match wire_type {
        0 => decode_varint(bytes, pos).is_some(),
        1 if *pos + 8 <= bytes.len() => {
            *pos += 8;
            true
        }
        2 => {
            if let Some(len) = decode_varint(bytes, pos) {
                let len = len as usize;
                if *pos + len <= bytes.len() {
                    *pos += len;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        5 if *pos + 4 <= bytes.len() => {
            *pos += 4;
            true
        }
        _ => false,
    }
}

/// Reads the name string of an MVT Layer without fully decoding other features.
#[must_use]
pub fn layer_name(bytes: &[u8]) -> Option<String> {
    let mut pos = 0;
    while pos < bytes.len() {
        let tag = decode_varint(bytes, &mut pos)?;
        let field_num = tag >> 3;
        let wire_type = (tag & 0x07) as u8;
        if field_num == 1 && wire_type == 2 {
            let len = decode_varint(bytes, &mut pos)? as usize;
            if pos + len <= bytes.len() {
                return Some(String::from_utf8_lossy(&bytes[pos..pos + len]).to_string());
            }
            return None;
        }
        if !skip_field(wire_type, bytes, &mut pos) {
            return None;
        }
    }
    None
}

/// Lists all layer names contained inside a vector tile in order.
#[must_use]
pub fn list_tile_layers(bytes: &[u8]) -> Vec<String> {
    let mut layers = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let Some(tag) = decode_varint(bytes, &mut pos) else { break };
        let field_num = tag >> 3;
        let wire_type = (tag & 0x07) as u8;
        if field_num == 3 && wire_type == 2 {
            let Some(len) = decode_varint(bytes, &mut pos) else { break };
            let len = len as usize;
            if pos + len <= bytes.len() {
                if let Some(name) = layer_name(&bytes[pos..pos + len]) {
                    layers.push(name);
                }
                pos += len;
            } else {
                break;
            }
        } else if !skip_field(wire_type, bytes, &mut pos) {
            break;
        }
    }
    layers
}

/// Drops whole layers from a vector tile without decoding preserved layers.
#[must_use]
pub fn strip_tile_layers(bytes: &[u8], drop_names: &[&str]) -> Vec<u8> {
    if bytes.is_empty() || drop_names.is_empty() {
        return bytes.to_vec();
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut pos = 0;
    let mut dropped = false;

    while pos < bytes.len() {
        let field_start = pos;
        let Some(tag) = decode_varint(bytes, &mut pos) else { break };
        let field_num = tag >> 3;
        let wire_type = (tag & 0x07) as u8;

        if field_num == 3 && wire_type == 2 {
            let Some(len) = decode_varint(bytes, &mut pos) else { break };
            let len = len as usize;
            if pos + len <= bytes.len() {
                let layer_bytes = &bytes[pos..pos + len];
                if let Some(name) = layer_name(layer_bytes)
                    && drop_names.contains(&name.as_str())
                {
                    dropped = true;
                    pos += len;
                    continue;
                }
                encode_varint(tag, &mut out);
                encode_varint(len as u64, &mut out);
                out.extend_from_slice(layer_bytes);
                pos += len;
            } else {
                break;
            }
        } else if skip_field(wire_type, bytes, &mut pos) {
            out.extend_from_slice(&bytes[field_start..pos]);
        } else {
            break;
        }
    }

    if dropped { out } else { bytes.to_vec() }
}

// ---------------------------------------------------------------------------
// Tile Address Validation & Error Types
// ---------------------------------------------------------------------------

/// Address of a Mapillary vector tile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TileAddress {
    pub layer: String,
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

impl TileAddress {
    /// Validates zoom ranges and tile coordinate bounds.
    ///
    /// # Errors
    /// Returns [`MapillaryError::InvalidAddress`] if zoom or coordinates are illegal.
    pub fn parse(layer: &str, z: u32, x: u32, y: u32) -> Result<Self, MapillaryError> {
        if layer != LAYER_COVERAGE {
            return Err(MapillaryError::InvalidAddress(format!(
                "Unknown tile layer '{layer}', only '{LAYER_COVERAGE}' is supported"
            )));
        }

        let zoom_valid = COVERAGE_ZOOM_RANGES.iter().any(|&(min, max)| z >= min && z <= max);
        if !zoom_valid {
            return Err(MapillaryError::InvalidAddress(
                "Tile zoom for coverage must be 0–5 or 11–14".to_string(),
            ));
        }

        let max_coord = 1u32.checked_shl(z).unwrap_or(u32::MAX);
        if x >= max_coord || y >= max_coord {
            return Err(MapillaryError::InvalidAddress("Tile address out of range".to_string()));
        }

        Ok(Self { layer: layer.to_string(), z, x, y })
    }

    #[must_use]
    pub fn cache_key(&self) -> String {
        format!("{}/{}/{}/{}", self.layer, self.z, self.x, self.y)
    }

    #[must_use]
    pub fn relative_path(&self) -> PathBuf {
        PathBuf::from(&self.layer)
            .join(self.z.to_string())
            .join(format!("{}-{}.pbf", self.x, self.y))
    }
}

/// Source from which a tile was served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileCacheSource {
    Memory,
    Disk,
    InFlight,
    Upstream,
}

impl TileCacheSource {
    #[must_use]
    pub fn as_header_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::InFlight => "inflight",
            Self::Upstream => "upstream",
        }
    }
}

/// Errors occurring in Mapillary operations.
#[derive(Debug, thiserror::Error)]
pub enum MapillaryError {
    #[error("Invalid tile address: {0}")]
    InvalidAddress(String),

    #[error("Mapillary client token not configured")]
    MissingToken,

    #[error("Mapillary rejected the access token")]
    KeyRejected,

    #[error("Mapillary is rate-limiting requests (retry after {retry_after_secs}s)")]
    RateLimited { retry_after_secs: u64 },

    #[error("Mapillary upstream returned HTTP {status}: {message}")]
    Upstream { status: u16, message: String },

    #[error("Tile exceeds size cap")]
    TileTooLarge,

    #[error("Mapillary redirect left allowed origin")]
    InvalidRedirect,

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Network error: {0}")]
    Network(String),
}

// ---------------------------------------------------------------------------
// In-Memory LRU Cache
// ---------------------------------------------------------------------------

struct MemoryEntry {
    bytes: Bytes,
    at: Instant,
}

struct MemoryCache {
    entries: HashMap<String, MemoryEntry>,
    order: VecDeque<String>,
    total_bytes: usize,
}

impl MemoryCache {
    fn new() -> Self {
        Self { entries: HashMap::new(), order: VecDeque::new(), total_bytes: 0 }
    }

    fn entry_cost(bytes: &Bytes) -> usize {
        bytes.len() + TILE_MEMORY_ENTRY_OVERHEAD_BYTES
    }

    fn get(&mut self, key: &str) -> Option<Bytes> {
        if let Some(entry) = self.entries.get(key) {
            if entry.at.elapsed() > TILE_DISK_TTL {
                self.remove(key);
                return None;
            }
            let bytes = entry.bytes.clone();
            // Move to back (most recently used)
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            Some(bytes)
        } else {
            None
        }
    }

    fn put(&mut self, key: String, bytes: &Bytes) {
        let cost = Self::entry_cost(bytes);
        if cost > TILE_MEMORY_BUDGET_BYTES / 2 {
            return;
        }

        self.remove(&key);

        while self.total_bytes + cost > TILE_MEMORY_BUDGET_BYTES && !self.order.is_empty() {
            if let Some(oldest) = self.order.pop_front()
                && let Some(entry) = self.entries.remove(&oldest)
            {
                self.total_bytes = self.total_bytes.saturating_sub(Self::entry_cost(&entry.bytes));
            }
        }

        self.entries.insert(key.clone(), MemoryEntry { bytes: bytes.clone(), at: Instant::now() });
        self.order.push_back(key);
        self.total_bytes += cost;
    }

    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(Self::entry_cost(&entry.bytes));
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.total_bytes)
    }
}

// ---------------------------------------------------------------------------
// Disk Cache & Sweep
// ---------------------------------------------------------------------------

struct DiskFileInfo {
    path: PathBuf,
    size: u64,
    mtime: SystemTime,
}

fn collect_cache_files(dir: &Path) -> Vec<DiskFileInfo> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return files;
    }

    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&current) else { continue };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file()
                && let Ok(meta) = entry.metadata()
            {
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push(DiskFileInfo { path, size: meta.len(), mtime });
            }
        }
    }
    files
}

/// Sweeps the tile disk cache: removes expired tiles (>24h), then oldest until under `max_bytes`.
#[must_use]
pub fn sweep_tile_disk(root: &Path, max_bytes: u64) -> (usize, u64) {
    let mut files = collect_cache_files(root);
    // Sort oldest first
    files.sort_by_key(|f| f.mtime);

    let cost = |size: u64| size.saturating_add(TILE_DISK_FILE_OVERHEAD_BYTES);
    let mut total_bytes: u64 = files.iter().map(|f| cost(f.size)).sum();
    let mut removed = 0;

    let now = SystemTime::now();
    for file in files {
        let age = now.duration_since(file.mtime).unwrap_or(Duration::ZERO);
        if age <= TILE_DISK_TTL && total_bytes <= max_bytes {
            break;
        }

        if std::fs::remove_file(&file.path).is_ok() {
            total_bytes = total_bytes.saturating_sub(cost(file.size));
            removed += 1;
        }
    }

    (removed, total_bytes)
}

// ---------------------------------------------------------------------------
// Upstream Hold & Single-Flight Coalescing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum UpstreamHold {
    KeyRejected(Instant),
    RateLimited { until: Instant, retry_after_secs: u64 },
}

// ---------------------------------------------------------------------------
// Mapillary Provider
// ---------------------------------------------------------------------------

/// Provider handle managing Mapillary coverage tiles and image searches.
pub struct MapillaryProvider {
    fetcher: Arc<Fetcher>,
    secrets: Arc<dyn SecretStore>,
    disk_dir: PathBuf,
    memory: Mutex<MemoryCache>,
    upstream_semaphore: Arc<Semaphore>,
    in_flight: Mutex<HashMap<String, broadcast::Sender<Result<Bytes, String>>>>,
    upstream_hold: Mutex<Option<UpstreamHold>>,
    last_sweep: Mutex<Instant>,
    write_seq: AtomicU64,
}

impl std::fmt::Debug for MapillaryProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapillaryProvider")
            .field("disk_dir", &self.disk_dir)
            .field("configured", &self.is_configured())
            .finish_non_exhaustive()
    }
}

impl MapillaryProvider {
    /// Creates a new Mapillary provider instance.
    #[must_use]
    pub fn new(
        fetcher: Arc<Fetcher>,
        secrets: Arc<dyn SecretStore>,
        disk_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            fetcher,
            secrets,
            disk_dir: disk_dir.into(),
            memory: Mutex::new(MemoryCache::new()),
            upstream_semaphore: Arc::new(Semaphore::new(TILE_UPSTREAM_CONCURRENCY)),
            in_flight: Mutex::new(HashMap::new()),
            upstream_hold: Mutex::new(None),
            last_sweep: Mutex::new(
                Instant::now().checked_sub(TILE_DISK_SWEEP_INTERVAL).unwrap_or_else(Instant::now),
            ),
            write_seq: AtomicU64::new(0),
        }
    }

    /// Whether a Mapillary token is configured on the server.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.secrets
            .get(SecretKey::MapillaryClientToken)
            .is_some_and(|s| !s.expose_secret().trim().is_empty())
    }

    fn check_upstream_hold(&self) -> Result<(), MapillaryError> {
        let mut lock = self.upstream_hold.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref hold) = *lock {
            match hold {
                UpstreamHold::KeyRejected(until) => {
                    if Instant::now() < *until {
                        return Err(MapillaryError::KeyRejected);
                    }
                }
                UpstreamHold::RateLimited { until, retry_after_secs } => {
                    if Instant::now() < *until {
                        return Err(MapillaryError::RateLimited {
                            retry_after_secs: *retry_after_secs,
                        });
                    }
                }
            }
            *lock = None;
        }
        Ok(())
    }

    fn set_upstream_hold(&self, hold: UpstreamHold) {
        let mut lock = self.upstream_hold.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *lock = Some(hold);
    }

    fn read_disk(&self, address: &TileAddress) -> Option<Bytes> {
        let path = self.disk_dir.join(address.relative_path());
        let meta = std::fs::metadata(&path).ok()?;
        let mtime = meta.modified().ok()?;
        if SystemTime::now().duration_since(mtime).unwrap_or(Duration::ZERO) > TILE_DISK_TTL {
            return None;
        }
        let data = std::fs::read(&path).ok()?;
        Some(Bytes::from(data))
    }

    fn write_disk(&self, address: &TileAddress, bytes: &Bytes) {
        let file = self.disk_dir.join(address.relative_path());
        let Some(parent) = file.parent() else { return };
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("Failed to create tile cache directory: {e}");
            return;
        }

        let seq = self.write_seq.fetch_add(1, Ordering::Relaxed);
        let tmp = format!("{}.{}-{}.tmp", file.display(), std::process::id(), seq);
        let tmp_path = PathBuf::from(tmp);

        if std::fs::write(&tmp_path, bytes).is_err() {
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }

        if std::fs::rename(&tmp_path, &file).is_err() {
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }

        self.maybe_sweep();
    }

    fn maybe_sweep(&self) {
        let should_sweep = {
            let mut last =
                self.last_sweep.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.elapsed() >= TILE_DISK_SWEEP_INTERVAL {
                *last = Instant::now();
                true
            } else {
                false
            }
        };

        if should_sweep {
            let root = self.disk_dir.clone();
            tokio::task::spawn_blocking(move || {
                let (removed, kept) = sweep_tile_disk(&root, TILE_DISK_MAX_BYTES);
                if removed > 0 {
                    tracing::debug!(
                        "Mapillary disk cache swept: removed {removed} files, keeping {kept} bytes"
                    );
                }
            });
        }
    }

    /// Fetches a Mapillary vector tile with caching, deduplication, and trimming.
    ///
    /// # Errors
    /// Returns [`MapillaryError`] if address is invalid, token is missing, or upstream fails.
    pub async fn fetch_tile(
        &self,
        layer: &str,
        z: u32,
        x: u32,
        y: u32,
    ) -> Result<(Bytes, TileCacheSource), MapillaryError> {
        let address = TileAddress::parse(layer, z, x, y)?;
        let key = address.cache_key();

        // 1. Check in-memory cache
        {
            let mut mem = self.memory.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(bytes) = mem.get(&key) {
                return Ok((bytes, TileCacheSource::Memory));
            }
        }

        // 2. Check disk cache
        if let Some(disk_bytes) = self.read_disk(&address) {
            let trimmed = if address.z >= 14 {
                Bytes::from(strip_tile_layers(&disk_bytes, DROP_LAYERS_Z14))
            } else {
                disk_bytes
            };
            self.memory
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .put(key, &trimmed);
            return Ok((trimmed, TileCacheSource::Disk));
        }

        // 3. Single-flight / In-flight coalescing
        let (tx, mut rx, is_leader) = {
            let mut in_flight =
                self.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(existing_tx) = in_flight.get(&key) {
                (None, existing_tx.subscribe(), false)
            } else {
                let (new_tx, new_rx) = broadcast::channel(1);
                in_flight.insert(key.clone(), new_tx.clone());
                (Some(new_tx), new_rx, true)
            }
        };

        if !is_leader {
            match rx.recv().await {
                Ok(Ok(bytes)) => return Ok((bytes, TileCacheSource::InFlight)),
                Ok(Err(err)) => return Err(MapillaryError::Network(err)),
                Err(_) => return Err(MapillaryError::Network("In-flight request dropped".into())),
            }
        }

        // Leader performs upstream fetch
        let fetch_res = self.fetch_upstream_tile(&address).await;
        let mut in_flight =
            self.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        in_flight.remove(&key);

        match fetch_res {
            Ok(bytes) => {
                if let Some(ref tx_chan) = tx {
                    let _ = tx_chan.send(Ok(bytes.clone()));
                }
                self.memory
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .put(key, &bytes);
                self.write_disk(&address, &bytes);
                Ok((bytes, TileCacheSource::Upstream))
            }
            Err(err) => {
                if let Some(ref tx_chan) = tx {
                    let _ = tx_chan.send(Err(err.to_string()));
                }
                Err(err)
            }
        }
    }

    async fn fetch_upstream_tile(&self, address: &TileAddress) -> Result<Bytes, MapillaryError> {
        let token = self
            .secrets
            .get(SecretKey::MapillaryClientToken)
            .ok_or(MapillaryError::MissingToken)?;
        let raw_token = token.expose_secret().trim();
        if raw_token.is_empty() {
            return Err(MapillaryError::MissingToken);
        }

        self.check_upstream_hold()?;

        let _permit = self
            .upstream_semaphore
            .acquire()
            .await
            .map_err(|_| MapillaryError::Network("Semaphore closed".into()))?;

        let url_str = format!(
            "{MAPILLARY_TILE_HOST}/{UPSTREAM_LAYER_COVERAGE}/2/{z}/{x}/{y}?access_token={token}",
            z = address.z,
            x = address.x,
            y = address.y,
            token = url::form_urlencoded::byte_serialize(raw_token.as_bytes()).collect::<String>()
        );
        let url = Url::parse(&url_str)
            .map_err(|e| MapillaryError::Network(format!("Invalid tile URL: {e}")))?;

        let opts = FetchOptions {
            max_body_bytes: Some(TILE_MAX_BYTES),
            timeout: Some(Duration::from_secs(60)),
            extra_headers: vec![(
                header::ACCEPT,
                HeaderValue::from_static("application/x-protobuf"),
            )],
            ..Default::default()
        };

        let res = self.fetcher.get(&url, &MAPILLARY_HOST_POLICY, &opts).await;

        match res {
            Ok(FetchResponse { status, body, headers, .. }) => {
                if status == 404 || status == 204 {
                    return Ok(Bytes::new());
                }
                if status == 401 || status == 403 {
                    self.set_upstream_hold(UpstreamHold::KeyRejected(
                        Instant::now() + TILE_KEY_REJECTED_HOLD,
                    ));
                    return Err(MapillaryError::KeyRejected);
                }
                if status == 429 {
                    let retry_after = parse_retry_after(&headers);
                    self.set_upstream_hold(UpstreamHold::RateLimited {
                        until: Instant::now() + retry_after,
                        retry_after_secs: retry_after.as_secs(),
                    });
                    return Err(MapillaryError::RateLimited {
                        retry_after_secs: retry_after.as_secs(),
                    });
                }
                if !(200..300).contains(&status) {
                    return Err(MapillaryError::Upstream {
                        status,
                        message: "Mapillary upstream failed".to_string(),
                    });
                }

                // Strip image layer if zoom >= 14
                let final_bytes = if address.z >= 14 {
                    Bytes::from(strip_tile_layers(&body, DROP_LAYERS_Z14))
                } else {
                    body
                };

                Ok(final_bytes)
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("401") || err_str.contains("403") {
                    self.set_upstream_hold(UpstreamHold::KeyRejected(
                        Instant::now() + TILE_KEY_REJECTED_HOLD,
                    ));
                    return Err(MapillaryError::KeyRejected);
                }
                if err_str.contains("429") {
                    self.set_upstream_hold(UpstreamHold::RateLimited {
                        until: Instant::now() + TILE_RATE_LIMIT_HOLD,
                        retry_after_secs: TILE_RATE_LIMIT_HOLD.as_secs(),
                    });
                    return Err(MapillaryError::RateLimited {
                        retry_after_secs: TILE_RATE_LIMIT_HOLD.as_secs(),
                    });
                }
                if err_str.contains("sent more than") {
                    return Err(MapillaryError::TileTooLarge);
                }
                Err(MapillaryError::Network(err_str))
            }
        }
    }

    /// Searches Mapillary images inside `bbox` using the Graph API.
    ///
    /// # Errors
    /// Returns [`MapillaryError`] on invalid parameters, missing token, or upstream errors.
    pub async fn search_images(
        &self,
        bbox: &BBox,
        limit: usize,
    ) -> Result<MapillaryImageSearchResponse, MapillaryError> {
        let token = self
            .secrets
            .get(SecretKey::MapillaryClientToken)
            .ok_or(MapillaryError::MissingToken)?;
        let raw_token = token.expose_secret().trim();
        if raw_token.is_empty() {
            return Err(MapillaryError::MissingToken);
        }

        let clamped_limit = limit.clamp(1, 100);
        let fields = "id,geometry,computed_geometry,captured_at,thumb_256_url,thumb_1024_url,thumb_2048_url,compass_angle,creator,sequence";
        let url_str = format!(
            "{MAPILLARY_GRAPH_HOST}/images?fields={fields}&bbox={west},{south},{east},{north}&limit={limit}&access_token={token}",
            west = bbox.west,
            south = bbox.south,
            east = bbox.east,
            north = bbox.north,
            limit = clamped_limit,
            token = url::form_urlencoded::byte_serialize(raw_token.as_bytes()).collect::<String>()
        );
        let url = Url::parse(&url_str)
            .map_err(|e| MapillaryError::Network(format!("Invalid Graph API URL: {e}")))?;

        let opts = FetchOptions {
            max_body_bytes: Some(4 * 1024 * 1024),
            timeout: Some(Duration::from_secs(30)),
            extra_headers: vec![(header::ACCEPT, HeaderValue::from_static("application/json"))],
            ..Default::default()
        };

        let res = self
            .fetcher
            .get(&url, &MAPILLARY_HOST_POLICY, &opts)
            .await
            .map_err(|e| MapillaryError::Network(e.to_string()))?;

        if !(200..300).contains(&res.status) {
            if res.status == 401 || res.status == 403 {
                return Err(MapillaryError::KeyRejected);
            }
            return Err(MapillaryError::Upstream {
                status: res.status,
                message: "Graph API search failed".to_string(),
            });
        }

        let parsed: MapillaryImageSearchResponse =
            serde_json::from_slice(&res.body).map_err(|e| {
                MapillaryError::Network(format!("Failed to parse image search JSON: {e}"))
            })?;

        Ok(parsed)
    }

    /// Resets memory cache (test seam).
    pub fn reset_memory_for_test(&self) {
        self.memory.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        self.upstream_hold.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }

    /// Memory cache size and bytes (test seam).
    #[must_use]
    pub fn memory_stats_for_test(&self) -> (usize, usize) {
        self.memory.lock().unwrap_or_else(std::sync::PoisonError::into_inner).stats()
    }
}

fn parse_retry_after(headers: &HeaderMap) -> Duration {
    if let Some(val) = headers.get(header::RETRY_AFTER).and_then(|v| v.to_str().ok())
        && let Ok(secs) = val.trim().parse::<u64>()
    {
        return Duration::from_secs(secs).clamp(Duration::from_secs(1), TILE_RATE_LIMIT_MAX_HOLD);
    }
    TILE_RATE_LIMIT_HOLD
}

// ---------------------------------------------------------------------------
// Graph API Data Contracts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapillaryImageItem {
    pub id: String,
    pub geometry: Option<serde_json::Value>,
    #[serde(default)]
    pub computed_geometry: Option<serde_json::Value>,
    #[serde(default)]
    pub captured_at: Option<u64>,
    #[serde(default)]
    pub thumb_256_url: Option<String>,
    #[serde(default)]
    pub thumb_1024_url: Option<String>,
    #[serde(default)]
    pub thumb_2048_url: Option<String>,
    #[serde(default)]
    pub compass_angle: Option<f64>,
    #[serde(default)]
    pub creator: Option<serde_json::Value>,
    #[serde(default)]
    pub sequence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapillaryImageSearchResponse {
    pub data: Vec<MapillaryImageItem>,
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FetchConfig;
    use crate::secrets::MemorySecretStore;
    use tempfile::tempdir;

    fn build_test_tile(layers: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(name, payload) in layers {
            let mut layer_msg = Vec::new();
            // Field 1: name (wire type 2)
            encode_varint((1 << 3) | 2, &mut layer_msg);
            encode_varint(name.len() as u64, &mut layer_msg);
            layer_msg.extend_from_slice(name.as_bytes());

            // Field 2: feature or custom payload
            if !payload.is_empty() {
                encode_varint((2 << 3) | 2, &mut layer_msg);
                encode_varint(payload.len() as u64, &mut layer_msg);
                layer_msg.extend_from_slice(payload);
            }

            // Top level: Field 3: layer (wire type 2)
            encode_varint((3 << 3) | 2, &mut out);
            encode_varint(layer_msg.len() as u64, &mut out);
            out.extend_from_slice(&layer_msg);
        }
        out
    }

    #[test]
    fn test_list_tile_layers() {
        let tile = build_test_tile(&[("sequence", b"seq_data"), ("image", b"img_data")]);
        let names = list_tile_layers(&tile);
        assert_eq!(names, vec!["sequence", "image"]);
    }

    #[test]
    fn test_strip_tile_layers() {
        let tile = build_test_tile(&[
            ("sequence", b"seq_data"),
            ("image", b"img_data_massive"),
            ("overview", b"overview_data"),
        ]);
        let trimmed = strip_tile_layers(&tile, &["image"]);
        let remaining = list_tile_layers(&trimmed);
        assert_eq!(remaining, vec!["sequence", "overview"]);
        assert!(trimmed.len() < tile.len());
    }

    #[test]
    fn test_tile_address_validation() {
        // Valid coverage zoom ranges: 0..=5 and 11..=14
        assert!(TileAddress::parse("coverage", 0, 0, 0).is_ok());
        assert!(TileAddress::parse("coverage", 5, 31, 31).is_ok());
        assert!(TileAddress::parse("coverage", 14, 16383, 16383).is_ok());

        // Invalid layer
        assert!(TileAddress::parse("signs", 14, 0, 0).is_err());

        // Disallowed zoom ranges (6..=10)
        assert!(TileAddress::parse("coverage", 6, 0, 0).is_err());
        assert!(TileAddress::parse("coverage", 10, 0, 0).is_err());
        assert!(TileAddress::parse("coverage", 15, 0, 0).is_err());

        // Out of coordinate bounds
        assert!(TileAddress::parse("coverage", 1, 2, 0).is_err());
        assert!(TileAddress::parse("coverage", 1, 0, 2).is_err());
    }

    #[test]
    fn test_memory_lru_cache_budget() {
        let mut cache = MemoryCache::new();
        let payload = Bytes::from(vec![42u8; 1000]);
        let key = "coverage/0/0/0".to_string();

        cache.put(key.clone(), &payload);
        assert_eq!(cache.get(&key), Some(payload));
        assert_eq!(cache.stats().0, 1);

        cache.clear();
        assert_eq!(cache.stats().0, 0);
        assert_eq!(cache.stats().1, 0);
    }

    #[test]
    fn test_disk_cache_sweep_cap() {
        let dir = tempdir().unwrap();
        let tile_dir = dir.path().join("coverage/14");
        std::fs::create_dir_all(&tile_dir).unwrap();

        let file1 = tile_dir.join("1-1.pbf");
        let file2 = tile_dir.join("2-2.pbf");
        let file3 = tile_dir.join("3-3.pbf");

        std::fs::write(&file1, vec![1u8; 10000]).unwrap();
        std::fs::write(&file2, vec![2u8; 10000]).unwrap();
        std::fs::write(&file3, vec![3u8; 10000]).unwrap();

        // 3 files each cost 10000 + 4096 = 14096 bytes -> total ~42288 bytes
        // If max_bytes is 20000, 2 oldest files must be deleted!
        let (removed, kept) = sweep_tile_disk(dir.path(), 20000);
        assert!(removed >= 1);
        assert!(kept <= 20000);
    }

    #[tokio::test]
    async fn test_provider_status_probe() {
        let dir = tempdir().unwrap();
        let fetcher = Arc::new(Fetcher::new(FetchConfig::default()).unwrap());

        let empty_secrets = Arc::new(MemorySecretStore::default());
        let provider_no_key = MapillaryProvider::new(fetcher.clone(), empty_secrets, dir.path());
        assert!(!provider_no_key.is_configured());

        let with_key = Arc::new(MemorySecretStore::with([(
            SecretKey::MapillaryClientToken,
            "MLY|synthetic-token",
        )]));
        let provider_with_key = MapillaryProvider::new(fetcher, with_key, dir.path());
        assert!(provider_with_key.is_configured());
    }
}
