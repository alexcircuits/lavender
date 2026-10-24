//! Flow-to-road geometric matching: assign TomTom congestion levels to OpenStreetMap roads.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::assert_is_empty,
    clippy::must_use_candidate,
    clippy::manual_midpoint,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::collapsible_if
)]

use lavender_domain::TrafficRoadSegment;
use std::collections::HashMap;

use super::mvt::{FlowSegment, RoadFeature};

const M_PER_DEG_LAT: f64 = 111_320.0;
const CELL_SIZE_M: f64 = 100.0;
const MATCH_RADIUS_M: f64 = 35.0;
const BEARING_TOLERANCE_DEG: f64 = 30.0;
const ROAD_SAMPLES: usize = 7;
const MIN_MATCHED_SAMPLES: usize = 2;

/// Compute the median of a slice of floats.
#[must_use]
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    }
}

/// Bearing in degrees (0 = North, clockwise) of a projected `(dx, dy)` vector.
#[inline]
fn bearing_deg(dx: f64, dy: f64) -> f64 {
    dx.atan2(dy).to_degrees()
}

/// Angular difference between two bearings in degrees, range `[0.0, 180.0]`.
#[inline]
fn bearing_diff_deg(a: f64, b: f64) -> f64 {
    let mut d = (a - b).abs() % 360.0;
    if d > 180.0 {
        d = 360.0 - d;
    }
    d
}

/// Squared distance from point `(px, py)` to line segment `(ax, ay)-(bx, by)` in meters².
#[inline]
fn point_seg_dist2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    let t =
        if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    (px - qx) * (px - qx) + (py - qy) * (py - qy)
}

#[derive(Clone, Copy)]
struct IndexedSegment {
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    bearing: f64,
    both_directions: bool,
    level: f64,
    closure: bool,
}

struct FlowSpatialIndex {
    ref_lon: f64,
    ref_lat: f64,
    m_per_deg_lon: f64,
    grid: HashMap<(i32, i32), Vec<IndexedSegment>>,
}

impl FlowSpatialIndex {
    fn new(flow_segments: &[FlowSegment]) -> Option<Self> {
        let anchor = flow_segments.iter().find(|f| f.coords.len() >= 2)?;
        let ref_lon = anchor.coords[0][0];
        let ref_lat = anchor.coords[0][1];
        let m_per_deg_lon = M_PER_DEG_LAT * ref_lat.to_radians().cos();

        let mut grid: HashMap<(i32, i32), Vec<IndexedSegment>> = HashMap::new();

        for flow in flow_segments {
            if flow.coords.len() < 2 {
                continue;
            }
            let level = flow.traffic_level;
            let closure = flow.closure;
            let both_directions = flow.coverage.as_deref() == Some("full");

            for window in flow.coords.windows(2) {
                let ax = (window[0][0] - ref_lon) * m_per_deg_lon;
                let ay = (window[0][1] - ref_lat) * M_PER_DEG_LAT;
                let bx = (window[1][0] - ref_lon) * m_per_deg_lon;
                let by = (window[1][1] - ref_lat) * M_PER_DEG_LAT;

                let seg_len = (bx - ax).hypot(by - ay);
                if !(seg_len > 0.0) {
                    continue;
                }
                let bearing = bearing_deg(bx - ax, by - ay);

                let pieces = ((seg_len / CELL_SIZE_M).ceil() as usize).max(1);
                for p in 0..pieces {
                    let t0 = (p as f64) / (pieces as f64);
                    let t1 = ((p + 1) as f64) / (pieces as f64);
                    let sub_ax = ax + (bx - ax) * t0;
                    let sub_ay = ay + (by - ay) * t0;
                    let sub_bx = ax + (bx - ax) * t1;
                    let sub_by = ay + (by - ay) * t1;

                    let seg = IndexedSegment {
                        ax: sub_ax,
                        ay: sub_ay,
                        bx: sub_bx,
                        by: sub_by,
                        bearing,
                        both_directions,
                        level,
                        closure,
                    };

                    let mx = (seg.ax + seg.bx) / 2.0;
                    let my = (seg.ay + seg.by) / 2.0;
                    let cx = (mx / CELL_SIZE_M).floor() as i32;
                    let cy = (my / CELL_SIZE_M).floor() as i32;

                    grid.entry((cx, cy)).or_default().push(seg);
                }
            }
        }

        if grid.is_empty() { None } else { Some(Self { ref_lon, ref_lat, m_per_deg_lon, grid }) }
    }

    fn project(&self, lon: f64, lat: f64) -> (f64, f64) {
        ((lon - self.ref_lon) * self.m_per_deg_lon, (lat - self.ref_lat) * M_PER_DEG_LAT)
    }

    fn nearest(&self, px: f64, py: f64, bearing: f64) -> (Option<IndexedSegment>, bool, bool) {
        let cx = (px / CELL_SIZE_M).floor() as i32;
        let cy = (py / CELL_SIZE_M).floor() as i32;
        let radius2 = MATCH_RADIUS_M * MATCH_RADIUS_M;

        let mut best: Option<IndexedSegment> = None;
        let mut best_dist2 = radius2;
        let mut ambiguous = false;
        let mut candidate = false;

        for gy in (cy - 1)..=(cy + 1) {
            for gx in (cx - 1)..=(cx + 1) {
                if let Some(bucket) = self.grid.get(&(gx, gy)) {
                    for seg in bucket {
                        let d2 = point_seg_dist2(px, py, seg.ax, seg.ay, seg.bx, seg.by);
                        if d2 > radius2 {
                            continue;
                        }
                        candidate = true;

                        let diff = bearing_diff_deg(seg.bearing, bearing);
                        let matches_dir = diff < BEARING_TOLERANCE_DEG
                            || (seg.both_directions && (180.0 - diff) < BEARING_TOLERANCE_DEG);
                        if !matches_dir {
                            continue;
                        }

                        if d2 < best_dist2 - 1.0 {
                            best_dist2 = d2;
                            best = Some(*seg);
                            ambiguous = false;
                        } else if (d2 - best_dist2).abs() <= 1.0 {
                            if let Some(b) = best {
                                if (b.level - seg.level).abs() > 1e-6 || b.closure != seg.closure {
                                    ambiguous = true;
                                }
                            } else {
                                best = Some(*seg);
                            }
                        }
                    }
                }
            }
        }

        (best, ambiguous, candidate)
    }
}

/// Matching outcome details.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Matched road segments with traffic congestion level and closure status.
    pub matched_roads: Vec<TrafficRoadSegment>,
    /// Number of roads that matched flow data.
    pub matched_count: usize,
    /// Number of candidate roads with nearby flow segments.
    pub candidate_count: usize,
}

/// Match decoded TomTom flow segments onto OpenStreetMap road features.
#[must_use]
pub fn match_flow_to_roads(roads: &[RoadFeature], flow_segments: &[FlowSegment]) -> MatchResult {
    if roads.is_empty() {
        return MatchResult { matched_roads: Vec::new(), matched_count: 0, candidate_count: 0 };
    }

    let Some(index) = FlowSpatialIndex::new(flow_segments) else {
        return MatchResult { matched_roads: Vec::new(), matched_count: 0, candidate_count: 0 };
    };

    let mut matched_roads = Vec::new();
    let mut matched_count = 0;
    let mut candidate_count = 0;

    for road in roads {
        if road.coords.len() < 2 {
            continue;
        }

        // Project road and accumulate arc length
        let mut xs = Vec::with_capacity(road.coords.len());
        let mut ys = Vec::with_capacity(road.coords.len());
        let mut cum = Vec::with_capacity(road.coords.len());
        cum.push(0.0);

        for (i, pt) in road.coords.iter().enumerate() {
            let (x, y) = index.project(pt[0], pt[1]);
            xs.push(x);
            ys.push(y);
            if i > 0 {
                let d = (x - xs[i - 1]).hypot(y - ys[i - 1]);
                let prev = cum[i - 1];
                cum.push(prev + d);
            }
        }

        let total_len = *cum.last().unwrap_or(&0.0);
        if !(total_len > 0.0) {
            continue;
        }

        let mut had_candidate = false;
        let mut matched_levels = Vec::new();
        let mut matched_closure = false;

        let mut cursor = 1;
        for s in 0..ROAD_SAMPLES {
            let target = (total_len * (s as f64 + 0.5)) / (ROAD_SAMPLES as f64);
            while cursor < road.coords.len() - 1 && cum[cursor] < target {
                cursor += 1;
            }

            let span = cum[cursor] - cum[cursor - 1];
            let seg_t = if span > 0.0 { (target - cum[cursor - 1]) / span } else { 0.0 };

            let px = xs[cursor - 1] + (xs[cursor] - xs[cursor - 1]) * seg_t;
            let py = ys[cursor - 1] + (ys[cursor] - ys[cursor - 1]) * seg_t;

            let sample_bearing = (if road.oneway == -1 { 180.0 } else { 0.0 })
                + bearing_deg(xs[cursor] - xs[cursor - 1], ys[cursor] - ys[cursor - 1]);

            let (best, ambiguous, candidate) = index.nearest(px, py, sample_bearing);
            if candidate {
                had_candidate = true;
            }
            if let Some(b) = best {
                if !ambiguous {
                    matched_levels.push(b.level);
                    if b.closure {
                        matched_closure = true;
                    }
                }
            }
        }

        if had_candidate {
            candidate_count += 1;
        }

        let min_required = MIN_MATCHED_SAMPLES.max(ROAD_SAMPLES / 2);
        if matched_levels.len() >= min_required {
            let level = median(&matched_levels).unwrap_or(0.0);
            matched_roads.push(TrafficRoadSegment {
                coords: road.coords.clone(),
                traffic_level: level,
                closure: matched_closure,
                road_type: Some(road.road_type.clone()),
            });
            matched_count += 1;
        }
    }

    MatchResult { matched_roads, matched_count, candidate_count }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::traffic::mvt;

    const LAT0: f64 = 30.26;
    const LON0: f64 = -97.7431;

    fn build_line(length_m: f64, bearing_deg: f64, north_m: f64, east_m: f64) -> Vec<[f64; 2]> {
        let m_per_deg_lon = M_PER_DEG_LAT * LAT0.to_radians().cos();
        let start_lat = LAT0 + north_m / M_PER_DEG_LAT;
        let start_lon = LON0 + east_m / m_per_deg_lon;
        let steps = (length_m / 25.0).round().max(1.0) as usize;
        let mut coords = Vec::with_capacity(steps + 1);
        let rad = bearing_deg.to_radians();

        for i in 0..=steps {
            let d = (length_m * i as f64) / (steps as f64);
            coords.push([
                start_lon + (rad.sin() * d) / m_per_deg_lon,
                start_lat + (rad.cos() * d) / M_PER_DEG_LAT,
            ]);
        }
        coords
    }

    #[test]
    fn test_coincident_parallel_road_matches() {
        let road = RoadFeature {
            coords: build_line(500.0, 0.0, 0.0, 0.0),
            road_type: "primary".to_string(),
            oneway: 0,
            drivable: true,
        };
        let flow = FlowSegment {
            coords: build_line(500.0, 0.0, 0.0, 4.0),
            traffic_level: 0.42,
            road_type: "Major road".to_string(),
            closure: false,
            coverage: None,
        };

        let res = match_flow_to_roads(&[road], &[flow]);
        assert_eq!(res.matched_count, 1);
        assert_eq!(res.candidate_count, 1);
        assert_eq!(res.matched_roads.len(), 1);
        assert!((res.matched_roads[0].traffic_level - 0.42).abs() < 1e-4);
        assert!(!res.matched_roads[0].closure);
    }

    #[test]
    fn test_perpendicular_decoy_does_not_match() {
        let road = RoadFeature {
            coords: build_line(500.0, 0.0, 0.0, 0.0),
            road_type: "primary".to_string(),
            oneway: 0,
            drivable: true,
        };
        let decoy = FlowSegment {
            coords: build_line(500.0, 90.0, 250.0, -250.0),
            traffic_level: 0.1,
            road_type: "Major road".to_string(),
            closure: false,
            coverage: None,
        };

        let res = match_flow_to_roads(&[road], &[decoy]);
        assert_eq!(res.matched_count, 0);
        assert_eq!(res.candidate_count, 1);
        assert!(res.matched_roads.is_empty());
    }

    #[test]
    fn test_closure_propagation() {
        let road = RoadFeature {
            coords: build_line(500.0, 0.0, 0.0, 0.0),
            road_type: "primary".to_string(),
            oneway: 0,
            drivable: true,
        };
        let open_half = FlowSegment {
            coords: build_line(250.0, 0.0, 0.0, 2.0),
            traffic_level: 0.9,
            road_type: "Major road".to_string(),
            closure: false,
            coverage: None,
        };
        let closed_half = FlowSegment {
            coords: build_line(250.0, 0.0, 250.0, 2.0),
            traffic_level: 0.0,
            road_type: "Major road".to_string(),
            closure: true,
            coverage: None,
        };

        let res = match_flow_to_roads(&[road], &[open_half, closed_half]);
        assert_eq!(res.matched_count, 1);
        assert!(res.matched_roads[0].closure);
    }

    #[test]
    fn test_matching_on_recorded_fixtures() {
        let flow_bytes = include_bytes!(
            "../../../../../legacy/src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf"
        );
        let ofm_bytes =
            include_bytes!("../../../../../legacy/src/data/fixtures/ofm-austin-12-935-1686.pbf");

        let flows =
            mvt::decode_tomtom_flow_tile(flow_bytes, 12, 935, 1686).expect("tomtom tile decode");
        let roads =
            mvt::decode_openfreemap_roads(ofm_bytes, 12, 935, 1686).expect("ofm roads decode");

        let res = match_flow_to_roads(&roads, &flows);
        assert!(res.matched_count > 0, "should match at least one road in recorded Austin fixture");
        assert!(res.candidate_count >= res.matched_count);
    }
}
