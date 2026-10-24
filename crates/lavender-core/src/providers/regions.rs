//! Geographic regions provider: offline Natural Earth physical regions and island polygons.
//!
//! Provides point-in-polygon queries and name searches across named geographic regions.

use std::sync::Arc;

use lavender_domain::{BBox, GeoPoint};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

const DEFAULT_REGIONS_JSON: &str =
    include_str!("../../../../legacy/src/data/local_data/natural_earth/regions.json");

/// A geographic region record with boundary polygon and optional ISO country code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionRecord {
    pub id: String,
    pub name: String,
    pub iso_a2: Option<String>,
    pub polygon: Vec<GeoPoint>,
}

#[derive(Debug, Clone)]
struct RegionEntry {
    record: RegionRecord,
    bbox: BBox,
    area_deg2: f64,
}

/// An in-memory spatial index of geographic region polygons.
#[derive(Debug, Clone)]
pub struct RegionIndex {
    entries: Arc<[RegionEntry]>,
}

impl Default for RegionIndex {
    fn default() -> Self {
        Self::load_default()
    }
}

impl RegionIndex {
    /// Loads the bundled Natural Earth regions dataset.
    #[must_use]
    pub fn load_default() -> Self {
        Self::from_geojson_str(DEFAULT_REGIONS_JSON).unwrap_or_else(|_| Self::empty())
    }

    /// Creates an empty regions index.
    #[must_use]
    pub fn empty() -> Self {
        Self { entries: Arc::from([]) }
    }

    /// Builds the index from GeoJSON text.
    ///
    /// # Errors
    /// Returns an error if the JSON is malformed.
    pub fn from_geojson_str(geojson_str: &str) -> Result<Self, CoreError> {
        let root: RegionsJson = serde_json::from_str(geojson_str)
            .map_err(|err| CoreError::BadRequest(format!("malformed regions.json: {err}")))?;

        let mut entries = Vec::new();

        for feature in root.features {
            let base_slug = slugify(&feature.name);
            let iso_a2 = lookup_iso_a2(&feature.name).map(str::to_owned);
            let part_count = feature.polygons.len();

            for (part_idx, ring) in feature.polygons.into_iter().enumerate() {
                if ring.len() < 3 {
                    continue;
                }

                let polygon: Vec<GeoPoint> = ring
                    .into_iter()
                    .filter_map(|coord| {
                        let lon = coord.first().copied()?;
                        let lat = coord.get(1).copied()?;
                        if lon.is_finite() && lat.is_finite() {
                            GeoPoint::new(lat, lon).ok()
                        } else {
                            None
                        }
                    })
                    .collect();

                if polygon.len() < 3 {
                    continue;
                }

                let Some(bbox) = compute_bbox(&polygon) else {
                    continue;
                };

                let area_deg2 = polygon_area_deg2(&polygon);

                let id = if part_count == 1 {
                    base_slug.clone()
                } else {
                    format!("{base_slug}-{part_idx}")
                };

                let record = RegionRecord {
                    id,
                    name: feature.name.clone(),
                    iso_a2: iso_a2.clone(),
                    polygon,
                };

                entries.push(RegionEntry { record, bbox, area_deg2 });
            }
        }

        Ok(Self { entries: Arc::from(entries) })
    }

    /// Returns the number of indexed region polygon parts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if there are no regions in the index.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Finds the most specific region containing `(lon, lat)`.
    ///
    /// If multiple regions enclose the point, the one with the smallest polygon
    /// area is returned (e.g. `Great Britain` rather than `British Isles`).
    #[must_use]
    pub fn region_at(&self, lon: f64, lat: f64) -> Option<&RegionRecord> {
        if !lon.is_finite() || !lat.is_finite() {
            return None;
        }

        // 1. Exact point-in-polygon check
        if let Some(record) = self.find_containing(lon, lat) {
            return Some(record);
        }

        // 2. Coastal / harbor fallback for coordinates just offshore or in river mouths
        // (e.g. Lower Manhattan Battery Park / New York Harbor in simplified 10m vectors)
        let offsets = [
            (0.015, 0.0),
            (-0.015, 0.0),
            (0.0, 0.015),
            (0.0, -0.015),
            (0.02, 0.02),
            (-0.02, 0.02),
            (-0.02, -0.02),
            (0.02, -0.02),
        ];

        for (dx, dy) in offsets {
            if let Some(record) = self.find_containing(lon + dx, lat + dy) {
                return Some(record);
            }
        }

        None
    }

    fn find_containing(&self, lon: f64, lat: f64) -> Option<&RegionRecord> {
        let mut best: Option<(&RegionRecord, f64)> = None;

        for entry in self.entries.as_ref() {
            if !bbox_contains(&entry.bbox, lon, lat) {
                continue;
            }
            if point_in_polygon(lon, lat, &entry.record.polygon) {
                match best {
                    None => {
                        best = Some((&entry.record, entry.area_deg2));
                    }
                    Some((_, best_area)) if entry.area_deg2 < best_area => {
                        best = Some((&entry.record, entry.area_deg2));
                    }
                    _ => {}
                }
            }
        }

        best.map(|(record, _)| record)
    }

    /// Searches regions by name, ID, or ISO code.
    ///
    /// Matches are ranked by query relevance (exact match > prefix match > substring match)
    /// and deduplicated by region name to return each distinct region at most once.
    #[must_use]
    pub fn search_regions(&self, query: &str, limit: usize) -> Vec<&RegionRecord> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut scored: Vec<(&RegionEntry, u32)> = Vec::new();

        for entry in self.entries.as_ref() {
            let name_lower = entry.record.name.to_ascii_lowercase();
            let score = if name_lower == q {
                1000
            } else if name_lower.starts_with(&q) {
                500
            } else if name_lower.contains(&q) {
                200
            } else if entry.record.id.contains(&q) {
                100
            } else if entry.record.iso_a2.as_ref().is_some_and(|iso| iso.eq_ignore_ascii_case(&q)) {
                150
            } else {
                0
            };

            if score > 0 {
                scored.push((entry, score));
            }
        }

        // Sort by score descending, then by polygon area descending (largest part first)
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.area_deg2.total_cmp(&a.0.area_deg2)));

        // Deduplicate by region name
        let mut seen_names = std::collections::HashSet::new();
        let mut results = Vec::with_capacity(limit);

        for (entry, _) in scored {
            if seen_names.insert(&entry.record.name) {
                results.push(&entry.record);
                if results.len() >= limit {
                    break;
                }
            }
        }

        results
    }
}

/// Raycasting (even-odd rule) point-in-polygon test.
#[must_use]
pub fn point_in_polygon(lon: f64, lat: f64, polygon: &[GeoPoint]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let xi = polygon[i].lon;
        let yi = polygon[i].lat;
        let xj = polygon[j].lon;
        let yj = polygon[j].lat;

        if ((yi > lat) != (yj > lat)) && (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn bbox_contains(bbox: &BBox, lon: f64, lat: f64) -> bool {
    if lat < bbox.south || lat > bbox.north {
        return false;
    }
    if bbox.crosses_antimeridian() {
        lon >= bbox.west || lon <= bbox.east
    } else {
        lon >= bbox.west && lon <= bbox.east
    }
}

fn compute_bbox(polygon: &[GeoPoint]) -> Option<BBox> {
    let first = polygon.first()?;
    let mut west = first.lon;
    let mut east = first.lon;
    let mut south = first.lat;
    let mut north = first.lat;

    for pt in polygon {
        if pt.lon < west {
            west = pt.lon;
        }
        if pt.lon > east {
            east = pt.lon;
        }
        if pt.lat < south {
            south = pt.lat;
        }
        if pt.lat > north {
            north = pt.lat;
        }
    }

    BBox::new(west, south, east, north).ok()
}

fn polygon_area_deg2(polygon: &[GeoPoint]) -> f64 {
    let n = polygon.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        sum += polygon[i].lon * polygon[j].lat - polygon[j].lon * polygon[i].lat;
    }
    (sum / 2.0).abs()
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn lookup_iso_a2(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "great britain" | "united kingdom" => Some("GB"),
        "japan" => Some("JP"),
        "united states" | "united states of america" => Some("US"),
        "new zealand" => Some("NZ"),
        "madagascar" => Some("MG"),
        "iceland" => Some("IS"),
        "greenland" => Some("GL"),
        "cuba" => Some("CU"),
        "cyprus" => Some("CY"),
        "taiwan" => Some("TW"),
        "sri lanka" => Some("LK"),
        "jamaica" => Some("JM"),
        "fiji" => Some("FJ"),
        "malta" => Some("MT"),
        "bahamas" => Some("BS"),
        "barbados" => Some("BB"),
        "philippines" => Some("PH"),
        "indonesia" => Some("ID"),
        "papua new guinea" => Some("PG"),
        "solomon islands" => Some("SB"),
        "vanuatu" => Some("VU"),
        "samoa" => Some("WS"),
        "tonga" => Some("TO"),
        "mauritius" => Some("MU"),
        "seychelles" => Some("SC"),
        "comoros" => Some("KM"),
        "cape verde" => Some("CV"),
        "são tomé and príncipe" | "sao tome and principe" => Some("ST"),
        "trinidad and tobago" => Some("TT"),
        "dominica" => Some("DM"),
        "saint lucia" => Some("LC"),
        "grenada" => Some("GD"),
        _ => None,
    }
}

#[derive(Deserialize)]
struct RegionsJson {
    features: Vec<RegionFeatureJson>,
}

#[derive(Deserialize)]
struct RegionFeatureJson {
    name: String,
    #[serde(default)]
    polygons: Vec<Vec<Vec<f64>>>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assert_is_empty)]
    use super::*;

    #[test]
    fn loads_bundled_regions() {
        let index = RegionIndex::load_default();
        assert!(!index.is_empty());
        assert!(index.len() >= 1000);
    }

    #[test]
    fn point_in_polygon_london() {
        let index = RegionIndex::load_default();
        // London coordinates: lon -0.1276, lat 51.5074
        let region = index.region_at(-0.1276, 51.5074);
        assert!(region.is_some(), "London should return a region");
        let r = region.unwrap();
        assert!(
            r.name == "Great Britain" || r.name == "British Isles",
            "Expected Great Britain or British Isles, got {}",
            r.name
        );
        assert_eq!(r.iso_a2.as_deref(), Some("GB"));
    }

    #[test]
    fn point_in_polygon_tokyo() {
        let index = RegionIndex::load_default();
        // Tokyo coordinates: lon 139.6917, lat 35.6895
        let region = index.region_at(139.6917, 35.6895);
        assert!(region.is_some(), "Tokyo should return a region");
        let r = region.unwrap();
        assert!(
            r.name == "Honshü" || r.name == "Japan",
            "Expected Honshu or Japan, got {}",
            r.name
        );
    }

    #[test]
    fn point_in_polygon_new_york() {
        let index = RegionIndex::load_default();
        // New York (Queens / Long Island: -73.9, 40.72)
        let region = index.region_at(-73.9, 40.72);
        assert!(region.is_some(), "New York should return a region");
        let r = region.unwrap();
        assert!(
            r.name == "Long I." || r.name == "North America",
            "Expected Long I. or North America, got {}",
            r.name
        );
    }

    #[test]
    fn search_regions_finds_matches() {
        let index = RegionIndex::load_default();
        let results = index.search_regions("Great Britain", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Great Britain");

        let japan_results = index.search_regions("Japan", 5);
        assert!(!japan_results.is_empty());
        assert_eq!(japan_results[0].name, "Japan");
    }
}
