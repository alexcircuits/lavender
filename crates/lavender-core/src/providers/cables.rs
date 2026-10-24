//! Static telecommunications infrastructure provider: submarine cables and landing points.
//!
//! `TeleGeography` submarine cables dataset (CC BY-NC-SA 3.0).
//! Commercial deployments must secure an appropriate license from `TeleGeography`.

use std::sync::Arc;

use lavender_domain::{BBox, GeoPoint};
use lavender_geo::PointIndex;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

const DEFAULT_CABLES_GEOJSON: &str = include_str!(
    "../../../../legacy/src/data/local_data/telegeography_submarine_cables/cable-geo.json"
);
const DEFAULT_LANDING_POINTS_GEOJSON: &str = include_str!(
    "../../../../legacy/src/data/local_data/telegeography_submarine_cables/landing-point-geo.json"
);

/// A submarine fiber-optic communications cable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmarineCable {
    pub id: String,
    pub name: String,
    pub color: String,
    pub coordinates: Vec<Vec<[f64; 2]>>,
    pub bbox: BBox,
    pub point_count: usize,
}

/// A coastal submarine cable landing station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingPoint {
    pub id: String,
    pub name: String,
    pub position: GeoPoint,
    pub is_tbd: bool,
}

/// In-memory spatial store for submarine cables and landing points.
#[derive(Debug, Clone)]
pub struct CableStore {
    cables: Arc<[SubmarineCable]>,
    landing_points: Arc<[LandingPoint]>,
    landing_index: Arc<PointIndex>,
}

impl Default for CableStore {
    fn default() -> Self {
        Self::load_bundled()
    }
}

impl CableStore {
    /// Loads the bundled submarine cables and landing points GeoJSON datasets.
    #[must_use]
    pub fn load_bundled() -> Self {
        Self::from_geojson_str(DEFAULT_CABLES_GEOJSON, DEFAULT_LANDING_POINTS_GEOJSON)
            .unwrap_or_else(|_| Self::empty())
    }

    /// Creates an empty cable store.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cables: Arc::from([]),
            landing_points: Arc::from([]),
            landing_index: Arc::new(PointIndex::default()),
        }
    }

    /// Parses cables and landing points from GeoJSON string slices.
    ///
    /// # Errors
    /// Returns an error if the JSON is malformed.
    pub fn from_geojson_str(cables_json: &str, landings_json: &str) -> Result<Self, CoreError> {
        let cables = parse_cables(cables_json)?;
        let landing_points = parse_landings(landings_json)?;

        let positions: Vec<GeoPoint> = landing_points.iter().map(|l| l.position).collect();
        let landing_index = PointIndex::build(&positions);

        Ok(Self {
            cables: Arc::from(cables),
            landing_points: Arc::from(landing_points),
            landing_index: Arc::new(landing_index),
        })
    }

    /// Total number of loaded cables.
    #[must_use]
    pub fn cable_count(&self) -> usize {
        self.cables.len()
    }

    /// Total number of loaded landing points.
    #[must_use]
    pub fn landing_count(&self) -> usize {
        self.landing_points.len()
    }

    /// Queries submarine cables intersecting the given bounding box.
    #[must_use]
    pub fn query_cables(&self, bbox: &BBox, zoom: u8, limit: usize) -> Vec<SubmarineCable> {
        let mut results = Vec::new();
        for cable in self.cables.iter() {
            if results.len() >= limit {
                break;
            }
            if !cable.bbox.intersects(bbox) {
                continue;
            }
            // Zoom-level LOD thinning: at low zoom, skip short minor cables
            if zoom < 4 && cable.point_count < 10 {
                continue;
            }
            results.push(cable.clone());
        }
        results
    }

    /// Queries cable landing points inside the given bounding box.
    #[must_use]
    pub fn query_landings(&self, bbox: &BBox, zoom: u8, limit: usize) -> Vec<LandingPoint> {
        let indices = self.landing_index.query(bbox);
        let mut results = Vec::new();

        // Thinning stride for lower zoom levels
        let stride = match zoom {
            0..=3 => 8,
            4..=5 => 4,
            6..=7 => 2,
            _ => 1,
        };

        for (i, &idx) in indices.iter().enumerate() {
            if i % stride != 0 {
                continue;
            }
            if let Some(landing) = self.landing_points.get(idx) {
                results.push(landing.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
    }

    /// Finds a single cable by ID.
    #[must_use]
    pub fn find_cable(&self, id: &str) -> Option<&SubmarineCable> {
        self.cables.iter().find(|c| c.id == id)
    }

    /// Finds a single landing point by ID.
    #[must_use]
    pub fn find_landing(&self, id: &str) -> Option<&LandingPoint> {
        self.landing_points.iter().find(|l| l.id == id)
    }
}

// ---------------------------------------------------------------------------
// GeoJSON Parsing
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FeatureCollectionRaw {
    features: Vec<FeatureRaw>,
}

#[derive(Deserialize)]
struct FeatureRaw {
    properties: serde_json::Value,
    geometry: GeometryRaw,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum GeometryRaw {
    LineString { coordinates: Vec<[f64; 2]> },
    MultiLineString { coordinates: Vec<Vec<[f64; 2]>> },
    Point { coordinates: [f64; 2] },
}

fn parse_cables(raw_json: &str) -> Result<Vec<SubmarineCable>, CoreError> {
    let collection: FeatureCollectionRaw = serde_json::from_str(raw_json)
        .map_err(|e| CoreError::BadRequest(format!("Failed to parse cable GeoJSON: {e}")))?;

    let mut cables = Vec::with_capacity(collection.features.len());

    for feature in collection.features {
        let props = feature.properties;
        let id = props.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let name = props.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let color = props.get("color").and_then(|v| v.as_str()).unwrap_or("#39d5ff").to_owned();

        let segments: Vec<Vec<[f64; 2]>> = match feature.geometry {
            GeometryRaw::LineString { coordinates } => vec![coordinates],
            GeometryRaw::MultiLineString { coordinates } => coordinates,
            GeometryRaw::Point { .. } => continue,
        };

        if segments.is_empty() {
            continue;
        }

        let mut min_lon = 180.0;
        let mut max_lon = -180.0;
        let mut min_lat = 90.0;
        let mut max_lat = -90.0;
        let mut total_points = 0;

        for seg in &segments {
            for &[lon, lat] in seg {
                if lon < min_lon {
                    min_lon = lon;
                }
                if lon > max_lon {
                    max_lon = lon;
                }
                if lat < min_lat {
                    min_lat = lat;
                }
                if lat > max_lat {
                    max_lat = lat;
                }
                total_points += 1;
            }
        }

        if total_points == 0 {
            continue;
        }

        // Clamp bounds to valid range
        min_lon = min_lon.clamp(-180.0, 180.0);
        max_lon = max_lon.clamp(-180.0, 180.0);
        min_lat = min_lat.clamp(-90.0, 90.0);
        max_lat = max_lat.clamp(-90.0, 90.0);

        if min_lat > max_lat {
            max_lat = min_lat;
        }

        let bbox = BBox::new(min_lon, min_lat, max_lon, max_lat).unwrap_or(BBox::WORLD);

        cables.push(SubmarineCable {
            id,
            name,
            color,
            coordinates: segments,
            bbox,
            point_count: total_points,
        });
    }

    Ok(cables)
}

fn parse_landings(raw_json: &str) -> Result<Vec<LandingPoint>, CoreError> {
    let collection: FeatureCollectionRaw = serde_json::from_str(raw_json)
        .map_err(|e| CoreError::BadRequest(format!("Failed to parse landings GeoJSON: {e}")))?;

    let mut landings = Vec::with_capacity(collection.features.len());

    for feature in collection.features {
        let props = feature.properties;
        let id = props.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let name = props.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let is_tbd = props.get("is_tbd").and_then(serde_json::Value::as_bool).unwrap_or(false);

        let GeometryRaw::Point { coordinates: [lon, lat] } = feature.geometry else {
            continue;
        };

        if let Ok(position) = GeoPoint::new(lat, lon) {
            landings.push(LandingPoint { id, name, position, is_tbd });
        }
    }

    Ok(landings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_cables_and_landings_load_successfully() {
        let store = CableStore::load_bundled();
        assert!(store.cable_count() > 400, "expected >400 cables, got {}", store.cable_count());
        assert!(
            store.landing_count() > 1000,
            "expected >1000 landings, got {}",
            store.landing_count()
        );
    }

    #[test]
    fn query_cables_in_atlantic() {
        let store = CableStore::load_bundled();
        let atlantic = BBox::new(-60.0, 10.0, -10.0, 60.0).unwrap();
        let results = store.query_cables(&atlantic, 8, 100);
        assert!(!results.is_empty(), "expected Atlantic cables");
    }

    #[test]
    fn query_landings_thinning() {
        let store = CableStore::load_bundled();
        let world = BBox::WORLD;
        let low_zoom = store.query_landings(&world, 2, 2000);
        let high_zoom = store.query_landings(&world, 10, 2000);
        assert!(low_zoom.len() < high_zoom.len(), "low zoom should have fewer landings");
    }

    #[test]
    fn find_specific_cable() {
        let store = CableStore::load_bundled();
        let cable = store.find_cable("kunoa-north");
        assert!(cable.is_some());
        let c = cable.unwrap();
        assert_eq!(c.name, "Kunoa North");
    }
}
