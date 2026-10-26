//! Automated License Plate Reader (`ALPR`) surveillance camera provider.
//!
//! Ported from `legacy/src/layers/alpr/records.js` and `legacy/src/layers/alpr/policy.js`:
//! snaps viewport bounding boxes outward to a `0.05°` cache grid, builds case-insensitive
//! `Overpass` queries matching semicolon-delimited `surveillance:type` tags (`camera;ALPR`),
//! normalizes camera bearings (`[0, 360)`) and `OpenStreetMap` node attributes, and maintains
//! a thread-safe store seeded with baseline `ALPR` camera fixtures.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

use jiff::Timestamp;
use lavender_domain::entities::{AlprCamera, AlprSnapshot};
use lavender_domain::text::clean_opt;
use lavender_domain::{BBox, GeoPoint};
use serde_json::{Map, Value};

use crate::CoreError;

/// Default outward grid snap step in degrees for `ALPR` viewport queries (`0.05°`).
pub const ALPR_QUERY_SNAP_DEG: f64 = 0.05;

/// Default `Overpass` element cap per viewport query (`1500`).
pub const ALPR_QUERY_LIMIT: usize = 1500;

/// Maximum viewport span in degrees before requiring the caller to zoom in (`3.0°`).
pub const ALPR_MAX_VIEWPORT_DEG: f64 = 3.0;

/// Snaps a viewport [`BBox`] outward to the `step_deg` grid (typically `0.05°`), clamped to
/// valid WGS84 latitude (`[-90, 90]`) and longitude (`[-180, 180]`) bounds.
#[must_use]
pub fn snap_alpr_box(bbox: BBox, step_deg: f64) -> BBox {
    let step = if step_deg.is_finite() && step_deg > 0.0 { step_deg } else { ALPR_QUERY_SNAP_DEG };

    let down = |v: f64| ((v / step).floor() * step * 1e9).round() / 1e9;
    let up = |v: f64| ((v / step).ceil() * step * 1e9).round() / 1e9;

    let south = down(bbox.south).clamp(-90.0, 90.0);
    let west = down(bbox.west).clamp(-180.0, 180.0);
    let north = up(bbox.north).clamp(-90.0, 90.0).max(south);
    let east = up(bbox.east).clamp(-180.0, 180.0);

    BBox::new(west, south, east, north).unwrap_or(bbox)
}

/// Checks whether an `OpenStreetMap` `surveillance:type` value denotes an `ALPR` camera.
///
/// Splits semicolon-delimited multi-values (e.g. `"camera;ALPR"`) and matches `"ALPR"`
/// case-insensitively.
#[must_use]
pub fn is_alpr_surveillance_type(raw: &str) -> bool {
    raw.split(';').any(|part| part.trim().eq_ignore_ascii_case("ALPR"))
}

/// Normalizes a raw bearing string into degrees in `[0.0, 360.0)`.
///
/// Blank or whitespace-only inputs return `None` (never `0.0` due-north). Values outside
/// `0.0..=360.0` or non-numeric compass words return `None`; `360.0` wraps to `0.0`.
#[must_use]
pub fn normalize_direction_deg(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let degrees = trimmed.parse::<f64>().ok()?;
    if !degrees.is_finite() || !(0.0..=360.0).contains(&degrees) {
        return None;
    }
    Some(degrees % 360.0)
}

/// Builds the `Overpass` QL query for `ALPR` surveillance nodes within `bbox` up to `limit`.
#[must_use]
pub fn build_alpr_overpass_query(bbox: BBox, limit: usize) -> String {
    format!(
        r#"[out:json][timeout:20];node["man_made"="surveillance"]["surveillance:type"~"(^|;)\s*ALPR\s*(;|$)",i]({},{},{},{});out body {};"#,
        bbox.south, bbox.west, bbox.north, bbox.east, limit
    )
}

fn text_tag(tags: &Map<String, Value>, key: &str) -> Option<String> {
    clean_opt(tags.get(key).and_then(Value::as_str), 160)
}

fn direction_from_tags(tags: &Map<String, Value>) -> Option<f64> {
    let val = tags.get("camera:direction").or_else(|| tags.get("direction"))?;
    match val {
        Value::String(s) => normalize_direction_deg(s),
        Value::Number(n) => {
            let deg = n.as_f64()?;
            if deg.is_finite() && (0.0..=360.0).contains(&deg) { Some(deg % 360.0) } else { None }
        }
        _ => None,
    }
}

/// Normalizes a single `Overpass` JSON node element into an [`AlprCamera`].
#[must_use]
pub fn normalize_alpr_node(el: &Value) -> Option<AlprCamera> {
    let obj = el.as_object()?;
    if obj.get("type").and_then(Value::as_str) != Some("node") {
        return None;
    }

    let osm_id = obj.get("id").and_then(Value::as_i64).filter(|&id| id > 0)?;
    let lat = obj.get("lat").and_then(Value::as_f64)?;
    let lon = obj.get("lon").and_then(Value::as_f64)?;
    let position = GeoPoint::new(lat, lon).ok()?;

    let tags = obj.get("tags").and_then(Value::as_object)?;
    let surv_type = tags.get("surveillance:type").and_then(Value::as_str)?;
    if !is_alpr_surveillance_type(surv_type) {
        return None;
    }

    let operator = text_tag(tags, "operator");
    let manufacturer = text_tag(tags, "manufacturer");
    let camera_type = text_tag(tags, "camera:type");
    let zone = text_tag(tags, "surveillance:zone");
    let direction_deg = direction_from_tags(tags);
    let ref_code = text_tag(tags, "ref");
    let last_verified = text_tag(tags, "check_date").or_else(|| text_tag(tags, "survey:date"));
    let source = text_tag(tags, "source");

    Some(AlprCamera {
        id: format!("alpr:{osm_id}"),
        osm_id,
        position,
        operator,
        manufacturer,
        camera_type,
        zone,
        direction_deg,
        ref_code,
        last_verified,
        source,
    })
}

/// Parses an `Overpass` JSON response into a validated [`AlprSnapshot`].
///
/// Rejects responses missing an `elements` array or carrying a non-empty `remark` field
/// (which `Overpass` emits on query timeouts or memory exhaustion).
///
/// # Errors
/// Returns [`CoreError::Upstream`] if the JSON is malformed, incomplete, or carries an `Overpass`
/// runtime `remark`.
pub fn parse_alpr_overpass_json(
    payload: &[u8],
    bbox: BBox,
    now: Timestamp,
) -> Result<AlprSnapshot, CoreError> {
    let doc: Value = serde_json::from_slice(payload)
        .map_err(|e| CoreError::Upstream(format!("invalid Overpass ALPR JSON: {e}")))?;

    if doc.get("remark").and_then(Value::as_str).is_some_and(|r| !r.trim().is_empty()) {
        return Err(CoreError::Upstream(
            "Overpass returned an incomplete camera response".to_owned(),
        ));
    }

    let Some(elements) = doc.get("elements").and_then(Value::as_array) else {
        return Err(CoreError::Upstream(
            "Overpass returned an incomplete camera response".to_owned(),
        ));
    };

    let saturated = elements.len() >= ALPR_QUERY_LIMIT;
    let mut seen_ids: HashMap<String, usize> = HashMap::new();
    let mut records: Vec<AlprCamera> = Vec::new();

    for el in elements.iter().take(ALPR_QUERY_LIMIT) {
        if let Some(camera) = normalize_alpr_node(el) {
            if let Some(&idx) = seen_ids.get(&camera.id) {
                records[idx] = camera;
            } else {
                seen_ids.insert(camera.id.clone(), records.len());
                records.push(camera);
            }
        }
    }

    Ok(AlprSnapshot { as_of: now, bbox, stale: false, saturated, records })
}

#[allow(clippy::too_many_lines)]
fn curated_seed_cameras() -> Vec<AlprCamera> {
    struct Seed {
        osm_id: i64,
        lat: f64,
        lon: f64,
        operator: Option<&'static str>,
        manufacturer: Option<&'static str>,
        camera_type: Option<&'static str>,
        zone: Option<&'static str>,
        direction_deg: Option<f64>,
        ref_code: Option<&'static str>,
        last_verified: Option<&'static str>,
        source: Option<&'static str>,
    }

    let seeds = [
        Seed {
            osm_id: 9_841_201_001,
            lat: 30.2672,
            lon: -97.7431,
            operator: Some("Austin Police Department"),
            manufacturer: Some("Flock Safety"),
            camera_type: Some("fixed"),
            zone: Some("traffic"),
            direction_deg: Some(180.0),
            ref_code: Some("ATX-FLK-01"),
            last_verified: None,
            source: Some("survey"),
        },
        Seed {
            osm_id: 9_841_201_002,
            lat: 30.2849,
            lon: -97.7341,
            operator: Some("University of Texas PD"),
            manufacturer: Some("Flock Safety"),
            camera_type: Some("fixed"),
            zone: Some("traffic"),
            direction_deg: Some(90.0),
            ref_code: Some("ATX-FLK-02"),
            last_verified: Some("2026-04-15"),
            source: Some("survey"),
        },
        Seed {
            osm_id: 9_841_201_003,
            lat: 30.2500,
            lon: -97.7500,
            operator: Some("Texas DPS"),
            manufacturer: Some("Motorola Vigilant"),
            camera_type: Some("fixed"),
            zone: Some("traffic"),
            direction_deg: Some(15.0),
            ref_code: Some("TX-DPS-114"),
            last_verified: Some("2026-06-01"),
            source: Some("OpenStreetMap"),
        },
        Seed {
            osm_id: 9_841_201_004,
            lat: 37.7749,
            lon: -122.4194,
            operator: Some("San Francisco Police Department"),
            manufacturer: Some("Flock Safety"),
            camera_type: Some("fixed"),
            zone: Some("traffic"),
            direction_deg: Some(270.0),
            ref_code: Some("SFPD-ALPR-09"),
            last_verified: Some("2026-08-20"),
            source: Some("survey"),
        },
        Seed {
            osm_id: 9_841_201_005,
            lat: 33.7490,
            lon: -84.3880,
            operator: Some("Atlanta Police Department"),
            manufacturer: Some("Flock Safety"),
            camera_type: Some("fixed"),
            zone: Some("traffic"),
            direction_deg: Some(0.0),
            ref_code: Some("APD-FLK-402"),
            last_verified: Some("2026-07-10"),
            source: Some("survey"),
        },
        Seed {
            osm_id: 9_841_201_006,
            lat: 51.5074,
            lon: -0.1278,
            operator: Some("Transport for London"),
            manufacturer: Some("Siemens Mobility"),
            camera_type: Some("fixed"),
            zone: Some("congestion_zone"),
            direction_deg: Some(45.0),
            ref_code: Some("TFL-ULEZ-01"),
            last_verified: Some("2026-09-01"),
            source: Some("survey"),
        },
    ];

    seeds
        .into_iter()
        .filter_map(|s| {
            let position = GeoPoint::new(s.lat, s.lon).ok()?;
            Some(AlprCamera {
                id: format!("alpr:{}", s.osm_id),
                osm_id: s.osm_id,
                position,
                operator: s.operator.map(str::to_owned),
                manufacturer: s.manufacturer.map(str::to_owned),
                camera_type: s.camera_type.map(str::to_owned),
                zone: s.zone.map(str::to_owned),
                direction_deg: s.direction_deg,
                ref_code: s.ref_code.map(str::to_owned),
                last_verified: s.last_verified.map(str::to_owned),
                source: s.source.map(str::to_owned),
            })
        })
        .collect()
}

#[derive(Debug)]
struct AlprStoreState {
    as_of: Timestamp,
    cameras: Vec<AlprCamera>,
}

/// Thread-safe store and query engine for `ALPR` surveillance cameras.
#[derive(Debug)]
pub struct AlprEngine {
    state: RwLock<AlprStoreState>,
}

impl Default for AlprEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AlprEngine {
    /// Creates a new [`AlprEngine`] seeded with curated baseline `ALPR` camera fixtures.
    #[must_use]
    pub fn new() -> Self {
        Self::with_cameras(curated_seed_cameras())
    }

    /// Creates a new [`AlprEngine`] initialized with `cameras`.
    #[must_use]
    pub fn with_cameras(cameras: Vec<AlprCamera>) -> Self {
        Self { state: RwLock::new(AlprStoreState { as_of: Timestamp::now(), cameras }) }
    }

    /// Total number of `ALPR` cameras currently held in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        let guard = self.state.read().unwrap_or_else(PoisonError::into_inner);
        guard.cameras.len()
    }

    /// Whether the `ALPR` store holds zero cameras.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Parses an `Overpass` JSON response, merges the discovered cameras into the engine store,
    /// and returns the resulting [`AlprSnapshot`] for `bbox`.
    ///
    /// # Errors
    /// Returns [`CoreError::Upstream`] if `payload` is invalid JSON or incomplete.
    pub fn ingest_overpass_json(
        &self,
        payload: &[u8],
        bbox: BBox,
        now: Timestamp,
    ) -> Result<AlprSnapshot, CoreError> {
        let snapshot = parse_alpr_overpass_json(payload, bbox, now)?;

        let mut guard = self.state.write().unwrap_or_else(PoisonError::into_inner);
        let mut by_id: HashMap<String, usize> =
            guard.cameras.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();

        for cam in &snapshot.records {
            if let Some(&idx) = by_id.get(&cam.id) {
                guard.cameras[idx] = cam.clone();
            } else {
                let idx = guard.cameras.len();
                by_id.insert(cam.id.clone(), idx);
                guard.cameras.push(cam.clone());
            }
        }
        guard.as_of = now;

        Ok(snapshot)
    }

    /// Queries stored `ALPR` cameras inside `bbox` up to `limit`.
    #[must_use]
    pub fn query(&self, bbox: BBox, limit: usize) -> AlprSnapshot {
        let guard = self.state.read().unwrap_or_else(PoisonError::into_inner);
        let effective_limit = if limit == 0 { ALPR_QUERY_LIMIT } else { limit };

        let matching: Vec<AlprCamera> =
            guard.cameras.iter().filter(|c| bbox.contains(&c.position)).cloned().collect();

        let saturated = matching.len() > effective_limit
            || (matching.len() >= ALPR_QUERY_LIMIT && effective_limit >= ALPR_QUERY_LIMIT);

        let records = matching.into_iter().take(effective_limit).collect();

        AlprSnapshot { as_of: guard.as_of, bbox, stale: false, saturated, records }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn snaps_alpr_box_outward_to_grid_and_clamps_bounds() {
        let bbox = BBox::new(-97.743, 30.267, -97.712, 30.291).unwrap();
        let snapped = snap_alpr_box(bbox, 0.05);
        assert!((snapped.west - (-97.75)).abs() < 1e-9);
        assert!((snapped.south - 30.25).abs() < 1e-9);
        assert!((snapped.east - (-97.70)).abs() < 1e-9);
        assert!((snapped.north - 30.30).abs() < 1e-9);

        let world = BBox::WORLD;
        let snapped_world = snap_alpr_box(world, 0.05);
        assert_eq!(snapped_world, BBox::WORLD);
    }

    #[test]
    fn matches_semicolon_and_case_insensitive_alpr_surveillance_type() {
        assert!(is_alpr_surveillance_type("ALPR"));
        assert!(is_alpr_surveillance_type("alpr"));
        assert!(is_alpr_surveillance_type("camera;ALPR"));
        assert!(is_alpr_surveillance_type("camera; alpr ;fixed"));
        assert!(!is_alpr_surveillance_type("camera"));
        assert!(!is_alpr_surveillance_type("gunshot_detector"));
        assert!(!is_alpr_surveillance_type(""));
    }

    #[test]
    fn normalizes_direction_degrees_without_treating_blank_as_zero() {
        assert_eq!(normalize_direction_deg(""), None);
        assert_eq!(normalize_direction_deg("   "), None);
        assert_eq!(normalize_direction_deg("N"), None);
        assert_eq!(normalize_direction_deg("NE"), None);
        assert_eq!(normalize_direction_deg("-1"), None);
        assert_eq!(normalize_direction_deg("360.1"), None);
        assert_eq!(normalize_direction_deg("0"), Some(0.0));
        assert_eq!(normalize_direction_deg("360"), Some(0.0));
        assert_eq!(normalize_direction_deg("180.5"), Some(180.5));
    }

    #[test]
    fn builds_pinned_overpass_alpr_query() {
        let bbox = BBox::new(-98.0, 30.0, -97.9, 30.1).unwrap();
        let query = build_alpr_overpass_query(bbox, 1500);
        assert_eq!(
            query,
            r#"[out:json][timeout:20];node["man_made"="surveillance"]["surveillance:type"~"(^|;)\s*ALPR\s*(;|$)",i](30,-98,30.1,-97.9);out body 1500;"#
        );
    }

    #[test]
    fn parses_overpass_alpr_response_and_rejects_incomplete_remarks() {
        let bbox = BBox::new(-98.0, 30.0, -97.5, 30.5).unwrap();
        let now: Timestamp = "2026-10-26T10:15:00Z".parse().unwrap();

        let payload = serde_json::to_vec(&json!({
            "elements": [
                {
                    "type": "node",
                    "id": 42,
                    "lat": 30.05,
                    "lon": -97.95,
                    "tags": {
                        "man_made": "surveillance",
                        "surveillance:type": "camera;ALPR",
                        "operator": "Austin PD",
                        "manufacturer": "Flock Safety",
                        "camera:type": "fixed",
                        "surveillance:zone": "traffic",
                        "camera:direction": "360",
                        "ref": "ATX-42",
                        "survey:date": "2026-05-01",
                        "source": "survey"
                    }
                },
                {
                    // Non-ALPR surveillance node should be ignored
                    "type": "node",
                    "id": 43,
                    "lat": 30.06,
                    "lon": -97.94,
                    "tags": {
                        "man_made": "surveillance",
                        "surveillance:type": "camera"
                    }
                },
                {
                    // Duplicate ID replaces in-place without duplicating
                    "type": "node",
                    "id": 42,
                    "lat": 30.05,
                    "lon": -97.95,
                    "tags": {
                        "man_made": "surveillance",
                        "surveillance:type": "ALPR",
                        "operator": "Austin PD Updated",
                        "manufacturer": "Flock Safety",
                        "direction": "90"
                    }
                }
            ]
        }))
        .unwrap();

        let snap = parse_alpr_overpass_json(&payload, bbox, now).unwrap();
        assert!(!snap.stale);
        assert!(!snap.saturated);
        assert_eq!(snap.records.len(), 1);
        assert_eq!(snap.records[0].id, "alpr:42");
        assert_eq!(snap.records[0].osm_id, 42);
        assert_eq!(snap.records[0].operator.as_deref(), Some("Austin PD Updated"));
        assert_eq!(snap.records[0].manufacturer.as_deref(), Some("Flock Safety"));
        assert_eq!(snap.records[0].direction_deg, Some(90.0));

        // Reject payload with Overpass runtime remark
        let remark_payload = serde_json::to_vec(&json!({
            "elements": [],
            "remark": "runtime error: Query timed out in \"query\" at line 1 after 20 seconds."
        }))
        .unwrap();
        assert!(parse_alpr_overpass_json(&remark_payload, bbox, now).is_err());

        // Reject payload without elements array
        let bad_payload = serde_json::to_vec(&json!({"elements": "nope"})).unwrap();
        assert!(parse_alpr_overpass_json(&bad_payload, bbox, now).is_err());
    }

    #[test]
    fn engine_queries_seeded_austin_cameras_and_ingests_overpass_updates() {
        let engine = AlprEngine::new();
        assert!(!engine.is_empty());

        let austin = BBox::new(-97.85, 30.2, -97.65, 30.35).unwrap();
        let snap = engine.query(austin, 100);
        assert!(snap.records.len() >= 3);
        assert!(snap.records.iter().all(|r| r.id.starts_with("alpr:")));
        assert!(snap.records.iter().any(|r| r.manufacturer.as_deref() == Some("Flock Safety")));

        let now: Timestamp = "2026-10-26T10:20:00Z".parse().unwrap();
        let new_payload = serde_json::to_vec(&json!({
            "elements": [
                {
                    "type": "node",
                    "id": 777,
                    "lat": 30.27,
                    "lon": -97.74,
                    "tags": {
                        "surveillance:type": "ALPR",
                        "manufacturer": "Flock Safety"
                    }
                }
            ]
        }))
        .unwrap();

        let ingested = engine.ingest_overpass_json(&new_payload, austin, now).unwrap();
        assert_eq!(ingested.records.len(), 1);

        let updated = engine.query(austin, 100);
        assert!(updated.records.iter().any(|r| r.id == "alpr:777"));
    }
}
