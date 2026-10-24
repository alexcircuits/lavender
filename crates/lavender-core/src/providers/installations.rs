//! Military installations provider and spatial index.
//!
//! Ported directly from `legacy/server/providers/military-installations/`:
//! Indexes airfields, naval bases, ranges, barracks and military facilities
//! with bounding-box quantization to maximize cache reuse.

use std::collections::HashMap;
use std::sync::Arc;

use lavender_domain::{BBox, GeoPoint};
use lavender_geo::PointIndex;
use serde::{Deserialize, Serialize};

/// Snap step in degrees for military installation viewports (0.05 degrees).
pub const MILITARY_INSTALLATION_BBOX_STEP_DEG: f64 = 0.05;

/// Maximum features returned in a single viewport query.
pub const MILITARY_INSTALLATION_ELEMENT_CAP: usize = 2000;

/// Classification of a military site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallationKind {
    Airfield,
    NavalBase,
    Range,
    Barracks,
    Base,
    General,
}

impl InstallationKind {
    #[must_use]
    pub fn from_tag(military_tag: Option<&str>, _landuse_tag: Option<&str>) -> Self {
        match military_tag {
            Some("airfield") => Self::Airfield,
            Some("naval_base") => Self::NavalBase,
            Some("range") => Self::Range,
            Some("barracks") => Self::Barracks,
            Some("base") => Self::Base,
            _ => Self::General,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Airfield => "airfield",
            Self::NavalBase => "naval_base",
            Self::Range => "range",
            Self::Barracks => "barracks",
            Self::Base => "base",
            Self::General => "general",
        }
    }
}

/// A military installation entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MilitaryInstallation {
    pub id: String,
    pub name: Option<String>,
    pub kind: InstallationKind,
    pub position: GeoPoint,
    pub tags: HashMap<String, String>,
}

/// Snaps a bounding box outward to the shared installation cache grid (0.05 deg).
#[must_use]
pub fn quantize_military_installation_box(bbox: &BBox) -> BBox {
    let step = MILITARY_INSTALLATION_BBOX_STEP_DEG;
    let snap = |val: f64, grow: i32| -> f64 {
        let cells = val / step;
        let c = if grow > 0 { cells.ceil() } else { cells.floor() };
        (c * step).clamp(-180.0, 180.0)
    };

    let south = (-90.0_f64).max(snap(bbox.south, -1));
    let west = (-180.0_f64).max(snap(bbox.west, -1));
    let north = (90.0_f64).min(snap(bbox.north, 1));
    let east = (180.0_f64).min(snap(bbox.east, 1));

    BBox::new(west, south, east, north).unwrap_or(*bbox)
}

/// Computes a stable cache key for an installation bounding box.
#[must_use]
pub fn military_installation_cache_key(bbox: &BBox, decimals: usize) -> String {
    format!(
        "{:.prec$},{:.prec$},{:.prec$},{:.prec$}",
        bbox.south,
        bbox.west,
        bbox.north,
        bbox.east,
        prec = decimals
    )
}

/// In-memory spatial index of military installations.
#[derive(Debug, Clone, Default)]
pub struct InstallationStore {
    items: Arc<[MilitaryInstallation]>,
    spatial_index: Arc<PointIndex>,
}

impl InstallationStore {
    #[must_use]
    pub const fn default_limit() -> usize {
        MILITARY_INSTALLATION_ELEMENT_CAP
    }

    #[must_use]
    pub fn new(items: Vec<MilitaryInstallation>) -> Self {
        let points: Vec<GeoPoint> = items.iter().map(|i| i.position).collect();
        let spatial_index = Arc::new(PointIndex::build(&points));
        Self { items: items.into(), spatial_index }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Queries installations within a bounding box, up to a limit.
    #[must_use]
    pub fn query_in_bbox(&self, bbox: &BBox, limit: usize) -> Vec<&MilitaryInstallation> {
        let mut indices = self.spatial_index.query(bbox);
        indices.truncate(limit);
        indices.into_iter().filter_map(|idx| self.items.get(idx)).collect()
    }

    /// Finds an installation by its OSM ID.
    #[must_use]
    pub fn find_by_id(&self, id: &str) -> Option<&MilitaryInstallation> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Parses Overpass JSON elements into military installation items.
    #[must_use]
    pub fn from_overpass_json(json_str: &str) -> Self {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else {
            return Self::default();
        };

        let Some(elements) = val.get("elements").and_then(serde_json::Value::as_array) else {
            return Self::default();
        };

        let mut items = Vec::new();
        for elem in elements {
            let Some(id_num) = elem.get("id").and_then(serde_json::Value::as_i64) else {
                continue;
            };
            let elem_type = elem.get("type").and_then(serde_json::Value::as_str).unwrap_or("node");
            let id = format!("{elem_type}/{id_num}");

            let lat = elem
                .get("lat")
                .and_then(serde_json::Value::as_f64)
                .or_else(|| elem.get("center").and_then(|c| c.get("lat")?.as_f64()));

            let lon = elem
                .get("lon")
                .and_then(serde_json::Value::as_f64)
                .or_else(|| elem.get("center").and_then(|c| c.get("lon")?.as_f64()));

            let (Some(lat), Some(lon)) = (lat, lon) else {
                continue;
            };

            let Ok(position) = GeoPoint::new(lat, lon) else {
                continue;
            };

            let tags_obj = elem.get("tags").and_then(serde_json::Value::as_object);
            let mut tags = HashMap::new();
            let mut name = None;
            let mut military_val = None;
            let mut landuse_val = None;

            if let Some(obj) = tags_obj {
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        if k == "name" {
                            name = Some(s.to_string());
                        } else if k == "military" {
                            military_val = Some(s);
                        } else if k == "landuse" {
                            landuse_val = Some(s);
                        }
                        tags.insert(k.clone(), s.to_string());
                    }
                }
            }

            let kind = InstallationKind::from_tag(military_val, landuse_val);

            items.push(MilitaryInstallation { id, name, kind, position, tags });
        }

        Self::new(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_bounding_boxes() {
        let bbox = BBox::new(-97.7431, 30.2672, -97.7300, 30.2750).unwrap();
        let snapped = quantize_military_installation_box(&bbox);
        assert!(snapped.west <= bbox.west);
        assert!(snapped.south <= bbox.south);
        assert!(snapped.east >= bbox.east);
        assert!(snapped.north >= bbox.north);
    }

    #[test]
    fn parses_overpass_mock_response() {
        let json = r#"{
            "elements": [
                {
                    "type": "way",
                    "id": 12345,
                    "center": {"lat": 30.2672, "lon": -97.7431},
                    "tags": {
                        "name": "Camp Mabry",
                        "military": "barracks"
                    }
                },
                {
                    "type": "node",
                    "id": 67890,
                    "lat": 30.3000,
                    "lon": -97.7000,
                    "tags": {
                        "name": "Naval Reserve Center",
                        "military": "naval_base"
                    }
                }
            ]
        }"#;

        let store = InstallationStore::from_overpass_json(json);
        assert_eq!(store.len(), 2);

        let item = store.find_by_id("way/12345").unwrap();
        assert_eq!(item.name.as_deref(), Some("Camp Mabry"));
        assert_eq!(item.kind, InstallationKind::Barracks);

        let bbox = BBox::new(-98.0, 30.0, -97.0, 31.0).unwrap();
        let query = store.query_in_bbox(&bbox, 10);
        assert_eq!(query.len(), 2);
    }
}
