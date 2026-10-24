//! Administrative and territorial boundary lines provider.
//!
//! Sourced from Natural Earth public domain 50m administrative boundaries:
//! - International land boundaries (`InternationalLand`)
//! - Disputed boundary lines and lines of control (`DisputedLine`)
//! - Breakaway and contested territories/regions (`BreakawayArea`)
//! - Maritime indicative lines (`MaritimeIndicative`)
//!
//! Provides in-memory R-tree spatial indexing (`rstar`), viewport bounding box filtering,
//! per-zoom Douglas-Peucker polyline simplification, and class filtering.

use std::sync::Arc;

use lavender_domain::{BBox, BoundaryClass, BoundaryFeature, GeoPoint};
use rstar::{AABB, RTree, RTreeObject};
use serde::Deserialize;

use crate::error::CoreError;

const DEFAULT_BOUNDARIES_JSON: &str = include_str!("../../data/natural_earth/boundaries.json");

/// Source attribution for Natural Earth boundaries.
pub const BOUNDARIES_ATTRIBUTION: &str =
    "Made with Natural Earth (public domain admin-0 boundary datasets; de facto point of view).";

/// Spatial index object wrapping feature index and bounding envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BoundarySpatialItem {
    index: usize,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for BoundarySpatialItem {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Raw feature as stored in the bundled `boundaries.json`.
#[derive(Deserialize)]
struct RawBoundaryFeature {
    id: String,
    class: BoundaryClass,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    sovereign: Option<String>,
    #[serde(default)]
    claimants: Vec<String>,
    #[serde(default)]
    note: Option<String>,
    lines: Vec<Vec<[f64; 2]>>,
    bbox: [f64; 4],
}

#[derive(Deserialize)]
struct RawBoundaryPack {
    features: Vec<RawBoundaryFeature>,
}

/// In-memory spatial store for administrative and territorial boundary features.
#[derive(Debug, Clone)]
pub struct BoundaryStore {
    features: Arc<[BoundaryFeature]>,
    spatial_index: Arc<RTree<BoundarySpatialItem>>,
}

impl Default for BoundaryStore {
    fn default() -> Self {
        Self::load_bundled()
    }
}

impl BoundaryStore {
    /// Loads the bundled Natural Earth boundaries dataset.
    #[must_use]
    pub fn load_bundled() -> Self {
        Self::from_json_str(DEFAULT_BOUNDARIES_JSON).unwrap_or_else(|_| Self::empty())
    }

    /// Creates an empty boundary store.
    #[must_use]
    pub fn empty() -> Self {
        Self { features: Arc::from([]), spatial_index: Arc::new(RTree::new()) }
    }

    /// Parses a JSON boundaries pack and constructs the R-tree spatial index.
    ///
    /// # Errors
    /// Returns a [`CoreError`] if JSON parsing or coordinate validation fails.
    pub fn from_json_str(raw_json: &str) -> Result<Self, CoreError> {
        let pack: RawBoundaryPack = serde_json::from_str(raw_json)
            .map_err(|e| CoreError::BadRequest(format!("Failed to parse boundaries JSON: {e}")))?;

        let mut features = Vec::with_capacity(pack.features.len());
        let mut spatial_items = Vec::with_capacity(pack.features.len());

        for (idx, raw) in pack.features.into_iter().enumerate() {
            let [west, south, east, north] = raw.bbox;
            let bbox = BBox::new(west, south, east, north).map_err(|e| {
                CoreError::BadRequest(format!("Invalid bbox for boundary feature {}: {e}", raw.id))
            })?;

            let mut lines = Vec::with_capacity(raw.lines.len());
            for line in raw.lines {
                let mut pts = Vec::with_capacity(line.len());
                for [lon, lat] in line {
                    let Ok(pt) = GeoPoint::new(lat, lon) else {
                        continue;
                    };
                    pts.push(pt);
                }
                if pts.len() >= 2 {
                    lines.push(pts);
                }
            }

            if lines.is_empty() {
                continue;
            }

            let feature = BoundaryFeature {
                id: raw.id,
                name: raw.name,
                class: raw.class,
                status: raw.status,
                sovereign: raw.sovereign,
                claimants: raw.claimants,
                note: raw.note,
                lines,
                bbox,
            };

            let envelope = AABB::from_corners([west, south], [east, north]);
            spatial_items.push(BoundarySpatialItem { index: idx, envelope });
            features.push(feature);
        }

        let spatial_index = RTree::bulk_load(spatial_items);

        Ok(Self { features: Arc::from(features), spatial_index: Arc::new(spatial_index) })
    }

    /// Returns the total count of boundary features loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Returns `true` if no boundary features are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    /// Finds a boundary feature by its unique ID.
    #[must_use]
    pub fn find_by_id(&self, id: &str) -> Option<&BoundaryFeature> {
        self.features.iter().find(|f| f.id == id)
    }

    /// Queries boundary features intersecting a given viewport bounding box,
    /// with optional class filtering, zoom-dependent simplification, and result limits.
    #[must_use]
    pub fn query_bbox(
        &self,
        bbox: &BBox,
        classes: Option<&[BoundaryClass]>,
        zoom: Option<u8>,
        limit: Option<usize>,
    ) -> Vec<BoundaryFeature> {
        let max_results = limit.unwrap_or(500).min(1000);
        let search_envelope = AABB::from_corners([bbox.west, bbox.south], [bbox.east, bbox.north]);

        let simplification_epsilon = zoom.map(|z| match z {
            0..=3 => 0.04,
            4..=6 => 0.015,
            7..=9 => 0.005,
            _ => 0.0,
        });

        let mut results = Vec::new();

        for item in self.spatial_index.locate_in_envelope_intersecting(search_envelope) {
            let Some(feature) = self.features.get(item.index) else {
                continue;
            };

            if let Some(cls) = classes
                && !cls.contains(&feature.class)
            {
                continue;
            }

            // If simplification is requested and epsilon > 0, simplify lines
            let feature_to_add = if let Some(eps) = simplification_epsilon {
                if eps > 0.0 {
                    let simplified_lines: Vec<Vec<GeoPoint>> = feature
                        .lines
                        .iter()
                        .map(|line| simplify_douglas_peucker(line, eps))
                        .filter(|line| line.len() >= 2)
                        .collect();

                    if simplified_lines.is_empty() {
                        continue;
                    }

                    BoundaryFeature {
                        id: feature.id.clone(),
                        name: feature.name.clone(),
                        class: feature.class,
                        status: feature.status.clone(),
                        sovereign: feature.sovereign.clone(),
                        claimants: feature.claimants.clone(),
                        note: feature.note.clone(),
                        lines: simplified_lines,
                        bbox: feature.bbox,
                    }
                } else {
                    feature.clone()
                }
            } else {
                feature.clone()
            };

            results.push(feature_to_add);
            if results.len() >= max_results {
                break;
            }
        }

        results
    }
}

/// Perpendicular distance squared from point `p` to segment between `v` and `w` in degrees.
fn perpendicular_distance_sq(p: &GeoPoint, v: &GeoPoint, w: &GeoPoint) -> f64 {
    let px = p.lon;
    let py = p.lat;
    let vx = v.lon;
    let vy = v.lat;
    let wx = w.lon;
    let wy = w.lat;

    let dx = wx - vx;
    let dy = wy - vy;
    let l2 = dx * dx + dy * dy;

    if l2 <= 1e-12 {
        let ex = px - vx;
        let ey = py - vy;
        return ex * ex + ey * ey;
    }

    let t = ((px - vx) * dx + (py - vy) * dy) / l2;
    let t_clamped = t.clamp(0.0, 1.0);
    let proj_x = vx + t_clamped * dx;
    let proj_y = vy + t_clamped * dy;

    let ex = px - proj_x;
    let ey = py - proj_y;
    ex * ex + ey * ey
}

/// Ramer-Douglas-Peucker line simplification on geographic coordinates.
fn simplify_douglas_peucker(points: &[GeoPoint], epsilon_deg: f64) -> Vec<GeoPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let eps_sq = epsilon_deg * epsilon_deg;
    let mut dmax = 0.0;
    let mut index = 0;
    let end = points.len() - 1;

    for i in 1..end {
        let d = perpendicular_distance_sq(&points[i], &points[0], &points[end]);
        if d > dmax {
            index = i;
            dmax = d;
        }
    }

    if dmax > eps_sq {
        let rec1 = simplify_douglas_peucker(&points[..=index], epsilon_deg);
        let rec2 = simplify_douglas_peucker(&points[index..], epsilon_deg);

        let mut result = rec1;
        result.pop();
        result.extend(rec2);
        result
    } else {
        vec![points[0], points[end]]
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::assert_is_empty,
        clippy::redundant_closure_for_method_calls
    )]
    use super::*;

    #[test]
    fn test_load_bundled_boundaries() {
        let store = BoundaryStore::load_bundled();
        assert!(!store.is_empty(), "Bundled boundaries should not be empty");
        assert!(store.len() >= 600, "Should have loaded 600+ boundary features");

        // Verify that all 4 classes exist in the store
        let mut has_intl = false;
        let mut has_disp = false;
        let mut has_brk = false;
        let mut has_mar = false;

        for f in store.features.iter() {
            match f.class {
                BoundaryClass::InternationalLand => has_intl = true,
                BoundaryClass::DisputedLine => has_disp = true,
                BoundaryClass::BreakawayArea => has_brk = true,
                BoundaryClass::MaritimeIndicative => has_mar = true,
            }
        }

        assert!(has_intl, "Store should contain InternationalLand features");
        assert!(has_disp, "Store should contain DisputedLine features");
        assert!(has_brk, "Store should contain BreakawayArea features");
        assert!(has_mar, "Store should contain MaritimeIndicative features");
    }

    #[test]
    fn test_find_by_id() {
        let store = BoundaryStore::load_bundled();
        let first = &store.features[0];
        let found = store.find_by_id(&first.id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, first.id);

        let missing = store.find_by_id("non-existent-id-9999");
        assert!(missing.is_none());
    }

    #[test]
    fn test_query_bbox_and_class_filter() {
        let store = BoundaryStore::load_bundled();

        // Query entire world
        let world_bbox = BBox::new(-180.0, -85.0, 180.0, 85.0).unwrap();
        let all_results = store.query_bbox(&world_bbox, None, None, Some(1000));
        assert!(!all_results.is_empty());

        // Filter only disputed lines
        let disp_results =
            store.query_bbox(&world_bbox, Some(&[BoundaryClass::DisputedLine]), None, Some(500));
        assert!(!disp_results.is_empty());
        for f in &disp_results {
            assert_eq!(f.class, BoundaryClass::DisputedLine);
        }

        // Filter only breakaway areas
        let brk_results =
            store.query_bbox(&world_bbox, Some(&[BoundaryClass::BreakawayArea]), None, Some(500));
        assert!(!brk_results.is_empty());
        for f in &brk_results {
            assert_eq!(f.class, BoundaryClass::BreakawayArea);
        }
    }

    #[test]
    fn test_query_bbox_spatial_localization() {
        let store = BoundaryStore::load_bundled();

        // Specific small BBox around Black Sea / Caucasus (Crimea, Abkhazia, South Ossetia)
        let caucasus_bbox = BBox::new(30.0, 40.0, 50.0, 48.0).unwrap();
        let results = store.query_bbox(&caucasus_bbox, None, None, Some(500));
        assert!(!results.is_empty());

        for f in &results {
            assert!(
                f.bbox.west <= caucasus_bbox.east
                    && f.bbox.east >= caucasus_bbox.west
                    && f.bbox.south <= caucasus_bbox.north
                    && f.bbox.north >= caucasus_bbox.south,
                "Feature bbox must intersect query bbox"
            );
        }
    }

    #[test]
    fn test_zoom_simplification() {
        let store = BoundaryStore::load_bundled();
        let world_bbox = BBox::new(-180.0, -85.0, 180.0, 85.0).unwrap();

        // Low zoom query (zoom 2)
        let low_zoom = store.query_bbox(
            &world_bbox,
            Some(&[BoundaryClass::InternationalLand]),
            Some(2),
            Some(10),
        );
        // High zoom query (zoom 12)
        let high_zoom = store.query_bbox(
            &world_bbox,
            Some(&[BoundaryClass::InternationalLand]),
            Some(12),
            Some(10),
        );

        let low_pts: usize =
            low_zoom.iter().map(|f| f.lines.iter().map(|l| l.len()).sum::<usize>()).sum();
        let high_pts: usize =
            high_zoom.iter().map(|f| f.lines.iter().map(|l| l.len()).sum::<usize>()).sum();

        assert!(
            low_pts < high_pts,
            "Low zoom should have fewer vertices than high zoom: low={low_pts}, high={high_pts}"
        );
    }
}
