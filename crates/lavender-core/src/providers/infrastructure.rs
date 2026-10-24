//! Static infrastructure provider: OpenStreetMap datacenters and dams.
//!
//! Datasets are loaded into an in-memory R-tree spatial index to answer viewport
//! queries with level-of-detail zoom thinning.

use std::sync::Arc;

use lavender_domain::{BBox, GeoPoint};
use lavender_geo::PointIndex;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

const DEFAULT_DAMS_GEOJSONL: &str =
    include_str!("../../../../legacy/src/data/local_data/dams/dams.geojsonl");
const DEFAULT_DATACENTERS_GEOJSONL: &str =
    include_str!("../../../../legacy/src/data/local_data/datacenters/datacenters.geojsonl");

/// Known major datacenter operators for zoom thinning.
const MAJOR_DATACENTER_OPERATORS: &[&str] = &[
    "amazon",
    "aws",
    "microsoft",
    "azure",
    "google",
    "meta",
    "facebook",
    "equinix",
    "digital realty",
    "ntt",
    "oracle",
    "apple",
    "ibm",
    "alibaba",
    "cyrusone",
    "quality technology services",
    "qts",
    "flexential",
    "coresite",
    "vantage",
    "lumen",
    "ovh",
];

/// What kind of static infrastructure a facility represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InfrastructureKind {
    Datacenter,
    Dam,
}

/// A static infrastructure facility (datacenter or dam).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfrastructureItem {
    pub id: String,
    pub kind: InfrastructureKind,
    pub name: String,
    pub operator: Option<String>,
    pub position: GeoPoint,
    pub details: serde_json::Value,
}

impl InfrastructureItem {
    /// Whether this facility is considered a major facility for zoom-level thinning.
    #[must_use]
    pub fn is_major(&self) -> bool {
        match self.kind {
            InfrastructureKind::Dam => is_major_dam(&self.details),
            InfrastructureKind::Datacenter => {
                is_major_datacenter(self.operator.as_deref(), &self.details)
            }
        }
    }
}

fn is_major_dam(details: &serde_json::Value) -> bool {
    let tags = details.get("tags");
    let output = details
        .get("output")
        .and_then(serde_json::Value::as_str)
        .or_else(|| tags.and_then(|t| t.get("plant:output:electricity")?.as_str()))
        .unwrap_or("");

    let lower = output.to_ascii_lowercase();
    if lower.contains("gw") {
        return true;
    }
    if lower.contains("mw") {
        let digits: String =
            lower.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let Ok(val) = digits.parse::<f64>() {
            if val >= 50.0 {
                return true;
            }
        } else {
            return true;
        }
    }

    let importance = details
        .get("importance")
        .and_then(serde_json::Value::as_str)
        .or_else(|| tags.and_then(|t| t.get("importance")?.as_str()))
        .unwrap_or("");
    if importance.eq_ignore_ascii_case("international") {
        return true;
    }

    let has_op = details.get("operator").or_else(|| tags.and_then(|t| t.get("operator"))).is_some();
    has_op && !output.is_empty()
}

fn is_major_datacenter(operator: Option<&str>, details: &serde_json::Value) -> bool {
    if let Some(op) = operator {
        let op_lower = op.to_ascii_lowercase();
        for major in MAJOR_DATACENTER_OPERATORS {
            if op_lower.contains(major) {
                return true;
            }
        }
    }

    let tags = details.get("tags");
    details.get("capacity").is_some()
        || details.get("it_load").is_some()
        || tags.and_then(|t| t.get("capacity:it_load")).is_some()
        || tags.and_then(|t| t.get("it_load")).is_some()
        || tags.and_then(|t| t.get("capacity")).is_some()
}

/// An in-memory spatial index over infrastructure items.
#[derive(Debug, Clone)]
pub struct InfrastructureIndex {
    items: Arc<[InfrastructureItem]>,
    spatial_index: Arc<PointIndex>,
    major_flags: Arc<[bool]>,
}

impl Default for InfrastructureIndex {
    fn default() -> Self {
        Self::load_default()
    }
}

impl InfrastructureIndex {
    /// Loads the bundled dams and datacenters datasets.
    #[must_use]
    pub fn load_default() -> Self {
        Self::from_geojsonl_str(DEFAULT_DAMS_GEOJSONL, DEFAULT_DATACENTERS_GEOJSONL)
            .unwrap_or_else(|_| Self::empty())
    }

    /// Creates an empty infrastructure index.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            items: Arc::from([]),
            spatial_index: Arc::new(PointIndex::default()),
            major_flags: Arc::from([]),
        }
    }

    /// Builds the index from GeoJSON Lines text for dams and datacenters.
    ///
    /// # Errors
    /// Returns an error if any record fails to parse.
    pub fn from_geojsonl_str(
        dams_geojsonl: &str,
        datacenters_geojsonl: &str,
    ) -> Result<Self, CoreError> {
        let mut items = Vec::new();
        let mut major_flags = Vec::new();

        parse_geojsonl_items(dams_geojsonl, InfrastructureKind::Dam, &mut items, &mut major_flags)?;
        parse_geojsonl_items(
            datacenters_geojsonl,
            InfrastructureKind::Datacenter,
            &mut items,
            &mut major_flags,
        )?;

        let positions: Vec<GeoPoint> = items.iter().map(|item| item.position).collect();
        let spatial_index = PointIndex::build(&positions);

        Ok(Self {
            items: Arc::from(items),
            spatial_index: Arc::new(spatial_index),
            major_flags: Arc::from(major_flags),
        })
    }

    /// Returns the total number of indexed infrastructure items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if there are no items in the index.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Queries infrastructure items intersecting `bbox`, applying zoom-level thinning
    /// and capping the result to `limit`.
    ///
    /// At `zoom_level < 6`, only major facilities are returned.
    /// At `6 <= zoom_level < 10`, major and named facilities are prioritised.
    /// At `zoom_level >= 10`, all facilities in the bounding box are returned up to `limit`.
    #[must_use]
    pub fn query_bbox(&self, bbox: &BBox, zoom_level: u8, limit: usize) -> Vec<InfrastructureItem> {
        if limit == 0 || self.is_empty() {
            return Vec::new();
        }

        let hits = self.spatial_index.query(bbox);
        let mut candidates: Vec<usize> = Vec::with_capacity(hits.len().min(limit));

        for idx in hits {
            let is_major = self.major_flags.get(idx).copied().unwrap_or(false);
            if zoom_level < 6 {
                if is_major {
                    candidates.push(idx);
                }
            } else if zoom_level < 10 {
                // Include major facilities or those with operators/names
                let item = &self.items[idx];
                if is_major || item.operator.is_some() || !item.name.is_empty() {
                    candidates.push(idx);
                }
            } else {
                candidates.push(idx);
            }
        }

        // Prioritise major facilities first, then by index for stability
        candidates.sort_by_key(|&idx| {
            let is_major = self.major_flags.get(idx).copied().unwrap_or(false);
            (!is_major, idx)
        });

        if candidates.len() > limit {
            candidates.truncate(limit);
        }

        candidates.into_iter().map(|idx| self.items[idx].clone()).collect()
    }
}

fn parse_geojsonl_items(
    text: &str,
    kind: InfrastructureKind,
    items: &mut Vec<InfrastructureItem>,
    major_flags: &mut Vec<bool>,
) -> Result<(), CoreError> {
    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|err| {
            CoreError::BadRequest(format!("malformed GeoJSON line {line_idx} for {kind:?}: {err}"))
        })?;

        let Some(geometry) = value.get("geometry") else {
            continue;
        };
        let Some(position) = extract_representative_point(geometry) else {
            continue;
        };

        let properties = value.get("properties").cloned().unwrap_or(serde_json::Value::Null);

        let id = extract_id(&value, &properties, line_idx, kind);
        let name = extract_name(&properties, kind, &id);
        let operator = extract_operator(&properties);

        let is_major = match kind {
            InfrastructureKind::Dam => is_major_dam(&properties),
            InfrastructureKind::Datacenter => is_major_datacenter(operator.as_deref(), &properties),
        };

        items.push(InfrastructureItem { id, kind, name, operator, position, details: properties });
        major_flags.push(is_major);
    }
    Ok(())
}

fn extract_id(
    feature: &serde_json::Value,
    properties: &serde_json::Value,
    line_idx: usize,
    kind: InfrastructureKind,
) -> String {
    if let Some(osm_id) = properties.get("osm_id") {
        if let Some(i) = osm_id.as_i64() {
            return i.to_string();
        }
        if let Some(s) = osm_id.as_str()
            && !s.is_empty()
        {
            return s.to_owned();
        }
    }
    if let Some(fid) = feature.get("id") {
        if let Some(i) = fid.as_i64() {
            return i.to_string();
        }
        if let Some(s) = fid.as_str()
            && !s.is_empty()
        {
            return s.to_owned();
        }
    }
    format!("{kind:?}-{line_idx}")
}

fn extract_name(
    properties: &serde_json::Value,
    kind: InfrastructureKind,
    fallback_id: &str,
) -> String {
    let tags = properties.get("tags");

    let candidates = [
        properties.get("name").and_then(serde_json::Value::as_str),
        tags.and_then(|t| t.get("name")?.as_str()),
        tags.and_then(|t| t.get("name:en")?.as_str()),
        tags.and_then(|t| t.get("official_name")?.as_str()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let t = candidate.trim();
        if !t.is_empty() && t != "null" && t != "undefined" {
            return t.to_owned();
        }
    }

    match kind {
        InfrastructureKind::Datacenter => format!("Datacenter {fallback_id}"),
        InfrastructureKind::Dam => format!("Dam {fallback_id}"),
    }
}

fn extract_operator(properties: &serde_json::Value) -> Option<String> {
    let tags = properties.get("tags");

    let candidates = [
        tags.and_then(|t| t.get("operator")?.as_str()),
        properties.get("operator").and_then(serde_json::Value::as_str),
        tags.and_then(|t| t.get("operator:short")?.as_str()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let t = candidate.trim();
        if !t.is_empty() && t != "null" && t != "undefined" {
            return Some(t.to_owned());
        }
    }
    None
}

fn extract_representative_point(geom: &serde_json::Value) -> Option<GeoPoint> {
    let geom_type = geom.get("type")?.as_str()?;
    let coords = geom.get("coordinates")?;
    match geom_type {
        "Point" => {
            let arr = coords.as_array()?;
            let lon = arr.first()?.as_f64()?;
            let lat = arr.get(1)?.as_f64()?;
            GeoPoint::new(lat, lon).ok()
        }
        "LineString" | "MultiPoint" => {
            let pts = coords.as_array()?;
            average_points(pts)
        }
        "Polygon" | "MultiLineString" => {
            let ring_list = coords.as_array()?;
            let ring = ring_list.first()?.as_array()?;
            average_points(ring)
        }
        "MultiPolygon" => {
            let poly_list = coords.as_array()?;
            let poly = poly_list.first()?.as_array()?;
            let ring = poly.first()?.as_array()?;
            average_points(ring)
        }
        _ => None,
    }
}

fn average_points(pts: &[serde_json::Value]) -> Option<GeoPoint> {
    let mut sum_lon = 0.0;
    let mut sum_lat = 0.0;
    let mut count = 0usize;
    for pt in pts {
        if let Some(arr) = pt.as_array()
            && let (Some(lon), Some(lat)) = (
                arr.first().and_then(serde_json::Value::as_f64),
                arr.get(1).and_then(serde_json::Value::as_f64),
            )
            && lon.is_finite()
            && lat.is_finite()
        {
            sum_lon += lon;
            sum_lat += lat;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count_f = count as f64;
    GeoPoint::new(sum_lat / count_f, sum_lon / count_f).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assert_is_empty)]
    use super::*;

    #[test]
    fn loads_bundled_infrastructure() {
        let index = InfrastructureIndex::load_default();
        assert_eq!(index.len(), 704 + 4351);
        assert!(!index.is_empty());
    }

    #[test]
    fn queries_bbox_with_zoom_thinning() {
        let index = InfrastructureIndex::load_default();
        let world = BBox::WORLD;

        let low_zoom = index.query_bbox(&world, 4, 1000);
        let high_zoom = index.query_bbox(&world, 10, 1000);

        // All returned at zoom 4 must be major
        for item in &low_zoom {
            assert!(item.is_major());
        }

        assert!(!low_zoom.is_empty());
        assert!(!high_zoom.is_empty());
        assert_eq!(low_zoom.len().min(1000), low_zoom.len());
    }

    #[test]
    fn query_bbox_respects_limit() {
        let index = InfrastructureIndex::load_default();
        let results = index.query_bbox(&BBox::WORLD, 12, 10);
        assert!(results.len() <= 10);
    }
}
