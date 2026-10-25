//! Administrative divisions provider: offline Natural Earth admin-1
//! (states/provinces) polygons, packed as delta-encoded integer rings exactly
//! like the boundary lines. Point-in-polygon lookup, name search and bbox
//! queries serve the interface's division drill-down (LB-152); the data is
//! upstream's own `states_provinces.json`, read from the pristine `legacy/`
//! mirror like the physical regions dataset.

use std::sync::Arc;

use lavender_domain::{BBox, GeoPoint};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::providers::regions::point_in_polygon;

const DEFAULT_DIVISIONS_JSON: &str = include_str!("../../data/natural_earth/states_provinces.json");

/// Delta rings store three-decimal degrees as integers.
const COORD_SCALE: f64 = 1000.0;
pub const DEFAULT_SEARCH_LIMIT: usize = 20;
const MAX_SEARCH_LIMIT: usize = 100;

/// Source attribution for the admin-1 divisions dataset.
pub const DIVISIONS_ATTRIBUTION: &str =
    "Made with Natural Earth (public domain admin-1 states and provinces; de facto point of view).";

/// An admin-1 division: a state, province or equivalent unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DivisionRecord {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    /// Admin-0 parent country name, as Natural Earth spells it.
    pub country: String,
    /// Full ISO 3166-2 code when assigned, for example `AF-BDS`.
    pub iso: Option<String>,
    /// Natural Earth's type label, for example `Province` or `State`.
    pub kind: Option<String>,
    pub polygons: Vec<Vec<GeoPoint>>,
    pub bbox: BBox,
}

/// The on-disk packing: either a GeoJSON-like object with features or an array of features.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawDivisions {
    Object { features: Vec<RawDivision> },
    List(Vec<RawDivision>),
}

#[derive(Debug, Deserialize)]
struct RawDivision {
    name: String,
    #[serde(rename = "nameEn")]
    name_en: Option<String>,
    country: String,
    iso: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Parts → rings → delta-encoded integers. Most parts carry one outer ring.
    #[serde(default)]
    polygons: Vec<Vec<Vec<i64>>>,
}

#[derive(Debug, Clone)]
struct DivisionEntry {
    record: DivisionRecord,
    bbox: BBox,
}

/// An in-memory index of admin-1 divisions.
#[derive(Debug, Clone)]
pub struct DivisionIndex {
    entries: Arc<[DivisionEntry]>,
}

impl Default for DivisionIndex {
    fn default() -> Self {
        Self::load_default()
    }
}

impl DivisionIndex {
    /// Loads the bundled Natural Earth admin-1 dataset.
    #[must_use]
    pub fn load_default() -> Self {
        Self::from_json_str(DEFAULT_DIVISIONS_JSON).unwrap_or_else(|_| Self::empty())
    }

    #[must_use]
    pub fn empty() -> Self {
        Self { entries: Arc::from([]) }
    }

    /// Builds the index from the packed JSON text.
    ///
    /// # Errors
    /// Returns an error if the JSON is malformed (a feature with a bad ring is
    /// skipped, not fatal, so one bad row upstream cannot take the feed down).
    pub fn from_json_str(json: &str) -> Result<Self, CoreError> {
        let raw: RawDivisions = serde_json::from_str(json)
            .map_err(|e| CoreError::Upstream(format!("divisions dataset malformed: {e}")))?;
        let features = match raw {
            RawDivisions::Object { features } | RawDivisions::List(features) => features,
        };
        let mut entries = Vec::with_capacity(features.len());
        for (index, feature) in features.into_iter().enumerate() {
            let mut polygons = Vec::with_capacity(feature.polygons.len());
            let mut bbox =
                BBox { west: f64::MAX, south: f64::MAX, east: f64::MIN, north: f64::MIN };
            for part in &feature.polygons {
                for ring in part {
                    let Some(points) = decode_ring(ring) else {
                        continue;
                    };
                    for p in &points {
                        bbox.west = bbox.west.min(p.lon);
                        bbox.east = bbox.east.max(p.lon);
                        bbox.south = bbox.south.min(p.lat);
                        bbox.north = bbox.north.max(p.lat);
                    }
                    polygons.push(points);
                }
            }
            if polygons.is_empty() {
                continue;
            }
            let id =
                feature.iso.clone().unwrap_or_else(|| format!("division-{index}")).to_lowercase();
            entries.push(DivisionEntry {
                record: DivisionRecord {
                    id,
                    name: feature.name,
                    name_en: feature.name_en,
                    country: feature.country,
                    iso: feature.iso,
                    kind: feature.kind,
                    polygons,
                    bbox,
                },
                bbox,
            });
        }
        Ok(Self { entries: Arc::from(entries) })
    }

    /// Divisions whose polygons contain `(lon, lat)`, smallest first so the
    /// most specific unit wins for the inspector.
    #[must_use]
    pub fn divisions_at(&self, lon: f64, lat: f64) -> Vec<&DivisionRecord> {
        let mut hits: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                let b = &e.bbox;
                lon >= b.west && lon <= b.east && lat >= b.south && lat <= b.north
            })
            .filter(|e| e.record.polygons.iter().any(|ring| point_in_polygon(lon, lat, ring)))
            .collect();
        hits.sort_by(|a, b| {
            let da = (a.bbox.east - a.bbox.west) * (a.bbox.north - a.bbox.south);
            let db = (b.bbox.east - b.bbox.west) * (b.bbox.north - b.bbox.south);
            da.total_cmp(&db)
        });
        hits.into_iter().map(|e| &e.record).collect()
    }

    /// Case-insensitive search over division and country names and ISO codes.
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<&DivisionRecord> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let limit = limit.clamp(1, MAX_SEARCH_LIMIT);
        self.entries
            .iter()
            .filter(|e| {
                e.record.name.to_lowercase().contains(&q)
                    || e.record.name_en.as_deref().is_some_and(|ne| ne.to_lowercase().contains(&q))
                    || e.record.country.to_lowercase().contains(&q)
                    || e.record.iso.as_deref().is_some_and(|iso| iso.to_lowercase().contains(&q))
            })
            .take(limit)
            .map(|e| &e.record)
            .collect()
    }

    /// Divisions whose bounding boxes intersect `bbox` (viewport prefetch).
    #[must_use]
    pub fn query_bbox(&self, bbox: &BBox, limit: usize) -> Vec<&DivisionRecord> {
        self.entries
            .iter()
            .filter(|e| {
                let b = &e.bbox;
                b.west <= bbox.east
                    && b.east >= bbox.west
                    && b.south <= bbox.north
                    && b.north >= bbox.south
            })
            .take(limit.clamp(1, 500))
            .map(|e| &e.record)
            .collect()
    }

    /// Record count, for health and tests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Decodes one delta-encoded integer ring into geographic points. Rings with
/// fewer than three points or absurd coordinates are refused (None).
#[allow(clippy::cast_precision_loss)]
fn decode_ring(ring: &[i64]) -> Option<Vec<GeoPoint>> {
    if ring.len() < 6 || !ring.len().is_multiple_of(2) {
        return None;
    }
    let mut points = Vec::with_capacity(ring.len() / 2);
    let mut x = 0_i64;
    let mut y = 0_i64;
    let mut i = 0;
    while i < ring.len() {
        x += ring[i];
        y += ring[i + 1];
        let lon = x as f64 / COORD_SCALE;
        let lat = y as f64 / COORD_SCALE;
        if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
            return None;
        }
        points.push(GeoPoint { lat, lon, alt_m: None });
        i += 2;
    }
    if points.len() < 3 {
        return None;
    }
    Some(points)
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use super::*;

    #[test]
    fn bundled_dataset_loads_with_real_coverage() {
        let index = DivisionIndex::load_default();
        assert!(index.len() > 4_000, "admin-1 count {}", index.len());
        // Every record has a name, a country and at least one ring with bbox.
        assert!(
            index.entries.iter().all(|e| !e.record.name.is_empty() && !e.record.country.is_empty())
        );
        assert!(index.entries.iter().all(|e| e.record.bbox.east >= e.record.bbox.west));
    }

    #[test]
    fn point_lookup_finds_the_expected_division() {
        let index = DivisionIndex::load_default();
        // Natural Earth admin-1 names are local spellings: Bavaria is "Bayern".
        let bavaria = index.divisions_at(11.5, 48.1);
        assert!(
            bavaria.iter().any(|d| d.country == "Germany" && d.name == "Bayern"),
            "hits: {:?}",
            bavaria.iter().map(|d| (&d.name, &d.country)).take(4).collect::<Vec<_>>()
        );
        let texas = index.divisions_at(-99.5, 31.2);
        assert!(texas.first().is_some_and(|d| d.name == "Texas"));
    }

    #[test]
    fn search_matches_names_countries_and_iso() {
        let index = DivisionIndex::load_default();
        assert!(
            index
                .search("bavaria", DEFAULT_SEARCH_LIMIT)
                .iter()
                .any(|d| d.name == "Bayern" || d.name_en.as_deref() == Some("Bavaria"))
        );
        assert!(index.search("texas", DEFAULT_SEARCH_LIMIT).iter().any(|d| d.name == "Texas"));
        assert!(!index.search("AF-BDS", DEFAULT_SEARCH_LIMIT).is_empty());
        assert!(index.search("   ", DEFAULT_SEARCH_LIMIT).is_empty());
    }

    #[test]
    fn bbox_query_and_geometry_sanity() {
        let index = DivisionIndex::load_default();
        let alps = BBox { west: 6.0, south: 45.0, east: 14.0, north: 48.0 };
        let hits = index.query_bbox(&alps, 50);
        assert!(!hits.is_empty());
        for d in &hits {
            assert!(d.polygons.iter().all(|ring| {
                ring.len() >= 3 && ring.iter().all(|p| p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0)
            }));
        }
        // Determinism: two loads answer identically.
        assert_eq!(index.len(), DivisionIndex::load_default().len());
    }

    #[test]
    fn malformed_rings_are_skipped_not_fatal() {
        let json = r#"[
            {"name":"Good","country":"X","iso":"X-GD","type":"State","polygons":[[[1000,2000,10,0,0,10,-10,0]]]},
            {"name":"Bad","country":"Y","iso":null,"type":null,"polygons":[[[1,2]]]}
        ]"#;
        let index = DivisionIndex::from_json_str(json).expect("parses");
        assert_eq!(index.len(), 1);
        assert!(index.search("Good", 5).iter().any(|d| d.name == "Good"));
    }
}
