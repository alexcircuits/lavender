//! `KartaView` and `Panoramax` coverage decoding (`LB-116`), porting
//! `legacy/src/layers/streetLevel/providers/{kartaview,panoramax}/decode.js`.

use std::collections::HashMap;

use lavender_domain::{StreetLevelCoverageTile, StreetLevelProviderId, StreetLevelSequence};
use serde_json::Value;

use crate::error::CoreError;

/// More than any real tile holds (a dense z13 tile has about 4,700 segments).
pub const MAX_TILE_SEGMENTS: usize = 20_000;
/// A chained line stops growing here and the next one starts.
pub const MAX_LINE_POINTS: usize = 5_000;

const MIN_CAPTURE_MS: i64 = 946_684_800_000;
const MAX_FUTURE_MS: i64 = 86_400_000;

/// Zeroes capture timestamps before 2000-01-01 or more than 24 h in the future (upstream #1341).
#[must_use]
pub fn sanitize_capture_timestamp_ms(raw_ms: i64, now_ms: i64) -> i64 {
    if raw_ms < MIN_CAPTURE_MS || raw_ms > now_ms.saturating_add(MAX_FUTURE_MS) {
        0
    } else {
        raw_ms
    }
}

fn tile_segments(payload: &Value) -> Result<Vec<Vec<[f64; 2]>>, CoreError> {
    let lines = payload
        .get("lines")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Upstream("KartaView sent an unreadable coverage tile".into()))?;
    let mut segments = Vec::new();
    for line in lines {
        if segments.len() >= MAX_TILE_SEGMENTS {
            break;
        }
        let mut points: Vec<[f64; 2]> = Vec::new();
        for point in line.get("track").and_then(Value::as_array).into_iter().flatten() {
            let lon = point.get("lng").and_then(Value::as_f64);
            let lat = point.get("lat").and_then(Value::as_f64);
            let (Some(lon), Some(lat)) = (lon, lat) else { continue };
            if !lon.is_finite() || !lat.is_finite() {
                continue;
            }
            if points.last().is_some_and(|l| key(*l) == key([lon, lat])) {
                continue;
            }
            points.push([lon, lat]);
        }
        if points.len() >= 2 {
            segments.push(points);
        }
    }
    Ok(segments)
}

fn key(p: [f64; 2]) -> (u64, u64) {
    (p[0].to_bits(), p[1].to_bits())
}

/// Chains segments that meet end to start, cutting the line count about fourfold.
#[must_use]
pub fn chain_segments(segments: &[Vec<[f64; 2]>]) -> Vec<Vec<[f64; 2]>> {
    let mut starting_at: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    for (i, seg) in segments.iter().enumerate() {
        if let Some(first) = seg.first() {
            starting_at.entry(key(*first)).or_default().push(i);
        }
    }
    let mut used = vec![false; segments.len()];
    let mut cursor: HashMap<(u64, u64), usize> = HashMap::new();
    let mut chains = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if used[index] {
            continue;
        }
        used[index] = true;
        let mut chain = segment.clone();
        while let Some(last) = chain.last().copied() {
            let at = key(last);
            let Some(list) = starting_at.get(&at) else { break };
            let mut i = cursor.get(&at).copied().unwrap_or(0);
            while i < list.len() && used[list[i]] {
                i += 1;
            }
            cursor.insert(at, i);
            let Some(&next) = list.get(i) else { break };
            if chain.len() + segments[next].len() - 1 > MAX_LINE_POINTS {
                break;
            }
            used[next] = true;
            chain.extend_from_slice(&segments[next][1..]);
        }
        chains.push(chain);
    }
    chains
}

/// Decodes a `KartaView` coverage tile into chained sequences.
///
/// # Errors
/// Returns [`CoreError::Upstream`] when the payload has no `lines` array.
pub fn decode_kartaview_tile(
    payload: &Value,
    z: u32,
    x: u32,
    y: u32,
) -> Result<StreetLevelCoverageTile, CoreError> {
    let chains = chain_segments(&tile_segments(payload)?);
    let sequences = chains
        .into_iter()
        .enumerate()
        .map(|(i, chain)| StreetLevelSequence {
            id: format!("{z}/{x}/{y}/{i}"),
            provider: StreetLevelProviderId::Kartaview,
            parts: vec![chain],
            captured_at_ms: 0,
            is_pano: Some(false),
        })
        .collect();
    Ok(StreetLevelCoverageTile { z, x, y, provider: StreetLevelProviderId::Kartaview, sequences })
}

/// Douglas–Peucker keeping points beyond `tolerance` of the finite segment `[a, b]`, so dead
/// ends and U-turns survive; first and last always kept. Iterative (no stack overflow).
#[must_use]
pub fn simplify_tile_line(points: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if points.len() < 3 || tolerance.is_nan() || tolerance <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut pending = vec![(0_usize, points.len() - 1)];
    while let Some((from, to)) = pending.pop() {
        let a = points[from];
        let b = points[to];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let span_len_sq = dx * dx + dy * dy;
        let mut far = None;
        let mut farthest = tolerance;
        for (i, p) in points.iter().enumerate().take(to).skip(from + 1) {
            let t = if span_len_sq > 0.0 {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / span_len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy));
            if d > farthest {
                farthest = d;
                far = Some(i);
            }
        }
        if let Some(f) = far {
            keep[f] = true;
            pending.push((from, f));
            pending.push((f, to));
        }
    }
    points.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timestamps_outside_the_plausible_window_are_zeroed() {
        let now = 1_760_000_000_000;
        assert_eq!(sanitize_capture_timestamp_ms(1_700_000_000_000, now), 1_700_000_000_000);
        assert_eq!(sanitize_capture_timestamp_ms(0, now), 0);
        assert_eq!(sanitize_capture_timestamp_ms(900_000_000_000, now), 0);
        assert_eq!(sanitize_capture_timestamp_ms(now + 2 * MAX_FUTURE_MS, now), 0);
        assert_eq!(sanitize_capture_timestamp_ms(now + 1000, now), now + 1000);
    }

    #[test]
    fn kartaview_segments_that_meet_are_chained() {
        let payload = json!({"lines": [
            {"track": [{"lat": 0.0, "lng": 0.0}, {"lat": 1.0, "lng": 1.0}]},
            {"track": [{"lat": 1.0, "lng": 1.0}, {"lat": 2.0, "lng": 2.0}]},
            {"track": [{"lat": 5.0, "lng": 5.0}, {"lat": 5.0, "lng": 5.0}]},
        ]});
        let tile = decode_kartaview_tile(&payload, 13, 1, 2).unwrap();
        assert_eq!(tile.sequences.len(), 1);
        assert_eq!(tile.sequences[0].parts[0].len(), 3);
        assert_eq!(tile.sequences[0].id, "13/1/2/0");
    }

    #[test]
    fn unreadable_kartaview_tile_is_an_error() {
        assert!(decode_kartaview_tile(&json!({}), 1, 0, 0).is_err());
    }

    #[test]
    fn simplify_drops_collinear_points_but_keeps_u_turns() {
        let straight = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        assert_eq!(simplify_tile_line(&straight, 0.1).len(), 2);
        let u_turn = [[0.0, 0.0], [10.0, 0.0], [0.5, 0.0]];
        assert_eq!(simplify_tile_line(&u_turn, 0.1).len(), 3);
    }
}
