//! Mapbox Vector Tile (MVT) parsing, geometry reconstruction, and feature extraction.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::collapsible_if,
    clippy::unnecessary_map_or,
    clippy::bool_to_int_with_if
)]

use prost::Message;
use std::collections::HashMap;
use std::f64::consts::PI;

use super::proto::{Feature, Layer, Tile, Value};

/// Decoded flow segment from a TomTom traffic flow vector tile.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowSegment {
    /// Geographic coordinates as `[lon, lat]` in degrees.
    pub coords: Vec<[f64; 2]>,
    /// Congestion level in range [0.0, 1.0], 1.0 = free-flow.
    pub traffic_level: f64,
    /// Road type string.
    pub road_type: String,
    /// Whether road closure is indicated.
    pub closure: bool,
    /// Traffic road coverage (e.g. "full" for both directions).
    pub coverage: Option<String>,
}

/// Decoded drivable road from an OpenFreeMap transportation vector tile.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadFeature {
    /// Geographic coordinates as `[lon, lat]` in degrees.
    pub coords: Vec<[f64; 2]>,
    /// Road classification (e.g. "motorway", "primary", "secondary").
    pub road_type: String,
    /// Oneway indicator: 1 = forward only, 0 = two-way.
    pub oneway: i32,
    /// Whether the road is drivable.
    pub drivable: bool,
}

#[inline]
fn zigzag_decode(n: u32) -> i32 {
    ((n >> 1) as i32) ^ (-((n & 1) as i32))
}

/// Convert tile grid coordinate `(cx, cy)` to `[longitude, latitude]` in degrees.
#[inline]
fn unproject_point(cx: i64, cy: i64, extent: u32, z: u32, x: u32, y: u32) -> [f64; 2] {
    let ext = f64::from(extent);
    let n = 2.0_f64.powi(z as i32);

    let tx = f64::from(x) + (cx as f64) / ext;
    let ty = f64::from(y) + (cy as f64) / ext;

    let lon = (tx / n) * 360.0 - 180.0;
    let sinh_param = PI - 2.0 * PI * ty / n;
    let lat = (sinh_param.sinh().atan() * 180.0) / PI;

    [lon, lat]
}

/// Decode linestring geometry commands from an MVT feature.
pub fn decode_feature_linestrings(
    geometry: &[u32],
    extent: u32,
    z: u32,
    x: u32,
    y: u32,
) -> Vec<Vec<[f64; 2]>> {
    let mut lines = Vec::new();
    let mut current_line: Vec<[f64; 2]> = Vec::new();
    let mut cursor_x: i64 = 0;
    let mut cursor_y: i64 = 0;

    let mut idx = 0;
    while idx < geometry.len() {
        let cmd_integer = geometry[idx];
        let cmd = cmd_integer & 0x7;
        let count = (cmd_integer >> 3) as usize;
        idx += 1;

        match cmd {
            1 => {
                // MoveTo
                for _ in 0..count {
                    if idx + 1 >= geometry.len() {
                        break;
                    }
                    let dx = zigzag_decode(geometry[idx]);
                    let dy = zigzag_decode(geometry[idx + 1]);
                    idx += 2;
                    cursor_x += i64::from(dx);
                    cursor_y += i64::from(dy);

                    if current_line.len() >= 2 {
                        lines.push(std::mem::take(&mut current_line));
                    } else {
                        current_line.clear();
                    }
                    current_line.push(unproject_point(cursor_x, cursor_y, extent, z, x, y));
                }
            }
            2 => {
                // LineTo
                for _ in 0..count {
                    if idx + 1 >= geometry.len() {
                        break;
                    }
                    let dx = zigzag_decode(geometry[idx]);
                    let dy = zigzag_decode(geometry[idx + 1]);
                    idx += 2;
                    cursor_x += i64::from(dx);
                    cursor_y += i64::from(dy);

                    current_line.push(unproject_point(cursor_x, cursor_y, extent, z, x, y));
                }
            }
            7 => {
                // ClosePath
                if current_line.len() >= 2 {
                    if let Some(&first) = current_line.first() {
                        current_line.push(first);
                    }
                    lines.push(std::mem::take(&mut current_line));
                }
            }
            _ => {
                // Unknown command, skip
                break;
            }
        }
    }

    if current_line.len() >= 2 {
        lines.push(current_line);
    }

    lines
}

/// Extract attribute map for a feature.
fn extract_feature_properties<'a>(
    feature: &Feature,
    layer: &'a Layer,
) -> HashMap<&'a str, &'a Value> {
    let mut props = HashMap::new();
    let mut i = 0;
    while i + 1 < feature.tags.len() {
        let key_idx = feature.tags[i] as usize;
        let val_idx = feature.tags[i + 1] as usize;
        i += 2;

        if key_idx < layer.keys.len() && val_idx < layer.values.len() {
            props.insert(layer.keys[key_idx].as_str(), &layer.values[val_idx]);
        }
    }
    props
}

fn value_to_f64(val: &Value) -> Option<f64> {
    if let Some(f) = val.float_value {
        return Some(f64::from(f));
    }
    if let Some(d) = val.double_value {
        return Some(d);
    }
    if let Some(i) = val.int_value {
        return Some(i as f64);
    }
    if let Some(u) = val.uint_value {
        return Some(u as f64);
    }
    if let Some(s) = val.sint_value {
        return Some(s as f64);
    }
    None
}

fn value_to_bool(val: &Value) -> bool {
    if let Some(b) = val.bool_value {
        return b;
    }
    if let Some(ref s) = val.string_value {
        return s.eq_ignore_ascii_case("true") || s == "1";
    }
    if let Some(i) = val.int_value {
        return i != 0;
    }
    false
}

fn value_to_string(val: &Value) -> Option<String> {
    val.string_value.clone()
}

/// Decodes flow segments from a TomTom flow MVT tile.
pub fn decode_tomtom_flow_tile(
    bytes: &[u8],
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<FlowSegment>, String> {
    let tile = Tile::decode(bytes).map_err(|e| format!("MVT decode error: {e}"))?;
    let Some(layer) = tile.layers.iter().find(|l| l.name == "Traffic flow") else {
        return Ok(Vec::new());
    };

    let extent = layer.extent.unwrap_or(4096);
    let mut segments = Vec::new();

    for feature in &layer.features {
        let props = extract_feature_properties(feature, layer);

        let closure = props.get("road_closure").map_or(false, |v| value_to_bool(v));

        let raw_level = props.get("traffic_level").and_then(|v| value_to_f64(v));

        if raw_level.is_none() && !closure {
            continue;
        }

        let traffic_level = raw_level.map_or(0.0, |l| l.clamp(0.0, 1.0));
        let road_type = props.get("road_type").and_then(|v| value_to_string(v)).unwrap_or_default();
        let coverage = props.get("traffic_road_coverage").and_then(|v| value_to_string(v));

        let lines = decode_feature_linestrings(&feature.geometry, extent, z, x, y);
        for coords in lines {
            if coords.len() >= 2 {
                segments.push(FlowSegment {
                    coords,
                    traffic_level,
                    road_type: road_type.clone(),
                    closure,
                    coverage: coverage.clone(),
                });
            }
        }
    }

    Ok(segments)
}

/// Check if an OpenMapTiles transportation feature represents a drivable road.
fn is_drivable_open_map_road(props: &HashMap<&str, &Value>) -> bool {
    let Some(class_val) = props.get("class").and_then(|v| v.string_value.as_deref()) else {
        return false;
    };

    let is_valid_class =
        matches!(class_val, "motorway" | "trunk" | "primary" | "secondary" | "tertiary" | "minor");
    if !is_valid_class {
        return false;
    }

    if props.contains_key("subclass") || props.contains_key("service") {
        return false;
    }

    if let Some(access) = props.get("access").and_then(|v| v.string_value.as_deref()) {
        if matches!(
            access,
            "no" | "private"
                | "destination"
                | "customers"
                | "delivery"
                | "agricultural"
                | "forestry"
        ) {
            return false;
        }
    }

    if let Some(brunnel) = props.get("brunnel").and_then(|v| v.string_value.as_deref()) {
        if brunnel == "tunnel" {
            return false;
        }
    }

    if let Some(indoor) = props.get("indoor") {
        if value_to_bool(indoor) {
            return false;
        }
    }

    true
}

/// Clip a polyline to bounding box `(west, south, east, north)`.
pub fn clip_tile_line(
    coords: &[[f64; 2]],
    west: f64,
    south: f64,
    east: f64,
    north: f64,
) -> Vec<Vec<[f64; 2]>> {
    let mut lines = Vec::new();
    let mut line: Vec<[f64; 2]> = Vec::new();

    for window in coords.windows(2) {
        let a = window[0];
        let b = window[1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];

        let mut lo: f64 = 0.0;
        let mut hi: f64 = 1.0;

        let p = [-dx, dx, -dy, dy];
        let q = [a[0] - west, east - a[0], a[1] - south, north - a[1]];

        let mut valid = true;
        for k in 0..4 {
            if p[k].abs() < 1e-15 {
                if q[k] < 0.0 {
                    valid = false;
                    break;
                }
            } else if p[k] < 0.0 {
                lo = lo.max(q[k] / p[k]);
            } else {
                hi = hi.min(q[k] / p[k]);
            }
        }

        if !valid || lo > hi {
            if line.len() > 1 {
                lines.push(std::mem::take(&mut line));
            } else {
                line.clear();
            }
            continue;
        }

        let start = [a[0] + lo * dx, a[1] + lo * dy];
        let end = [a[0] + hi * dx, a[1] + hi * dy];

        if let Some(&last) = line.last() {
            if (last[0] - start[0]).abs() + (last[1] - start[1]).abs() > 1e-10 {
                if line.len() > 1 {
                    lines.push(std::mem::take(&mut line));
                } else {
                    line.clear();
                }
                line.push(start);
            }
        } else {
            line.push(start);
        }
        line.push(end);
    }

    if line.len() > 1 {
        lines.push(line);
    }

    // Filter short fragments (< 12 meters)
    lines.retain(|pts| {
        let mut metres = 0.0;
        for w in pts.windows(2) {
            let cos_lat = (w[1][1].to_radians()).cos();
            let d_lon_m = (w[1][0] - w[0][0]) * cos_lat * 111_320.0;
            let d_lat_m = (w[1][1] - w[0][1]) * 111_320.0;
            metres += d_lon_m.hypot(d_lat_m);
        }
        metres >= 12.0
    });

    lines
}

/// Decodes drivable roads from an OpenFreeMap transportation vector tile.
pub fn decode_openfreemap_roads(
    bytes: &[u8],
    z: u32,
    x: u32,
    y: u32,
) -> Result<Vec<RoadFeature>, String> {
    let tile = Tile::decode(bytes).map_err(|e| format!("MVT decode error: {e}"))?;
    let Some(layer) = tile.layers.iter().find(|l| l.name == "transportation") else {
        return Ok(Vec::new());
    };

    let extent = layer.extent.unwrap_or(4096);
    let (west, south, east, north) = super::tiles::tile_to_bbox(z, x, y);
    let mut roads = Vec::new();

    for feature in &layer.features {
        let props = extract_feature_properties(feature, layer);
        if !is_drivable_open_map_road(&props) {
            continue;
        }

        let road_type = match props.get("class").and_then(|v| v.string_value.as_deref()) {
            Some("minor") => "residential".to_string(),
            Some(c) => c.to_string(),
            None => continue,
        };

        let oneway_val = props.get("oneway").and_then(|v| v.int_value).unwrap_or(0);
        let reverse = oneway_val == -1;
        let oneway = if reverse || oneway_val == 1 { 1 } else { 0 };

        let lines = decode_feature_linestrings(&feature.geometry, extent, z, x, y);
        for coords in lines {
            let clipped_lines = clip_tile_line(&coords, west, south, east, north);
            for mut line in clipped_lines {
                if line.len() >= 2 {
                    if reverse {
                        line.reverse();
                    }
                    roads.push(RoadFeature {
                        coords: line,
                        road_type: road_type.clone(),
                        oneway,
                        drivable: true,
                    });
                }
            }
        }
    }

    Ok(roads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zigzag_decode() {
        assert_eq!(zigzag_decode(0), 0);
        assert_eq!(zigzag_decode(1), -1);
        assert_eq!(zigzag_decode(2), 1);
        assert_eq!(zigzag_decode(3), -2);
        assert_eq!(zigzag_decode(4), 2);
    }

    #[test]
    fn test_decode_real_tomtom_fixture() {
        let fixture_bytes = include_bytes!(
            "../../../../../legacy/src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf"
        );
        let segments = decode_tomtom_flow_tile(fixture_bytes, 12, 935, 1686)
            .expect("should decode TomTom fixture");
        assert!(!segments.is_empty(), "should find traffic flow segments in fixture");
        for seg in &segments {
            assert!(seg.coords.len() >= 2);
            assert!(seg.traffic_level >= 0.0 && seg.traffic_level <= 1.0);
        }
    }

    #[test]
    fn test_decode_real_openfreemap_fixture() {
        let fixture_bytes =
            include_bytes!("../../../../../legacy/src/data/fixtures/ofm-austin-12-935-1686.pbf");
        let roads = decode_openfreemap_roads(fixture_bytes, 12, 935, 1686)
            .expect("should decode OpenFreeMap fixture");
        assert!(!roads.is_empty(), "should find roads in fixture");
        for road in &roads {
            assert!(road.coords.len() >= 2);
            assert!(road.drivable);
        }
    }
}
