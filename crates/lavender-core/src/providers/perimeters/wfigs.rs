//! Strict, bounded reading of WFIGS current-perimeter GeoJSON query pages.
//!
//! A page that is not a feature collection (or is an ArcGIS error document) is rejected
//! whole. Inside a valid page each feature stands alone: one with a missing, malformed
//! or over-budget shape is skipped and counted, never the whole snapshot (ArcGIS sends
//! `geometry: null` when generalisation removes a very small polygon).

use std::collections::HashSet;
use std::fmt;

use jiff::{SignedDuration, Timestamp};
use lavender_domain::text::clean_opt;
use lavender_domain::{BBox, FirePerimeter, GeoPoint, PerimeterDetail};
use lavender_geo::polygon::{self, Position};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use url::Url;

use super::{largest_first, vertices};
use crate::providers::{ProviderError, lenient};

pub const SOURCE_NAME: &str = "NIFC WFIGS";

/// Every attribute the incident card shows, and nothing else.
const OUT_FIELDS: &str = "poly_IncidentName,attr_UniqueFireIdentifier,attr_IncidentSize,\
attr_PercentContained,attr_POOState,attr_IncidentTypeCategory,attr_FireDiscoveryDateTime,\
poly_DateCurrent,attr_FireCause,attr_FireBehaviorGeneral,attr_TotalIncidentPersonnel,\
attr_POOCounty,attr_EstimatedCostToDate,attr_IncidentComplexityLevel,attr_CpxName";
/// Server-side generalisation of about 20 m, the finest outline Lavender keeps.
const MAX_ALLOWABLE_OFFSET_DEG: &str = "0.0002";
/// Five decimals of a degree is about one metre.
const GEOMETRY_PRECISION: &str = "5";

/// Features per request (the service allows up to 2000).
pub const PAGE_SIZE: usize = 1_000;
/// Pages followed while the service reports `exceededTransferLimit`.
pub const MAX_PAGES: usize = 5;
pub const MAX_PAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_POLYGONS_PER_FEATURE: usize = 2_000;
const MAX_RINGS_PER_POLYGON: usize = 500;
/// Bounds simplification work per ring (Douglas–Peucker is quadratic at worst). The
/// largest ring recorded on 2026-10-15 (a 343 000-acre fire) had 5 701 positions.
const MAX_RING_VERTICES: usize = 50_000;
const MAX_FEATURE_VERTICES: usize = 100_000;
/// Outline positions kept across all pages of one snapshot (about 16 MB of coordinates);
/// past it the smallest fires are left out (see [`keep_largest`]).
pub const MAX_SNAPSHOT_VERTICES: usize = 1_000_000;
/// Later timestamps than this are treated as clock errors and dropped.
const MAX_CLOCK_SKEW: SignedDuration = SignedDuration::from_hours(1);
/// 1990-01-01T00:00:00Z: earlier "dates" are placeholders, not fire history.
const EARLIEST_MS: i64 = 631_152_000_000;

/// The query for one page of current perimeters, `offset` features in.
///
/// # Errors
/// Fails when the configured layer root cannot be joined with `query`.
pub fn query_url(layer: &Url, offset: usize) -> Result<Url, ProviderError> {
    let mut url = layer
        .join("query")
        .map_err(|_| ProviderError::Unconfigured("invalid WFIGS layer root".to_owned()))?;
    url.query_pairs_mut()
        .append_pair("where", "1=1")
        .append_pair("outFields", OUT_FIELDS)
        .append_pair("maxAllowableOffset", MAX_ALLOWABLE_OFFSET_DEG)
        .append_pair("geometryPrecision", GEOMETRY_PRECISION)
        .append_pair("outSR", "4326")
        .append_pair("orderByFields", "OBJECTID")
        .append_pair("resultOffset", &offset.to_string())
        .append_pair("resultRecordCount", &PAGE_SIZE.to_string())
        .append_pair("f", "geojson");
    Ok(url)
}

/// What one page contributed.
#[derive(Debug, Default)]
pub struct Page {
    /// Valid perimeters at full detail (level 0), in page order.
    pub perimeters: Vec<FirePerimeter>,
    /// Features skipped: missing, malformed or oversized shape, or a repeated id.
    pub dropped: usize,
    /// Features the page listed, valid or not (the next page's offset step).
    pub features: usize,
    /// Whether the service has more features after this page.
    pub more: bool,
}

/// State shared by the pages of one snapshot: the fire ids already seen.
#[derive(Debug, Default)]
pub struct Snapshot {
    ids: HashSet<String>,
}

/// When the fires' outlines hold more than `budget` positions in all, keeps the largest
/// fires (largest first) up to the first that no longer fits, and says how many smaller
/// ones were left out. Fires within the budget are returned as they are.
#[must_use]
pub fn keep_largest(mut fires: Vec<FirePerimeter>, budget: usize) -> (Vec<FirePerimeter>, usize) {
    if fires.iter().map(vertices).sum::<usize>() <= budget {
        return (fires, 0);
    }
    fires.sort_by(largest_first);
    let mut used = 0_usize;
    let fitting = fires
        .iter()
        .take_while(|fire| {
            used += vertices(fire);
            used <= budget
        })
        .count();
    let left_out = fires.len() - fitting;
    fires.truncate(fitting);
    (fires, left_out)
}

fn invalid() -> ProviderError {
    ProviderError::parse(SOURCE_NAME, &"not a bounded GeoJSON feature collection")
}

/// Reads one query page.
///
/// # Errors
/// Rejects oversized or malformed documents, ArcGIS error documents (for example
/// rate-limit answers sent with HTTP 200) and pages larger than requested.
pub fn parse_page(
    body: &[u8],
    now: Timestamp,
    snapshot: &mut Snapshot,
) -> Result<Page, ProviderError> {
    if body.len() > MAX_PAGE_BYTES {
        return Err(invalid());
    }
    let document: RawCollection = serde_json::from_slice(body).map_err(|_| invalid())?;
    if let Some(error) = document.error {
        // Only the numeric code is reported: messages are upstream text.
        let code = error.code.map_or_else(|| "without a code".to_owned(), |c| format!("{c}"));
        return Err(ProviderError::Unavailable(format!("{SOURCE_NAME} answered error {code}.")));
    }
    let Some(features) = document.features else { return Err(invalid()) };
    if document.kind.as_deref() != Some("FeatureCollection") {
        return Err(invalid());
    }
    let more = document.exceeded_transfer_limit == Some(true)
        || document.properties.and_then(|p| p.exceeded_transfer_limit) == Some(true);
    let mut page = Page { features: features.len(), more, ..Page::default() };
    for feature in features {
        match feature.and_then(|f| normalize(f, now, snapshot)) {
            Some(perimeter) => page.perimeters.push(perimeter),
            None => page.dropped += 1,
        }
    }
    Ok(page)
}

fn normalize(raw: RawFeature, now: Timestamp, snapshot: &mut Snapshot) -> Option<FirePerimeter> {
    if raw.kind != "Feature" {
        return None;
    }
    let p = raw.properties;
    let id = p.unique_id.as_deref().and_then(fire_id).or_else(|| raw.id.map(|n| n.to_string()))?;
    let polygons = polygons(raw.geometry?)?;
    if snapshot.ids.contains(&id) {
        return None;
    }
    let area = |rings: &Vec<Vec<Position>>| polygon::signed_area(&rings[0]).abs();
    let largest = polygons.iter().max_by(|a, b| area(a).total_cmp(&area(b)))?;
    let [lon, lat] = polygon::vertex_mean(&largest[0])?;
    let anchor = GeoPoint::new(lat, lon).ok()?;
    let [west, south, east, north] = polygon::extent(polygons.iter().flat_map(|p| &p[0]))?;
    let extent = BBox::new(west, south, east, north).ok()?;
    snapshot.ids.insert(id.clone());
    Some(FirePerimeter {
        id,
        name: clean_opt(p.name.as_deref(), 80),
        acres: number(p.acres, 0.0, 10_000_000.0),
        contained_pct: number(p.contained_pct, 0.0, 100.0),
        state: clean_opt(p.state.as_deref(), 8),
        category: clean_opt(p.category.as_deref(), 8),
        discovered_at: time(p.discovered_ms, now),
        updated_at: time(p.updated_ms, now),
        cause: clean_opt(p.cause.as_deref(), 40),
        behavior: clean_opt(p.behavior.as_deref(), 40),
        personnel: head_count(p.personnel),
        county: clean_opt(p.county.as_deref(), 100),
        cost_to_date_usd: number(p.cost_to_date, 0.0, 1e11),
        complexity: clean_opt(p.complexity.as_deref(), 40),
        complex_name: clean_opt(p.complex_name.as_deref(), 80),
        anchor,
        extent,
        inciweb_url: None,
        detail: PerimeterDetail { level: 0, tolerance_m: 0.0 },
        polygons,
    })
}

/// WFIGS ids look like `2026-CALPF-002271`: short, ASCII letters, digits and dashes.
fn fire_id(raw: &str) -> Option<String> {
    let id = raw.trim();
    (!id.is_empty() && id.len() <= 40 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .then(|| id.to_owned())
}

fn number(raw: Option<f64>, low: f64, high: f64) -> Option<f64> {
    raw.filter(|n| n.is_finite() && (low..=high).contains(n))
}

fn head_count(raw: Option<f64>) -> Option<u32> {
    let count = number(raw, 0.0, 100_000.0).filter(|n| n.fract() == 0.0)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // A whole number 0–100 000.
    Some(count as u32)
}

/// ArcGIS dates are epoch milliseconds.
fn time(raw: Option<f64>, now: Timestamp) -> Option<Timestamp> {
    // NaN and infinities fail the whole-number test.
    let ms = raw.filter(|ms| ms.fract() == 0.0 && ms.abs() < 1e15)?;
    #[allow(clippy::cast_possible_truncation)] // A whole number well inside i64.
    let ms = ms as i64;
    let latest = now.checked_add(MAX_CLOCK_SKEW).ok()?.as_millisecond();
    (EARLIEST_MS..=latest).contains(&ms).then(|| Timestamp::from_millisecond(ms).ok()).flatten()
}

/// Validates and cleans a `Polygon` or `MultiPolygon`: every ring closed, within coordinate
/// range and within the size caps. Rings without area are dropped (a polygon whose
/// exterior has none goes with it); a feature left with nothing has no usable shape.
fn polygons(geometry: RawGeometry) -> Option<Vec<Vec<Vec<Position>>>> {
    let raw = match geometry {
        RawGeometry::Polygon { coordinates } => vec![coordinates],
        RawGeometry::MultiPolygon { coordinates } => coordinates,
    };
    if raw.is_empty() || raw.len() > MAX_POLYGONS_PER_FEATURE {
        return None;
    }
    let mut vertices = 0;
    let mut cleaned_polygons = Vec::with_capacity(raw.len());
    for rings in &raw {
        if rings.is_empty() || rings.len() > MAX_RINGS_PER_POLYGON {
            return None;
        }
        let mut cleaned_rings: Vec<Vec<Position>> = Vec::with_capacity(rings.len());
        for (index, ring) in rings.iter().enumerate() {
            vertices += ring.len();
            if ring.len() > MAX_RING_VERTICES || vertices > MAX_FEATURE_VERTICES {
                return None;
            }
            let positions: Vec<Position> =
                ring.iter().map(|p| position(p)).collect::<Option<_>>()?;
            if positions.len() < 4 || positions.first() != positions.last() {
                return None;
            }
            let exterior = index == 0;
            match polygon::clean_ring(&positions, exterior) {
                Some(clean) if exterior || !cleaned_rings.is_empty() => cleaned_rings.push(clean),
                _ => {}
            }
        }
        if !cleaned_rings.is_empty() {
            cleaned_polygons.push(cleaned_rings);
        }
    }
    (!cleaned_polygons.is_empty()).then_some(cleaned_polygons)
}

/// `[longitude, latitude]`, optionally followed by an ignored altitude.
fn position(raw: &[f64]) -> Option<Position> {
    let (&[lon, lat] | &[lon, lat, _]) = raw else { return None };
    (lon.is_finite() && lat.is_finite() && lon.abs() <= 180.0 && lat.abs() <= 90.0)
        .then_some([lon, lat])
}

#[derive(Deserialize)]
struct RawCollection {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "features")]
    features: Option<Vec<Option<RawFeature>>>,
    #[serde(rename = "exceededTransferLimit")]
    exceeded_transfer_limit: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    properties: Option<RawCollectionProperties>,
    #[serde(default, deserialize_with = "lenient")]
    error: Option<RawError>,
}

#[derive(Deserialize)]
struct RawCollectionProperties {
    #[serde(rename = "exceededTransferLimit", default, deserialize_with = "lenient")]
    exceeded_transfer_limit: Option<bool>,
}

#[derive(Deserialize)]
struct RawError {
    #[serde(default, deserialize_with = "lenient")]
    code: Option<i64>,
}

#[derive(Deserialize)]
struct RawFeature {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "lenient")]
    id: Option<u64>,
    properties: RawProperties,
    geometry: Option<RawGeometry>,
}

#[derive(Deserialize)]
struct RawProperties {
    #[serde(rename = "attr_UniqueFireIdentifier", default, deserialize_with = "lenient")]
    unique_id: Option<String>,
    #[serde(rename = "poly_IncidentName", default, deserialize_with = "lenient")]
    name: Option<String>,
    #[serde(rename = "attr_IncidentSize", default, deserialize_with = "lenient")]
    acres: Option<f64>,
    #[serde(rename = "attr_PercentContained", default, deserialize_with = "lenient")]
    contained_pct: Option<f64>,
    #[serde(rename = "attr_POOState", default, deserialize_with = "lenient")]
    state: Option<String>,
    #[serde(rename = "attr_IncidentTypeCategory", default, deserialize_with = "lenient")]
    category: Option<String>,
    #[serde(rename = "attr_FireDiscoveryDateTime", default, deserialize_with = "lenient")]
    discovered_ms: Option<f64>,
    #[serde(rename = "poly_DateCurrent", default, deserialize_with = "lenient")]
    updated_ms: Option<f64>,
    #[serde(rename = "attr_FireCause", default, deserialize_with = "lenient")]
    cause: Option<String>,
    #[serde(rename = "attr_FireBehaviorGeneral", default, deserialize_with = "lenient")]
    behavior: Option<String>,
    #[serde(rename = "attr_TotalIncidentPersonnel", default, deserialize_with = "lenient")]
    personnel: Option<f64>,
    #[serde(rename = "attr_POOCounty", default, deserialize_with = "lenient")]
    county: Option<String>,
    #[serde(rename = "attr_EstimatedCostToDate", default, deserialize_with = "lenient")]
    cost_to_date: Option<f64>,
    #[serde(rename = "attr_IncidentComplexityLevel", default, deserialize_with = "lenient")]
    complexity: Option<String>,
    #[serde(rename = "attr_CpxName", default, deserialize_with = "lenient")]
    complex_name: Option<String>,
}

/// GeoJSON geometries Lavender accepts; any other type rejects the feature.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum RawGeometry {
    Polygon { coordinates: Vec<Vec<Vec<f64>>> },
    MultiPolygon { coordinates: Vec<Vec<Vec<Vec<f64>>>> },
}

/// Reads the `features` array one feature at a time, so a malformed feature becomes
/// `None` instead of failing the page, and only one feature's JSON tree is held at once.
fn features<'de, D>(deserializer: D) -> Result<Option<Vec<Option<RawFeature>>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Features;

    impl<'de> Visitor<'de> for Features {
        type Value = Vec<Option<RawFeature>>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an array of GeoJSON features")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(value) = seq.next_element::<serde_json::Value>()? {
                if out.len() >= PAGE_SIZE {
                    return Err(serde::de::Error::custom("more features than requested"));
                }
                out.push(serde_json::from_value(value).ok());
            }
            Ok(out)
        }
    }

    deserializer.deserialize_seq(Features).map(Some)
}
