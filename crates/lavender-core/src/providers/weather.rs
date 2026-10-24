//! NOAA nowCOAST observed weather provider (radar, lightning density, clouds).
//!
//! Ported directly from `legacy/server/providers/weather.js` to Rust.
//! Manages WMS capabilities parsing, frame time validation, `GeographicTilingScheme` tile bounding,
//! and byte-capped LRU image caching with strict PNG signature validation.

#![allow(clippy::expect_used)] // LazyLock regex statics

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use bytes::Bytes;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::fetch::{FetchError, FetchOptions, Fetcher, HostPolicy};

const BASE_URL: &str = "https://nowcoast.noaa.gov/geoserver/observations/";
const MAX_METADATA_BYTES: usize = 512 * 1024;
const MAX_TILE_CACHE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TILE_ENTRIES: usize = 128;
const HOUR_MS: i64 = 3_600_000;
const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

static RE_TIME_FORMAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$").expect("valid regex")
});

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeatherBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone)]
pub struct WeatherProductSpec {
    pub id: &'static str,
    pub service: &'static str,
    pub layer: &'static str,
    pub style: &'static str,
    pub title: &'static str,
    pub coverage: &'static str,
    pub description: &'static str,
    pub attribution: &'static str,
    pub default_width: u32,
    pub default_height: u32,
    pub metadata_ttl_ms: u64,
}

pub const PRODUCTS: &[WeatherProductSpec] = &[
    WeatherProductSpec {
        id: "lightning",
        service: "lightning_detection",
        layer: "ldn_lightning_strike_density",
        style: "lightning_density",
        title: "Lightning density · 15 min",
        coverage: "Pacific and Americas: 110°E across the dateline to 0°, 25°S–80°N; not global coverage.",
        description: "Observed 15-minute lightning strike density on an approximately 8 km grid, scaled as strikes/km²/min ×10³. Ground-network density, not individual GLM flashes.",
        attribution: "NOAA/NWS nowCOAST; derived from Vaisala NLDN/GLD360",
        default_width: 4096,
        default_height: 2048,
        metadata_ttl_ms: 600_000,
    },
    WeatherProductSpec {
        id: "radar",
        service: "weather_radar",
        layer: "conus_base_reflectivity_mosaic",
        style: "weather_radar_base_reflectivity",
        title: "CONUS radar reflectivity",
        coverage: "Contiguous United States; gaps do not establish absence of precipitation.",
        description: "Observed MRMS radar base reflectivity (dBZ), approximately 1 km and 4-minute updates; not a rainfall forecast.",
        attribution: "NOAA/NWS/NESDIS nowCOAST",
        default_width: 4096,
        default_height: 2048,
        metadata_ttl_ms: 120_000,
    },
    WeatherProductSpec {
        id: "clouds",
        service: "satellite",
        layer: "global_longwave_imagery_mosaic",
        style: "reflectance",
        title: "Global satellite infrared",
        coverage: "Global mosaic with incomplete polar coverage; nominal coverage 60°S–60°N.",
        description: "Longwave infrared cloud and land/sea temperature patterns, approximately 3 km; hourly updates with 2–3 hour source latency. Not a cloud-only mask.",
        attribution: "NOAA/NWS/NESDIS nowCOAST",
        default_width: 2048,
        default_height: 1024,
        metadata_ttl_ms: 120_000,
    },
    WeatherProductSpec {
        id: "clouds-regional",
        service: "satellite",
        layer: "goes_longwave_imagery",
        style: "goes-lir",
        title: "GOES regional satellite infrared",
        coverage: "GOES East/West regional North American coverage; not a global image.",
        description: "GOES-19/18 longwave infrared Band 14 cloud and surface temperature patterns, approximately 2 km and 5-minute updates. Not a cloud-only mask.",
        attribution: "NOAA/NWS/NESDIS nowCOAST",
        default_width: 4096,
        default_height: 2048,
        metadata_ttl_ms: 120_000,
    },
];

#[must_use]
pub fn find_product_spec(id: &str) -> Option<&'static WeatherProductSpec> {
    PRODUCTS.iter().find(|p| p.id == id)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WeatherError {
    #[error("XML exceeds maximum size of 512 KiB")]
    XmlTooLarge,
    #[error("XML contains disallowed DOCTYPE or ENTITY definitions")]
    XxeAttack,
    #[error("Invalid weather metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("Weather observations expired")]
    WeatherObservationsExpired,
    #[error("Invalid weather tile coordinates")]
    InvalidTile,
    #[error("Invalid weather image format or dimensions")]
    InvalidImage,
    #[error("Unknown weather product '{0}'")]
    ProductNotFound(String),
    #[error("Weather upstream unavailable")]
    UpstreamUnavailable,
    #[error("Fetch error: {0}")]
    Fetch(String),
}

impl From<FetchError> for WeatherError {
    fn from(err: FetchError) -> Self {
        Self::Fetch(err.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherMetadata {
    pub product: String,
    pub bounds: WeatherBounds,
    pub times: Vec<String>,
    pub allowed_times: Vec<String>,
    pub default_time: String,
    pub fetched_at: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherManifest {
    pub schema_version: u32,
    pub product: String,
    pub title: String,
    pub coverage: String,
    pub description: String,
    pub source: String,
    pub attribution: String,
    pub bounds: Option<WeatherBounds>,
    pub times: Vec<String>,
    pub latest: Option<String>,
    pub time: Option<String>,
    pub observed_at: Option<String>,
    pub fetched_at: Option<i64>,
    pub stale: bool,
    pub unavailable: bool,
    pub reason: Option<String>,
    pub tile_size: u32,
    pub max_level: u32,
    pub tiling_scheme: String,
    pub tile_template: Option<String>,
    pub image_url: Option<String>,
}

/// Parses observation time canonicalizing to RFC3339 UTC representation.
#[must_use]
pub fn canonicalize_observation_time(val: &str) -> Option<String> {
    if !RE_TIME_FORMAT.is_match(val) {
        return None;
    }
    let ts = jiff::Timestamp::from_str(val).ok()?;
    Some(ts.to_string())
}

/// Parses NOAA nowCOAST WMS capabilities XML safely with entity expansion disabled.
///
/// # Errors
/// Returns [`WeatherError`] if the XML is malformed, contains XXE directives, or is missing required layers.
#[allow(clippy::too_many_lines)]
pub fn parse_weather_capabilities(
    xml: &str,
    product: &str,
    now_ms: i64,
) -> Result<WeatherMetadata, WeatherError> {
    let spec = find_product_spec(product)
        .ok_or_else(|| WeatherError::ProductNotFound(product.to_string()))?;

    if xml.len() > MAX_METADATA_BYTES {
        return Err(WeatherError::XmlTooLarge);
    }

    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return Err(WeatherError::XxeAttack);
    }

    // Manual stack extraction matching legacy parser to ensure exact behavior without entity expansion
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = false;

    let mut in_target_layer = false;
    let mut layer_depth: u32 = 0;
    let mut target_layer_depth: u32 = 0;
    let mut current_tag = String::new();

    let mut west = None;
    let mut south = None;
    let mut east = None;
    let mut north = None;
    let mut time_dimension_str = None;
    let mut default_time_str = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                let tag_name = name.as_ref();
                let local_name = tag_name.rsplit(':').next().unwrap_or(tag_name).to_string();

                if local_name == "Layer" {
                    layer_depth += 1;
                }

                if in_target_layer && local_name == "Dimension" {
                    let mut is_time = false;
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        let val = attr.value.as_ref();
                        if key == "name" && val == "time" {
                            is_time = true;
                        }
                        if key == "default" {
                            default_time_str = Some(val.to_string());
                        }
                    }
                    if is_time {
                        current_tag = "Dimension:time".to_string();
                    }
                } else {
                    current_tag = local_name;
                }
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                let text = e.as_ref();
                let trimmed = text.trim();

                if !in_target_layer && current_tag == "Name" && trimmed == spec.layer {
                    in_target_layer = true;
                    target_layer_depth = layer_depth;
                }

                if in_target_layer && layer_depth == target_layer_depth {
                    match current_tag.as_str() {
                        "westBoundLongitude" => west = trimmed.parse::<f64>().ok(),
                        "southBoundLatitude" => south = trimmed.parse::<f64>().ok(),
                        "eastBoundLongitude" => east = trimmed.parse::<f64>().ok(),
                        "northBoundLatitude" => north = trimmed.parse::<f64>().ok(),
                        "Dimension:time" => time_dimension_str = Some(trimmed.to_string()),
                        _ => {}
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                let tag_name = name.as_ref();
                let local_name = tag_name.rsplit(':').next().unwrap_or(tag_name);
                if local_name == "Layer" {
                    if in_target_layer && layer_depth == target_layer_depth {
                        break;
                    }
                    layer_depth = layer_depth.saturating_sub(1);
                }
                current_tag.clear();
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => return Err(WeatherError::InvalidMetadata("XML parse error")),
            _ => {}
        }
        buf.clear();
    }

    let (Some(w), Some(s), Some(e), Some(n)) = (west, south, east, north) else {
        return Err(WeatherError::InvalidMetadata("Missing geographic bounding box"));
    };

    if w < -180.0 || e > 180.0 || s < -90.0 || n > 90.0 || w >= e || s >= n {
        return Err(WeatherError::InvalidMetadata("Invalid bounding box values"));
    }

    let bounds = WeatherBounds { west: w, south: s, east: e, north: n };

    let Some(raw_times_str) = time_dimension_str else {
        return Err(WeatherError::InvalidMetadata("Missing time dimension"));
    };

    let mut times: Vec<String> = Vec::new();
    for token in raw_times_str.split(',') {
        let trimmed = token.trim();
        if let Some(canonical) = canonicalize_observation_time(trimmed)
            && let Ok(ts) = jiff::Timestamp::from_str(&canonical)
        {
            let epoch_ms = ts.as_millisecond();
            // Reject timestamps > 5 minutes in future
            if epoch_ms <= now_ms + 5 * 60_000 {
                times.push(canonical);
            }
        }
    }

    if times.is_empty() {
        return Err(WeatherError::InvalidMetadata("No valid observation times"));
    }

    // Retain entries in the last 24 hours
    let mut recent: Vec<String> = times
        .into_iter()
        .filter(|t| {
            if let Ok(ts) = jiff::Timestamp::from_str(t) {
                now_ms - ts.as_millisecond() <= 24 * HOUR_MS
            } else {
                false
            }
        })
        .collect();

    recent.sort();
    recent.dedup();

    if recent.is_empty() {
        return Err(WeatherError::WeatherObservationsExpired);
    }

    let default_time = default_time_str
        .and_then(|d| canonicalize_observation_time(&d))
        .unwrap_or_else(|| recent.last().cloned().unwrap_or_default());

    let allowed_times =
        if recent.len() > 26 { recent[recent.len() - 26..].to_vec() } else { recent.clone() };

    let selected_times =
        if recent.len() > 13 { recent[recent.len() - 13..].to_vec() } else { recent };

    Ok(WeatherMetadata {
        product: product.to_string(),
        bounds,
        times: selected_times,
        allowed_times,
        default_time,
        fetched_at: now_ms,
        stale: false,
    })
}

/// `GeographicTilingScheme` tile bounds calculation for zoom levels 0 through 6.
///
/// # Errors
/// Returns [`WeatherError::InvalidTile`] for coordinates outside zoom range 0..=6.
pub fn weather_tile_bounds(z: u32, x: u32, y: u32) -> Result<[f64; 4], WeatherError> {
    if z > 6 {
        return Err(WeatherError::InvalidTile);
    }
    let max_x = 2_u32.checked_shl(z).ok_or(WeatherError::InvalidTile)?;
    let max_y = 1_u32.checked_shl(z).ok_or(WeatherError::InvalidTile)?;

    if x >= max_x || y >= max_y {
        return Err(WeatherError::InvalidTile);
    }

    let span = 180.0 / f64::from(max_y);
    let west = -180.0 + f64::from(x) * span;
    let south = 90.0 - f64::from(y + 1) * span;
    let east = -180.0 + f64::from(x + 1) * span;
    let north = 90.0 - f64::from(y) * span;

    Ok([west, south, east, north])
}

/// Validates that image bytes conform to PNG magic bytes and IHDR header matching expected dimensions.
///
/// # Errors
/// Returns [`WeatherError::InvalidImage`] if the bytes are not a valid PNG or dimensions mismatch.
pub fn validate_png(bytes: &[u8], width: u32, height: u32) -> Result<(), WeatherError> {
    if bytes.len() < 33 || bytes[0..8] != PNG_SIG {
        return Err(WeatherError::InvalidImage);
    }

    // IHDR chunk: length 13 at [8..12], type "IHDR" at [12..16]
    let chunk_len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if chunk_len != 13 || &bytes[12..16] != b"IHDR" {
        return Err(WeatherError::InvalidImage);
    }

    let img_width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let img_height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

    if img_width != width || img_height != height {
        return Err(WeatherError::InvalidImage);
    }

    Ok(())
}

#[derive(Debug)]
struct TileCacheEntry {
    bytes: Bytes,
    at: i64,
}

#[derive(Debug, Default)]
struct TileCache {
    entries: HashMap<String, TileCacheEntry>,
    order: Vec<String>,
    total_bytes: usize,
}

impl TileCache {
    fn insert(&mut self, key: String, bytes: Bytes, now_ms: i64) {
        if let Some(old) = self.entries.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes.len());
            self.order.retain(|k| k != &key);
        }

        while self.entries.len() >= MAX_TILE_ENTRIES
            || (self.total_bytes + bytes.len() > MAX_TILE_CACHE_BYTES && !self.order.is_empty())
        {
            let oldest_key = self.order.remove(0);
            if let Some(removed) = self.entries.remove(&oldest_key) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.bytes.len());
            }
        }

        self.total_bytes += bytes.len();
        self.order.push(key.clone());
        self.entries.insert(key, TileCacheEntry { bytes, at: now_ms });
    }

    fn get(&mut self, key: &str, now_ms: i64) -> Option<Bytes> {
        let entry = self.entries.get(key)?;
        if now_ms - entry.at > 24 * HOUR_MS {
            return None;
        }
        let b = entry.bytes.clone();
        self.order.retain(|k| k != key);
        self.order.push(key.to_string());
        Some(b)
    }
}

/// Weather provider with cached WMS metadata and byte-budgeted tile proxy.
#[derive(Debug, Clone)]
pub struct WeatherProvider {
    fetcher: Arc<Fetcher>,
    metadata_cache: Arc<Mutex<HashMap<String, WeatherMetadata>>>,
    tile_cache: Arc<Mutex<TileCache>>,
    host_policy: HostPolicy,
}

impl WeatherProvider {
    #[must_use]
    pub fn new(fetcher: Arc<Fetcher>) -> Self {
        Self {
            fetcher,
            metadata_cache: Arc::new(Mutex::new(HashMap::new())),
            tile_cache: Arc::new(Mutex::new(TileCache::default())),
            host_policy: HostPolicy::new(&["nowcoast.noaa.gov"]),
        }
    }

    /// Fetches and caches product metadata.
    ///
    /// # Errors
    /// Returns [`WeatherError`] on fetch or parse failure.
    pub async fn get_metadata(&self, product: &str) -> Result<WeatherMetadata, WeatherError> {
        let spec = find_product_spec(product)
            .ok_or_else(|| WeatherError::ProductNotFound(product.to_string()))?;

        let now_ms = jiff::Timestamp::now().as_millisecond();

        if let Ok(cache) = self.metadata_cache.lock()
            && let Some(meta) = cache.get(product)
            && (now_ms - meta.fetched_at).max(0).cast_unsigned() <= spec.metadata_ttl_ms
        {
            return Ok(meta.clone());
        }

        let url_str = format!(
            "{BASE_URL}{}/ows?service=WMS&version=1.3.0&request=GetCapabilities",
            spec.service
        );
        let url = url::Url::parse(&url_str).map_err(|e| WeatherError::Fetch(e.to_string()))?;

        let options = FetchOptions {
            max_body_bytes: Some(MAX_METADATA_BYTES),
            timeout: Some(Duration::from_secs(12)),
            ..Default::default()
        };

        let response = self.fetcher.get(&url, &self.host_policy, &options).await?;
        let xml = String::from_utf8_lossy(&response.body);
        let parsed = parse_weather_capabilities(&xml, product, now_ms)?;

        if let Ok(mut cache) = self.metadata_cache.lock() {
            cache.insert(product.to_string(), parsed.clone());
        }

        Ok(parsed)
    }

    /// Fetches a WMS tile image (256x256).
    ///
    /// # Errors
    /// Returns [`WeatherError`] on fetch or image validation failure.
    pub async fn get_tile(
        &self,
        product: &str,
        time: &str,
        z: u32,
        x: u32,
        y: u32,
        size: u32,
    ) -> Result<Bytes, WeatherError> {
        let spec = find_product_spec(product)
            .ok_or_else(|| WeatherError::ProductNotFound(product.to_string()))?;

        if !["256", "512", "1024"].contains(&size.to_string().as_str()) {
            return Err(WeatherError::InvalidTile);
        }

        let bounds = weather_tile_bounds(z, x, y)?;
        let key = format!("{product}:{time}:tile:{size}:{z}/{x}/{y}");
        let now_ms = jiff::Timestamp::now().as_millisecond();

        if let Ok(mut cache) = self.tile_cache.lock()
            && let Some(b) = cache.get(&key, now_ms)
        {
            return Ok(b);
        }

        let url_str = format!(
            "{BASE_URL}{}/ows?service=WMS&version=1.1.1&request=GetMap&layers={}&styles={}&srs=EPSG:4326&bbox={},{},{},{}&width={}&height={}&format=image/png&transparent=true&time={}",
            spec.service,
            spec.layer,
            spec.style,
            bounds[0],
            bounds[1],
            bounds[2],
            bounds[3],
            size,
            size,
            time
        );

        let url = url::Url::parse(&url_str).map_err(|e| WeatherError::Fetch(e.to_string()))?;
        let options = FetchOptions {
            max_body_bytes: Some(size as usize * size as usize * 4 + 65_536),
            timeout: Some(Duration::from_secs(12)),
            ..Default::default()
        };

        let response = self.fetcher.get(&url, &self.host_policy, &options).await?;
        validate_png(&response.body, size, size)?;

        if let Ok(mut cache) = self.tile_cache.lock() {
            cache.insert(key, response.body.clone(), now_ms);
        }

        Ok(response.body)
    }

    /// Generates product manifest for frontend consumers.
    #[must_use]
    pub fn describe_manifest(
        &self,
        product: &str,
        meta: Option<&WeatherMetadata>,
    ) -> WeatherManifest {
        let spec = find_product_spec(product).unwrap_or(&PRODUCTS[0]);
        let time = meta.and_then(|m| m.times.last().cloned());
        let tile_template = time
            .as_ref()
            .map(|t| format!("/api/v1/tiles/weather/{product}/{t}/{{z}}/{{x}}/{{y}}.png"));

        WeatherManifest {
            schema_version: 1,
            product: product.to_string(),
            title: spec.title.to_string(),
            coverage: spec.coverage.to_string(),
            description: spec.description.to_string(),
            source: "NOAA nowCOAST".to_string(),
            attribution: spec.attribution.to_string(),
            bounds: meta.map(|m| m.bounds),
            times: meta.map_or_else(Vec::new, |m| m.times.clone()),
            latest: time.clone(),
            time: time.clone(),
            observed_at: time,
            fetched_at: meta.map(|m| m.fetched_at),
            stale: meta.is_none_or(|m| m.stale),
            unavailable: meta.is_none(),
            reason: if meta.is_none() {
                Some("Weather imagery unavailable".to_string())
            } else {
                None
            },
            tile_size: 256,
            max_level: 6,
            tiling_scheme: "geographic".to_string(),
            tile_template,
            image_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_CAPABILITIES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<WMS_Capabilities version="1.3.0">
  <Capability>
    <Layer>
      <Title>Observations</Title>
      <Layer>
        <Name>conus_base_reflectivity_mosaic</Name>
        <Title>Base Reflectivity</Title>
        <EX_GeographicBoundingBox>
          <westBoundLongitude>-130.0</westBoundLongitude>
          <eastBoundLongitude>-60.0</eastBoundLongitude>
          <southBoundLatitude>20.0</southBoundLatitude>
          <northBoundLatitude>55.0</northBoundLatitude>
        </EX_GeographicBoundingBox>
        <Dimension name="time" units="ISO8601" default="2026-10-21T14:00:00Z">
          2026-10-21T13:45:00Z,2026-10-21T13:50:00Z,2026-10-21T14:00:00Z
        </Dimension>
      </Layer>
    </Layer>
  </Capability>
</WMS_Capabilities>"#;

    #[test]
    fn parses_valid_capabilities_xml() {
        let now_ms = jiff::Timestamp::from_str("2026-10-21T14:05:00Z").unwrap().as_millisecond();
        let meta = parse_weather_capabilities(FIXTURE_CAPABILITIES, "radar", now_ms).unwrap();
        assert_eq!(meta.bounds.west, -130.0);
        assert_eq!(meta.bounds.east, -60.0);
        assert_eq!(meta.times.len(), 3);
        assert_eq!(meta.default_time, "2026-10-21T14:00:00Z");
    }

    #[test]
    fn rejects_xxe_attacks() {
        let xxe = r#"<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><WMS_Capabilities/>"#;
        assert_eq!(
            parse_weather_capabilities(xxe, "radar", 0).unwrap_err(),
            WeatherError::XxeAttack
        );
    }

    #[test]
    fn computes_geographictilingscheme_bounds() {
        // Level 0: 2 tiles x, 1 tile y
        let b0 = weather_tile_bounds(0, 0, 0).unwrap();
        assert_eq!(b0, [-180.0, -90.0, 0.0, 90.0]);

        let b1 = weather_tile_bounds(0, 1, 0).unwrap();
        assert_eq!(b1, [0.0, -90.0, 180.0, 90.0]);

        // Level 1: 4 tiles x, 2 tiles y
        let b2 = weather_tile_bounds(1, 0, 0).unwrap();
        assert_eq!(b2, [-180.0, 0.0, -90.0, 90.0]);

        assert!(weather_tile_bounds(7, 0, 0).is_err());
    }

    #[test]
    fn validates_png_signatures() {
        let mut good_png = vec![137, 80, 78, 71, 13, 10, 26, 10]; // Sig
        good_png.extend_from_slice(&13_u32.to_be_bytes()); // Chunk len
        good_png.extend_from_slice(b"IHDR"); // Chunk type
        good_png.extend_from_slice(&256_u32.to_be_bytes()); // Width
        good_png.extend_from_slice(&256_u32.to_be_bytes()); // Height
        good_png.extend_from_slice(&[8, 6, 0, 0, 0]); // Bit depth, color type, etc.
        good_png.extend_from_slice(&[0, 0, 0, 0]); // CRC

        assert!(validate_png(&good_png, 256, 256).is_ok());
        assert!(validate_png(&good_png, 512, 512).is_err()); // Dimension mismatch
        assert!(validate_png(&[0; 32], 256, 256).is_err()); // Corrupt header
    }
}
