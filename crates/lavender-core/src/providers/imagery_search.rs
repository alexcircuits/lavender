//! Recent satellite imagery search and relay provider (NASA CMR and GIBS).
//!
//! Provides:
//! - Search over NASA CMR (Common Metadata Repository) for HLS (Harmonized Landsat and
//!   Sentinel-2) image granules by bounding box and temporal window.
//! - Granule grouping by UTC calendar day with cloud coverage statistics and spatial footprint union.
//! - VIIRS true-color daily overview candidate synthesis.
//! - Candidate ranking prioritizing whole-box clear-sky imagery over partial coverage.
//! - GIBS WMTS template descriptors and Worldview Snapshots (WVS) thumbnail probing/relaying.
//! - Strict geographic bounds validation (dateline, polar limit, maximum side length).

#![allow(clippy::expect_used, clippy::cast_possible_truncation, clippy::similar_names)]

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use bytes::Bytes;
use jiff::Timestamp;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::fetch::{FetchOptions, Fetcher, HostPolicy};
use lavender_domain::geo::BBox;

/// Hard limit on the side of a query box, in kilometres.
pub const MAX_BOX_SIDE_KM: f64 = 1000.0;

/// Limit of Web-Mercator GIBS tiles (±85.0511° latitude).
pub const MERCATOR_LAT_LIMIT: f64 = 85.0511;

/// Maximum cloud cover percentage considered clear-sky.
pub const MAX_CLOUD_FOR_CLEAR: f64 = 20.0;

/// Default search lookback in days.
pub const DEFAULT_CATALOG_DAYS: u32 = 30;

/// Maximum search lookback in days.
pub const MAX_CATALOG_DAYS: u32 = 60;

/// Max records to fetch per collection page.
pub const CMR_PAGE_SIZE: usize = 200;

/// Cap on total CMR records per query.
pub const CMR_MAX_RECORDS: usize = 2000;

/// Approximate kilometres per degree of latitude on Earth.
pub const KM_PER_DEG_LAT: f64 = 111.32;

/// Number of milliseconds in one UTC calendar day.
const DAY_MS: i64 = 86_400_000;

/// CMR search endpoint.
const CMR_ENDPOINT: &str = "https://cmr.earthdata.nasa.gov/search/granules.umm_json";

/// Worldview Snapshots endpoint.
const WVS_ENDPOINT: &str = "https://wvs.earthdata.nasa.gov/api/v1/snapshot";

/// Maximum size in bytes for a fetched thumbnail image.
const MAX_THUMBNAIL_BYTES: usize = 4 * 1024 * 1024;

/// In-memory cache TTL for search results.
const CACHE_TTL_SECS: i64 = 600;

/// In-memory cache capacity.
const CACHE_CAPACITY: usize = 128;

static RE_DATE_FORMAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex"));

static RE_ISO_FORMAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?$").expect("valid regex")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductId {
    S30,
    L30,
    Viirs,
}

impl ProductId {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::S30 => "S30",
            Self::L30 => "L30",
            Self::Viirs => "VIIRS",
        }
    }

    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "S30" => Some(Self::S30),
            "L30" => Some(Self::L30),
            "VIIRS" => Some(Self::Viirs),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProductSpec {
    pub id: ProductId,
    pub label: &'static str,
    pub name: &'static str,
    pub sensor: &'static str,
    pub resolution_m: u32,
    pub gibs_layer: &'static str,
    pub max_level: u32,
    pub format: &'static str,
    pub cmr_collection: Option<&'static str>,
    pub overview: bool,
}

pub const PRODUCTS: &[ProductSpec] = &[
    ProductSpec {
        id: ProductId::S30,
        label: "HLS S30",
        name: "Sentinel-2",
        sensor: "Sentinel-2 via HLS",
        resolution_m: 30,
        gibs_layer: "HLS_S30_Nadir_BRDF_Adjusted_Reflectance",
        max_level: 12,
        format: "png",
        cmr_collection: Some("C2021957295-LPCLOUD"),
        overview: false,
    },
    ProductSpec {
        id: ProductId::L30,
        label: "HLS L30",
        name: "Landsat 8/9",
        sensor: "Landsat 8/9 via HLS",
        resolution_m: 30,
        gibs_layer: "HLS_L30_Nadir_BRDF_Adjusted_Reflectance",
        max_level: 12,
        format: "png",
        cmr_collection: Some("C2021957657-LPCLOUD"),
        overview: false,
    },
    ProductSpec {
        id: ProductId::Viirs,
        label: "VIIRS",
        name: "Daily overview",
        sensor: "VIIRS NOAA-21",
        resolution_m: 250,
        gibs_layer: "VIIRS_NOAA21_CorrectedReflectance_TrueColor",
        max_level: 9,
        format: "jpg",
        cmr_collection: None,
        overview: true,
    },
];

#[must_use]
pub fn find_product(id: &str) -> Option<&'static ProductSpec> {
    PRODUCTS.iter().find(|p| p.id.as_str() == id)
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImageryError {
    #[error("Box edges must be finite numbers")]
    InvalidEdges,
    #[error("Longitudes must be within ±180°")]
    InvalidLongitude,
    #[error("Select one side of the dateline")]
    DatelineCrossed,
    #[error("Imagery stops at ±{0:.2}° latitude", MERCATOR_LAT_LIMIT)]
    PolarLimit,
    #[error("Box must enclose an area")]
    DegenerateBox,
    #[error("{0}")]
    BoxTooLarge(String),
    #[error("Invalid clock: {0}")]
    InvalidClock(String),
    #[error("Unknown imagery product: {0}")]
    UnknownProduct(String),
    #[error("No CMR collection for product: {0}")]
    NoCmrCollection(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Image too large ({size} bytes, limit {limit} bytes)")]
    ImageTooLarge { size: usize, limit: usize },
    #[error("Invalid image format or magic bytes")]
    InvalidImageFormat,
    #[error("Thumbnail not found or unavailable")]
    ThumbnailUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl ValidatedBox {
    /// Validates and normalises a bounding box against recent imagery operational rules.
    ///
    /// # Errors
    /// Returns [`ImageryError`] if the box crosses the antimeridian, exceeds polar limits,
    /// is degenerate, or exceeds [`MAX_BOX_SIDE_KM`].
    pub fn validate(bbox: &BBox) -> Result<Self, ImageryError> {
        let (west, south, east, north) = (bbox.west, bbox.south, bbox.east, bbox.north);
        if !west.is_finite() || !south.is_finite() || !east.is_finite() || !north.is_finite() {
            return Err(ImageryError::InvalidEdges);
        }
        if west.abs() > 180.0 || east.abs() > 180.0 {
            return Err(ImageryError::InvalidLongitude);
        }
        if west > east {
            return Err(ImageryError::DatelineCrossed);
        }
        let (norm_south, norm_north) = if south <= north { (south, north) } else { (north, south) };

        if norm_south.abs() > MERCATOR_LAT_LIMIT || norm_north.abs() > MERCATOR_LAT_LIMIT {
            return Err(ImageryError::PolarLimit);
        }
        if (east - west).abs() < 1e-9 || (norm_north - norm_south).abs() < 1e-9 {
            return Err(ImageryError::DegenerateBox);
        }

        let mid_lat = f64::midpoint(norm_south, norm_north);
        let width_km = (east - west) * KM_PER_DEG_LAT * mid_lat.to_radians().cos();
        let height_km = (norm_north - norm_south) * KM_PER_DEG_LAT;

        if width_km > MAX_BOX_SIDE_KM || height_km > MAX_BOX_SIDE_KM {
            let max_side = width_km.max(height_km);
            let msg = format!(
                "Box is {} km wide · limit {} km",
                format_km(max_side),
                format_km(MAX_BOX_SIDE_KM)
            );
            return Err(ImageryError::BoxTooLarge(msg));
        }

        Ok(Self { west, south: norm_south, east, north: norm_north })
    }

    /// Computes the dimensions in kilometres.
    #[must_use]
    pub fn side_km(&self) -> (f64, f64) {
        let mid_lat = f64::midpoint(self.south, self.north);
        let width_km = (self.east - self.west) * KM_PER_DEG_LAT * mid_lat.to_radians().cos();
        let height_km = (self.north - self.south) * KM_PER_DEG_LAT;
        (width_km, height_km)
    }

    /// Computes centre coordinate in degrees (lon, lat).
    #[must_use]
    pub fn centre(&self) -> (f64, f64) {
        (f64::midpoint(self.west, self.east), f64::midpoint(self.south, self.north))
    }
}

fn format_km(km: f64) -> String {
    let rounded = km.round() as i64;
    let s = rounded.to_string();
    let mut result = String::new();
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateAvailability {
    Present,
    Empty,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageLevel {
    Full,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Granule {
    pub id: String,
    pub product: String,
    pub time_start: String,
    pub time_end: String,
    pub cloud: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageryCandidate {
    pub key: String,
    pub product: String,
    pub day: String,
    pub cloud: Option<CloudRange>,
    pub time_range: Option<TimeRange>,
    pub availability: CandidateAvailability,
    pub coverage: CoverageLevel,
    pub granule_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub granules: Vec<Granule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RankReason {
    Clear,
    Cloudy,
    Partial,
    Overview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedLatest {
    pub candidate: Option<ImageryCandidate>,
    pub reason: Option<RankReason>,
    pub certain: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageryAttribution {
    pub credit: String,
    pub licence: String,
    pub statement: String,
}

impl Default for ImageryAttribution {
    fn default() -> Self {
        Self {
            credit: "NASA EOSDIS / GIBS / LP DAAC".to_owned(),
            licence: "Public Domain / NASA Open Data".to_owned(),
            statement: "We acknowledge the use of imagery from the NASA Worldview Snapshots and GIBS applications, part of the NASA Earth Observing System Data and Information System (EOSDIS).".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageryErrorDetail {
    pub product: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentImageryResponse {
    pub candidates: Vec<ImageryCandidate>,
    pub ranked_latest: RankedLatest,
    pub tile_templates: HashMap<String, String>,
    pub attribution: ImageryAttribution,
    pub truncated: bool,
    pub errors: Vec<ImageryErrorDetail>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailProbeResponse {
    pub product: String,
    pub day: String,
    pub data_present: bool,
    pub acquisition_time: Option<String>,
}

/// Generates a GIBS WMTS template for a product and day.
///
/// # Errors
/// Returns [`ImageryError::UnknownProduct`] if the product is not recognised.
pub fn gibs_template(product: &str, day: &str) -> Result<String, ImageryError> {
    let spec =
        find_product(product).ok_or_else(|| ImageryError::UnknownProduct(product.to_owned()))?;
    Ok(format!(
        "https://gibs-{{s}}.earthdata.nasa.gov/wmts/epsg3857/best/{}/default/{}/GoogleMapsCompatible_Level{}/{{z}}/{{y}}/{{x}}.{}",
        spec.gibs_layer, day, spec.max_level, spec.format
    ))
}

/// Constructs a Worldview Snapshots snapshot URL for an area and day.
///
/// # Errors
/// Returns [`ImageryError`] if the product is unknown or bbox invalid.
pub fn wvs_snapshot_url(
    product: &str,
    day: &str,
    bbox: &ValidatedBox,
    width: u32,
    height: u32,
    format: &str,
) -> Result<String, ImageryError> {
    let spec =
        find_product(product).ok_or_else(|| ImageryError::UnknownProduct(product.to_owned()))?;
    Ok(format!(
        "{}?REQUEST=GetSnapshot&LAYERS={}&CRS=EPSG:4326&TIME={}&BBOX={:.6},{:.6},{:.6},{:.6}&WIDTH={}&HEIGHT={}&FORMAT={}",
        WVS_ENDPOINT,
        spec.gibs_layer,
        day,
        bbox.south,
        bbox.west,
        bbox.north,
        bbox.east,
        width,
        height,
        format
    ))
}

/// Parses CMR `granules.umm_json` response body.
#[must_use]
pub fn parse_cmr_umm(json: &serde_json::Value, product: &str) -> (Vec<Granule>, usize) {
    let mut granules = Vec::new();
    let hits = json.get("hits").and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;

    let Some(items) = json.get("items").and_then(serde_json::Value::as_array) else {
        return (granules, hits);
    };

    for item in items {
        let meta = item.get("meta");
        let Some(umm) = item.get("umm") else {
            continue;
        };

        let time_range = umm.get("TemporalExtent").and_then(|t| t.get("RangeDateTime"));
        let time_start = time_range
            .and_then(|r| r.get("BeginningDateTime"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| RE_ISO_FORMAT.is_match(s));

        let time_start = match time_start {
            Some(t) => t.to_owned(),
            None => continue,
        };

        let time_end = time_range
            .and_then(|r| r.get("EndingDateTime"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| RE_ISO_FORMAT.is_match(s))
            .map_or_else(|| time_start.clone(), str::to_owned);

        let id = meta
            .and_then(|m| m.get("concept-id"))
            .or_else(|| umm.get("GranuleUR"))
            .or_else(|| meta.and_then(|m| m.get("native-id")))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();

        let cloud = umm
            .get("AdditionalAttributes")
            .and_then(serde_json::Value::as_array)
            .and_then(|attrs| {
                attrs.iter().find(|a| {
                    a.get("Name").and_then(serde_json::Value::as_str) == Some("CLOUD_COVERAGE")
                })
            })
            .and_then(|attr| attr.get("Values").and_then(serde_json::Value::as_array))
            .and_then(|vals| vals.first())
            .and_then(|v| {
                if let Some(n) = v.as_f64() {
                    Some(n)
                } else if let Some(s) = v.as_str() {
                    s.parse::<f64>().ok()
                } else {
                    None
                }
            })
            .filter(|c| c.is_finite());

        let footprint = extract_footprint(umm);

        granules.push(Granule {
            id,
            product: product.to_owned(),
            time_start,
            time_end,
            cloud,
            footprint,
        });
    }

    let actual_hits = if hits > 0 { hits } else { granules.len() };
    (granules, actual_hits)
}

fn extract_footprint(umm: &serde_json::Value) -> Option<Vec<[f64; 2]>> {
    let geom = umm
        .get("SpatialExtent")
        .and_then(|s| s.get("HorizontalSpatialDomain"))
        .and_then(|h| h.get("Geometry"))?;

    if let Some(poly_points) = geom
        .get("GPolygons")
        .and_then(serde_json::Value::as_array)
        .and_then(|p| p.first())
        .and_then(|p| p.get("Boundary"))
        .and_then(|b| b.get("Points"))
        .and_then(serde_json::Value::as_array)
    {
        let ring: Vec<[f64; 2]> = poly_points
            .iter()
            .filter_map(|pt| {
                let lon = pt.get("Longitude").and_then(serde_json::Value::as_f64)?;
                let lat = pt.get("Latitude").and_then(serde_json::Value::as_f64)?;
                if lon.is_finite() && lat.is_finite() { Some([lon, lat]) } else { None }
            })
            .collect();
        if ring.len() >= 3 {
            return Some(ring);
        }
    }

    if let Some(rect) =
        geom.get("BoundingRectangles").and_then(serde_json::Value::as_array).and_then(|r| r.first())
    {
        let west = rect.get("WestBoundingCoordinate").and_then(serde_json::Value::as_f64)?;
        let south = rect.get("SouthBoundingCoordinate").and_then(serde_json::Value::as_f64)?;
        let east = rect.get("EastBoundingCoordinate").and_then(serde_json::Value::as_f64)?;
        let north = rect.get("NorthBoundingCoordinate").and_then(serde_json::Value::as_f64)?;
        if west.is_finite() && south.is_finite() && east.is_finite() && north.is_finite() {
            return Some(vec![[west, south], [east, south], [east, north], [west, north]]);
        }
    }

    None
}

/// Point-in-polygon raycasting test using Jordan curve algorithm.
#[must_use]
pub fn point_in_polygon(lon: f64, lat: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut j = ring.len().saturating_sub(1);
    for i in 0..ring.len() {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        let crosses = (yi > lat) != (yj > lat)
            && lon < (xj - xi) * (lat - yi) / (yj - yi + f64::EPSILON) + xi;
        if crosses {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Evaluates spatial coverage of candidate granules over the query bounding box.
#[must_use]
pub fn coverage_for(granules: &[Granule], bbox: &ValidatedBox) -> CoverageLevel {
    let footprints: Vec<&Vec<[f64; 2]>> = granules
        .iter()
        .filter_map(|g| g.footprint.as_ref())
        .filter(|ring| ring.len() >= 3)
        .collect();

    if footprints.is_empty() {
        return CoverageLevel::Unknown;
    }

    let samples = [
        (bbox.west, bbox.south),
        (bbox.east, bbox.south),
        (bbox.east, bbox.north),
        (bbox.west, bbox.north),
        (f64::midpoint(bbox.west, bbox.east), f64::midpoint(bbox.south, bbox.north)),
    ];

    let all_inside = samples
        .iter()
        .all(|&(lon, lat)| footprints.iter().any(|ring| point_in_polygon(lon, lat, ring)));

    if all_inside { CoverageLevel::Full } else { CoverageLevel::Partial }
}

/// Extracts calendar day (`YYYY-MM-DD`) from an ISO-8601 UTC timestamp.
#[must_use]
pub fn utc_day_from_iso(iso: &str) -> Option<String> {
    if iso.len() >= 10 && RE_DATE_FORMAT.is_match(&iso[0..10]) {
        Some(iso[0..10].to_owned())
    } else {
        None
    }
}

/// Groups individual granules into candidate image days.
#[must_use]
pub fn group_granules_by_day(granules: Vec<Granule>, bbox: &ValidatedBox) -> Vec<ImageryCandidate> {
    let mut buckets: HashMap<(String, String), Vec<Granule>> = HashMap::new();

    for g in granules {
        if let Some(day) = utc_day_from_iso(&g.time_start) {
            buckets.entry((g.product.clone(), day)).or_default().push(g);
        }
    }

    let mut candidates: Vec<ImageryCandidate> = buckets
        .into_iter()
        .map(|((product, day), group)| {
            let key = format!("{product}:{day}");
            let clouds: Vec<f64> = group.iter().filter_map(|g| g.cloud).collect();
            let cloud = if clouds.is_empty() {
                None
            } else {
                let min = clouds.iter().copied().fold(f64::INFINITY, f64::min);
                let max = clouds.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                Some(CloudRange { min, max })
            };

            let starts: Vec<&str> = group.iter().map(|g| g.time_start.as_str()).collect();
            let ends: Vec<&str> = group.iter().map(|g| g.time_end.as_str()).collect();
            let time_range = if starts.is_empty() || ends.is_empty() {
                None
            } else {
                let min_start = starts.iter().copied().min().unwrap_or("").to_owned();
                let max_end = ends.iter().copied().max().unwrap_or("").to_owned();
                Some(TimeRange { start: min_start, end: max_end })
            };

            let coverage = coverage_for(&group, bbox);
            let count = group.len();

            ImageryCandidate {
                key,
                product,
                day,
                cloud,
                time_range,
                availability: CandidateAvailability::Present,
                coverage,
                granule_count: count,
                granules: group,
            }
        })
        .collect();

    sort_candidates(&mut candidates);
    candidates
}

/// Generates synthetic VIIRS daily overview candidates for the requested lookback window.
#[must_use]
pub fn viirs_candidates(now_timestamp: Timestamp, days: u32) -> Vec<ImageryCandidate> {
    let mut result = Vec::new();
    let now_epoch_ms = now_timestamp.as_millisecond();
    let days_clamped = days.min(MAX_CATALOG_DAYS);

    for i in 0..days_clamped {
        let day_ms = now_epoch_ms - i64::from(i) * DAY_MS;
        if let Ok(ts) = Timestamp::from_millisecond(day_ms) {
            let iso = ts.to_string();
            if let Some(day) = utc_day_from_iso(&iso) {
                result.push(ImageryCandidate {
                    key: format!("VIIRS:{day}"),
                    product: "VIIRS".to_owned(),
                    day,
                    cloud: None,
                    time_range: None,
                    availability: CandidateAvailability::Unknown,
                    coverage: CoverageLevel::Full,
                    granule_count: 0,
                    granules: Vec::new(),
                });
            }
        }
    }
    result
}

fn product_rank(product: &str) -> u8 {
    match product {
        "S30" => 0,
        "L30" => 1,
        "VIIRS" => 2,
        _ => 3,
    }
}

/// Sorts candidates: newest day first; for the same day, S30 > L30 > VIIRS.
pub fn sort_candidates(candidates: &mut [ImageryCandidate]) {
    candidates.sort_by(|a, b| {
        b.day.cmp(&a.day).then_with(|| product_rank(&a.product).cmp(&product_rank(&b.product)))
    });
}

/// Merges candidate lists, removing duplicate keys (first occurrence wins).
#[must_use]
pub fn merge_candidates(lists: Vec<Vec<ImageryCandidate>>) -> Vec<ImageryCandidate> {
    let mut by_key: HashMap<String, ImageryCandidate> = HashMap::new();
    let mut ordered = Vec::new();

    for list in lists {
        for candidate in list {
            if !by_key.contains_key(&candidate.key) {
                by_key.insert(candidate.key.clone(), candidate.clone());
                ordered.push(candidate);
            }
        }
    }

    sort_candidates(&mut ordered);
    ordered
}

/// Ranks the latest and best candidate for the "START HERE" view.
#[must_use]
pub fn rank_latest(candidates: &[ImageryCandidate], truncated: bool) -> RankedLatest {
    let mut sorted = candidates.to_vec();
    sort_candidates(&mut sorted);

    let is_present = |c: &ImageryCandidate| c.availability != CandidateAvailability::Empty;
    let is_hls = |c: &ImageryCandidate| {
        (c.product == "S30" || c.product == "L30") && is_present(c) && c.granule_count > 0
    };
    let covers_box = |c: &ImageryCandidate| c.coverage != CoverageLevel::Partial;
    let is_clear_sky = |c: &ImageryCandidate| {
        c.granules.iter().all(|g| g.cloud.is_some_and(|cl| cl <= MAX_CLOUD_FOR_CLEAR))
    };

    // Tier 1: HLS whole-box clear sky
    if let Some(c) = sorted.iter().find(|c| is_hls(c) && covers_box(c) && is_clear_sky(c)) {
        return RankedLatest {
            candidate: Some(c.clone()),
            reason: Some(RankReason::Clear),
            certain: !truncated,
        };
    }

    // Tier 2: HLS whole-box cloudy
    if let Some(c) = sorted.iter().find(|c| is_hls(c) && covers_box(c)) {
        return RankedLatest {
            candidate: Some(c.clone()),
            reason: Some(RankReason::Cloudy),
            certain: !truncated,
        };
    }

    // Tier 3: HLS partial clear sky
    if let Some(c) = sorted.iter().find(|c| is_hls(c) && is_clear_sky(c)) {
        return RankedLatest {
            candidate: Some(c.clone()),
            reason: Some(RankReason::Partial),
            certain: !truncated,
        };
    }

    // Tier 4: HLS partial (any)
    if let Some(c) = sorted.iter().find(|c| is_hls(c)) {
        return RankedLatest {
            candidate: Some(c.clone()),
            reason: Some(RankReason::Partial),
            certain: !truncated,
        };
    }

    // Tier 5: VIIRS overview
    if let Some(c) = sorted.iter().find(|c| c.product == "VIIRS" && is_present(c)) {
        return RankedLatest {
            candidate: Some(c.clone()),
            reason: Some(RankReason::Overview),
            certain: !truncated,
        };
    }

    RankedLatest { candidate: None, reason: None, certain: !truncated }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    west: i32,
    south: i32,
    east: i32,
    north: i32,
    days: u32,
}

impl CacheKey {
    fn from(bbox: &ValidatedBox, days: u32) -> Self {
        Self {
            west: (bbox.west * 100_000.0).round() as i32,
            south: (bbox.south * 100_000.0).round() as i32,
            east: (bbox.east * 100_000.0).round() as i32,
            north: (bbox.north * 100_000.0).round() as i32,
            days,
        }
    }
}

#[derive(Debug, Clone)]
struct CacheEntry {
    response: RecentImageryResponse,
    cached_at: Timestamp,
}

#[derive(Debug)]
pub struct ImagerySearchProvider {
    fetcher: Arc<Fetcher>,
    host_policy: HostPolicy,
    cache: Mutex<HashMap<CacheKey, CacheEntry>>,
}

impl ImagerySearchProvider {
    #[must_use]
    pub fn new(fetcher: Arc<Fetcher>) -> Self {
        let policy = HostPolicy::new(&[
            "cmr.earthdata.nasa.gov",
            "wvs.earthdata.nasa.gov",
            "gibs-a.earthdata.nasa.gov",
            "gibs-b.earthdata.nasa.gov",
            "gibs-c.earthdata.nasa.gov",
        ]);
        Self { fetcher, host_policy: policy, cache: Mutex::new(HashMap::new()) }
    }

    /// Searches NASA CMR for HLS imagery granules and merges with VIIRS daily overviews.
    ///
    /// # Errors
    /// Returns [`ImageryError`] if the bounding box or parameters are invalid.
    pub async fn search(
        &self,
        bbox: &BBox,
        days: u32,
        clock: Option<Timestamp>,
    ) -> Result<RecentImageryResponse, ImageryError> {
        let validated = ValidatedBox::validate(bbox)?;
        let days = days.clamp(1, MAX_CATALOG_DAYS);
        let key = CacheKey::from(&validated, days);
        let now = clock.unwrap_or_else(Timestamp::now);

        // Check cache
        {
            let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = cache.get(&key) {
                let age_s = now.duration_since(entry.cached_at).as_secs();
                if age_s < CACHE_TTL_SECS {
                    return Ok(entry.response.clone());
                }
                cache.remove(&key);
            }
        }

        let now_ms = now.as_millisecond();
        let end_iso = now.to_string();
        let start_ms = now_ms - (i64::from(days) - 1) * DAY_MS;
        let start_ts = Timestamp::from_millisecond(start_ms)
            .map_err(|e| ImageryError::InvalidClock(e.to_string()))?;
        let start_day = utc_day_from_iso(&start_ts.to_string())
            .ok_or_else(|| ImageryError::InvalidClock("invalid start timestamp".to_owned()))?;
        let start_iso = format!("{start_day}T00:00:00Z");

        // Concurrent search for S30 and L30
        let (s30_res, l30_res) = tokio::join!(
            self.fetch_cmr_granules("S30", &validated, &start_iso, &end_iso),
            self.fetch_cmr_granules("L30", &validated, &start_iso, &end_iso)
        );

        let mut all_granules = Vec::new();
        let mut errors = Vec::new();
        let mut truncated = false;

        match s30_res {
            Ok((granules, is_trunc)) => {
                all_granules.extend(granules);
                truncated |= is_trunc;
            }
            Err(e) => {
                errors
                    .push(ImageryErrorDetail { product: "S30".to_owned(), message: e.to_string() });
            }
        }

        match l30_res {
            Ok((granules, is_trunc)) => {
                all_granules.extend(granules);
                truncated |= is_trunc;
            }
            Err(e) => {
                errors
                    .push(ImageryErrorDetail { product: "L30".to_owned(), message: e.to_string() });
            }
        }

        let hls_candidates = group_granules_by_day(all_granules, &validated);
        let viirs = viirs_candidates(now, days);
        let merged = merge_candidates(vec![hls_candidates, viirs]);
        let ranked = rank_latest(&merged, truncated);

        let mut tile_templates = HashMap::new();
        let today_str = utc_day_from_iso(&end_iso).unwrap_or_else(|| "default".to_owned());
        for p in PRODUCTS {
            if let Ok(tmpl) = gibs_template(p.id.as_str(), &today_str) {
                tile_templates.insert(p.id.as_str().to_owned(), tmpl);
            }
        }

        let response = RecentImageryResponse {
            candidates: merged,
            ranked_latest: ranked,
            tile_templates,
            attribution: ImageryAttribution::default(),
            truncated,
            errors,
        };

        // Cache update
        {
            let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if cache.len() >= CACHE_CAPACITY {
                cache.clear();
            }
            cache.insert(key, CacheEntry { response: response.clone(), cached_at: now });
        }

        Ok(response)
    }

    async fn fetch_cmr_granules(
        &self,
        product: &str,
        bbox: &ValidatedBox,
        start_iso: &str,
        end_iso: &str,
    ) -> Result<(Vec<Granule>, bool), ImageryError> {
        let spec = find_product(product)
            .ok_or_else(|| ImageryError::UnknownProduct(product.to_owned()))?;
        let concept_id =
            spec.cmr_collection.ok_or_else(|| ImageryError::NoCmrCollection(product.to_owned()))?;

        let current_url = format!(
            "{}?collection_concept_id={}&bounding_box={:.6},{:.6},{:.6},{:.6}&temporal={},{}&sort_key=-start_date&page_size={}",
            CMR_ENDPOINT,
            concept_id,
            bbox.west,
            bbox.south,
            bbox.east,
            bbox.north,
            start_iso,
            end_iso,
            CMR_PAGE_SIZE
        );

        let mut granules = Vec::new();
        let mut hits = 0;
        let mut cursor: Option<String> = None;

        loop {
            let mut opts = FetchOptions {
                max_body_bytes: Some(8 * 1024 * 1024),
                timeout: Some(Duration::from_secs(15)),
                ..FetchOptions::default()
            };

            if let Some(c) = &cursor
                && let Ok(val) = reqwest::header::HeaderValue::from_str(c)
            {
                opts.extra_headers
                    .push((reqwest::header::HeaderName::from_static("cmr-search-after"), val));
            }

            let url = Url::parse(&current_url).map_err(|e| ImageryError::Network(e.to_string()))?;
            let resp = self
                .fetcher
                .get(&url, &self.host_policy, &opts)
                .await
                .map_err(|e| ImageryError::Network(e.to_string()))?;

            if resp.status != 200 {
                return Err(ImageryError::Network(format!("HTTP {} from CMR", resp.status)));
            }

            let json: serde_json::Value = serde_json::from_slice(&resp.body)
                .map_err(|e| ImageryError::Parse(e.to_string()))?;

            let (page_granules, page_hits) = parse_cmr_umm(&json, product);
            let page_len = page_granules.len();
            granules.extend(page_granules);
            hits = hits.max(page_hits);

            cursor = resp
                .headers
                .get("CMR-Search-After")
                .or_else(|| resp.headers.get("cmr-search-after"))
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);

            if cursor.is_none()
                || page_len < CMR_PAGE_SIZE
                || granules.len() >= CMR_MAX_RECORDS
                || granules.len() >= hits
            {
                break;
            }
        }

        let is_truncated = hits > granules.len();
        Ok((granules, is_truncated))
    }

    /// Probes Worldview Snapshots to verify whether imagery pixels are present for a given day and area.
    ///
    /// # Errors
    /// Returns [`ImageryError`] if the network request fails or parameters are invalid.
    pub async fn probe_thumbnail(
        &self,
        product: &str,
        day: &str,
        bbox: &BBox,
    ) -> Result<ThumbnailProbeResponse, ImageryError> {
        let validated = ValidatedBox::validate(bbox)?;
        let url_str = wvs_snapshot_url(product, day, &validated, 64, 64, "image/png")?;
        let url = Url::parse(&url_str).map_err(|e| ImageryError::Network(e.to_string()))?;

        let opts = FetchOptions {
            max_body_bytes: Some(128 * 1024),
            timeout: Some(Duration::from_secs(8)),
            ..FetchOptions::default()
        };

        let resp = self
            .fetcher
            .get(&url, &self.host_policy, &opts)
            .await
            .map_err(|e| ImageryError::Network(e.to_string()))?;

        let data_present = resp
            .headers
            .get("Data-Present")
            .or_else(|| resp.headers.get("data-present"))
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| !v.eq_ignore_ascii_case("false"));

        let acquisition_time = resp
            .headers
            .get("Acquisition-Time")
            .or_else(|| resp.headers.get("acquisition-time"))
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        Ok(ThumbnailProbeResponse {
            product: product.to_owned(),
            day: day.to_owned(),
            data_present,
            acquisition_time,
        })
    }

    /// Relays a Worldview Snapshots thumbnail image with size and signature validation.
    ///
    /// # Errors
    /// Returns [`ImageryError`] if the image fails validation or upstream fails.
    pub async fn fetch_thumbnail(
        &self,
        product: &str,
        day: &str,
        bbox: &BBox,
        size: u32,
    ) -> Result<(Bytes, Option<String>, bool), ImageryError> {
        let validated = ValidatedBox::validate(bbox)?;
        let size_clamped = size.clamp(64, 1024);
        let format = if product == "VIIRS" { "image/jpeg" } else { "image/png" };

        let url_str =
            wvs_snapshot_url(product, day, &validated, size_clamped, size_clamped, format)?;
        let url = Url::parse(&url_str).map_err(|e| ImageryError::Network(e.to_string()))?;

        let opts = FetchOptions {
            max_body_bytes: Some(MAX_THUMBNAIL_BYTES),
            timeout: Some(Duration::from_secs(12)),
            ..FetchOptions::default()
        };

        let resp = self
            .fetcher
            .get(&url, &self.host_policy, &opts)
            .await
            .map_err(|e| ImageryError::Network(e.to_string()))?;

        if resp.status != 200 {
            return Err(ImageryError::ThumbnailUnavailable);
        }

        let data_present = resp
            .headers
            .get("Data-Present")
            .or_else(|| resp.headers.get("data-present"))
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| !v.eq_ignore_ascii_case("false"));

        let acquisition_time = resp
            .headers
            .get("Acquisition-Time")
            .or_else(|| resp.headers.get("acquisition-time"))
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        // Validate image magic bytes
        if resp.body.len() < 4 {
            return Err(ImageryError::InvalidImageFormat);
        }
        let is_png = resp.body.starts_with(&[137, 80, 78, 71]);
        let is_jpeg = resp.body.starts_with(&[0xff, 0xd8, 0xff]);

        if !is_png && !is_jpeg {
            return Err(ImageryError::InvalidImageFormat);
        }

        Ok((resp.body, acquisition_time, data_present))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_BOX: BBox = BBox { west: -97.8, south: 30.2, east: -97.7, north: 30.3 };

    #[test]
    fn validates_bounding_boxes() {
        assert!(ValidatedBox::validate(&TEST_BOX).is_ok());

        // Dateline crossing
        let dl = BBox { west: 179.0, south: 0.0, east: -179.0, north: 1.0 };
        assert_eq!(ValidatedBox::validate(&dl).unwrap_err(), ImageryError::DatelineCrossed);

        // Polar limit
        let polar = BBox { west: 0.0, south: 84.0, east: 0.5, north: 85.06 };
        assert_eq!(ValidatedBox::validate(&polar).unwrap_err(), ImageryError::PolarLimit);

        // Degenerate
        let degen = BBox { west: 10.0, south: 10.0, east: 10.0, north: 11.0 };
        assert_eq!(ValidatedBox::validate(&degen).unwrap_err(), ImageryError::DegenerateBox);

        // Too large
        let tall = BBox { west: 0.0, south: 0.0, east: 0.1, north: 10.0 };
        assert!(matches!(ValidatedBox::validate(&tall).unwrap_err(), ImageryError::BoxTooLarge(_)));
    }

    #[test]
    fn parses_cmr_umm_json() {
        let fixture = serde_json::json!({
            "hits": 2,
            "items": [
                {
                    "meta": { "concept-id": "G1-LPCLOUD" },
                    "umm": {
                        "TemporalExtent": {
                            "RangeDateTime": {
                                "BeginningDateTime": "2026-09-18T17:12:01.000Z",
                                "EndingDateTime": "2026-09-18T17:12:31.000Z"
                            }
                        },
                        "AdditionalAttributes": [
                            { "Name": "CLOUD_COVERAGE", "Values": ["12.5"] }
                        ],
                        "SpatialExtent": {
                            "HorizontalSpatialDomain": {
                                "Geometry": {
                                    "GPolygons": [
                                        {
                                            "Boundary": {
                                                "Points": [
                                                    { "Longitude": -98.0, "Latitude": 30.0 },
                                                    { "Longitude": -97.0, "Latitude": 30.0 },
                                                    { "Longitude": -97.0, "Latitude": 31.0 },
                                                    { "Longitude": -98.0, "Latitude": 31.0 }
                                                ]
                                            }
                                        }
                                    ]
                                }
                            }
                        }
                    }
                },
                {
                    "meta": { "concept-id": "G2-LPCLOUD" },
                    "umm": {
                        "TemporalExtent": {
                            "RangeDateTime": {
                                "BeginningDateTime": "2026-09-18T17:20:00Z"
                            }
                        },
                        "AdditionalAttributes": [
                            { "Name": "CLOUD_COVERAGE", "Values": ["4.0"] }
                        ]
                    }
                }
            ]
        });

        let (granules, hits) = parse_cmr_umm(&fixture, "S30");
        assert_eq!(hits, 2);
        assert_eq!(granules.len(), 2);
        assert_eq!(granules[0].id, "G1-LPCLOUD");
        assert_eq!(granules[0].cloud, Some(12.5));
        assert!(granules[0].footprint.is_some());
        assert_eq!(granules[1].cloud, Some(4.0));
        assert!(granules[1].footprint.is_none());
    }

    #[test]
    fn groups_granules_and_computes_coverage() {
        let val_box = ValidatedBox::validate(&TEST_BOX).unwrap();
        let ring = vec![[-98.0, 30.0], [-97.0, 30.0], [-97.0, 31.0], [-98.0, 31.0]];

        let granules = vec![
            Granule {
                id: "g1".into(),
                product: "S30".into(),
                time_start: "2026-09-18T17:12:01.000Z".into(),
                time_end: "2026-09-18T17:12:31.000Z".into(),
                cloud: Some(10.0),
                footprint: Some(ring),
            },
            Granule {
                id: "g2".into(),
                product: "S30".into(),
                time_start: "2026-09-18T17:15:00.000Z".into(),
                time_end: "2026-09-18T17:15:30.000Z".into(),
                cloud: Some(18.0),
                footprint: None,
            },
        ];

        let grouped = group_granules_by_day(granules, &val_box);
        assert_eq!(grouped.len(), 1);
        let cand = &grouped[0];
        assert_eq!(cand.key, "S30:2026-09-18");
        assert_eq!(cand.cloud, Some(CloudRange { min: 10.0, max: 18.0 }));
        assert_eq!(cand.coverage, CoverageLevel::Full);
        assert_eq!(cand.granule_count, 2);
    }

    #[test]
    fn ranks_latest_candidate_priority() {
        let _val_box = ValidatedBox::validate(&TEST_BOX).unwrap();
        let ring = vec![[-98.0, 30.0], [-97.0, 30.0], [-97.0, 31.0], [-98.0, 31.0]];

        let clear_cand = ImageryCandidate {
            key: "S30:2026-09-18".into(),
            product: "S30".into(),
            day: "2026-09-18".into(),
            cloud: Some(CloudRange { min: 5.0, max: 15.0 }),
            time_range: None,
            availability: CandidateAvailability::Present,
            coverage: CoverageLevel::Full,
            granule_count: 1,
            granules: vec![Granule {
                id: "g1".into(),
                product: "S30".into(),
                time_start: "2026-09-18T12:00:00Z".into(),
                time_end: "2026-09-18T12:05:00Z".into(),
                cloud: Some(15.0),
                footprint: Some(ring),
            }],
        };

        let cloudy_cand = ImageryCandidate {
            key: "L30:2026-09-19".into(),
            product: "L30".into(),
            day: "2026-09-19".into(),
            cloud: Some(CloudRange { min: 45.0, max: 50.0 }),
            time_range: None,
            availability: CandidateAvailability::Present,
            coverage: CoverageLevel::Full,
            granule_count: 1,
            granules: vec![Granule {
                id: "g2".into(),
                product: "L30".into(),
                time_start: "2026-09-19T12:00:00Z".into(),
                time_end: "2026-09-19T12:05:00Z".into(),
                cloud: Some(50.0),
                footprint: None,
            }],
        };

        // Clear day 2026-09-18 beats newer cloudy day 2026-09-19
        let ranked = rank_latest(&[cloudy_cand.clone(), clear_cand.clone()], false);
        assert_eq!(ranked.candidate.as_ref().map(|c| c.key.as_str()), Some("S30:2026-09-18"));
        assert_eq!(ranked.reason, Some(RankReason::Clear));

        // When only cloudy exists, it wins with Cloudy reason
        let ranked_cloudy = rank_latest(&[cloudy_cand], false);
        assert_eq!(
            ranked_cloudy.candidate.as_ref().map(|c| c.key.as_str()),
            Some("L30:2026-09-19")
        );
        assert_eq!(ranked_cloudy.reason, Some(RankReason::Cloudy));
    }

    #[test]
    fn generates_gibs_and_wvs_urls() {
        let val_box = ValidatedBox::validate(&TEST_BOX).unwrap();
        let gibs = gibs_template("S30", "2026-09-18").unwrap();
        assert!(gibs.contains("HLS_S30_Nadir_BRDF_Adjusted_Reflectance"));
        assert!(gibs.contains("GoogleMapsCompatible_Level12/{z}/{y}/{x}.png"));

        let wvs = wvs_snapshot_url("S30", "2026-09-18", &val_box, 256, 256, "image/png").unwrap();
        assert!(wvs.contains("CRS=EPSG:4326&TIME=2026-09-18"));
        assert!(wvs.contains("BBOX=30.200000,-97.800000,30.300000,-97.700000"));
        assert!(wvs.contains("WIDTH=256&HEIGHT=256&FORMAT=image/png"));
    }
}
