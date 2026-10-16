//! Polygon ring cleaning, measurement and simplification for map display.
//!
//! Positions are GeoJSON `[longitude, latitude]` pairs in degrees, and rings are closed:
//! the first position is repeated at the end. Simplification measures distance in a local
//! equirectangular projection around the ring's mean latitude, so a tolerance in metres
//! means about the same on the ground in Florida and in Alaska. Rings that cross the
//! antimeridian are not unwrapped first; they are simplified as written.

use crate::geodesy::EARTH_RADIUS_M;

/// A GeoJSON position: `[longitude, latitude]` in degrees.
pub type Position = [f64; 2];

/// Metres along a meridian per degree of latitude on the mean-radius sphere.
pub const METRES_PER_DEGREE: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

/// Rings enclosing less than this many square degrees (about one square centimetre at
/// the equator) are treated as having no area at all.
const MIN_RING_AREA_DEG2: f64 = 1e-14;

/// The ring's area in square degrees: positive when counter-clockwise, negative when
/// clockwise (shoelace formula; the ring may be open or closed).
#[must_use]
pub fn signed_area(ring: &[Position]) -> f64 {
    let Some(origin) = ring.first() else { return 0.0 };
    // Measuring from the first position keeps the products small and precise.
    let relative = |p: &Position| (p[0] - origin[0], p[1] - origin[1]);
    let mut twice_area = 0.0;
    for pair in ring.windows(2) {
        let (x1, y1) = relative(&pair[0]);
        let (x2, y2) = relative(&pair[1]);
        twice_area += x1 * y2 - x2 * y1;
    }
    twice_area / 2.0
}

/// Removes repeated consecutive positions and orients the ring as RFC 7946 asks:
/// exteriors counter-clockwise, holes clockwise. Returns `None` for a ring that is not
/// closed, has fewer than three distinct corners, or encloses no area.
#[must_use]
pub fn clean_ring(ring: &[Position], exterior: bool) -> Option<Vec<Position>> {
    if ring.len() < 4 || ring.first() != ring.last() {
        return None;
    }
    let mut cleaned: Vec<Position> = Vec::with_capacity(ring.len());
    for position in ring {
        if cleaned.last() != Some(position) {
            cleaned.push(*position);
        }
    }
    let area = signed_area(&cleaned);
    if cleaned.len() < 4 || area.abs() < MIN_RING_AREA_DEG2 {
        return None;
    }
    if (area > 0.0) != exterior {
        cleaned.reverse();
    }
    Some(cleaned)
}

/// Douglas–Peucker simplification of a closed ring. Every removed position lies within
/// `tolerance_m` metres of the simplified outline, and the ring keeps at least a
/// triangle so a small fire never vanishes. Returns `None` for a ring that is not closed
/// or whose positions all lie on one line.
#[must_use]
pub fn simplify_ring(ring: &[Position], tolerance_m: f64) -> Option<Vec<Position>> {
    if ring.len() < 4 || ring.first() != ring.last() {
        return None;
    }
    let points = project(ring);
    let closing = points.len() - 1;
    // Split at the corner farthest from the start, so both halves are ordinary lines.
    let split = (1..closing).max_by(|&a, &b| {
        distance(points[0], points[a]).total_cmp(&distance(points[0], points[b]))
    })?;
    if distance(points[0], points[split]) == 0.0 {
        return None;
    }
    let tolerance = tolerance_m.max(0.0) / METRES_PER_DEGREE;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[split] = true;
    keep[closing] = true;
    mark_kept(&points, 0, split, tolerance, &mut keep);
    mark_kept(&points, split, closing, tolerance, &mut keep);
    if keep.iter().filter(|k| **k).count() < 4 {
        // Only the chord is left: add the corner farthest from it to keep an area.
        let (corner, offset) = (1..closing)
            .filter(|&i| i != split)
            .map(|i| (i, distance_to_segment(points[i], points[0], points[split])))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        if offset == 0.0 {
            return None;
        }
        keep[corner] = true;
    }
    Some(ring.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect())
}

/// Douglas–Peucker simplification of an open line; both ends are always kept and every
/// removed position lies within `tolerance_m` metres of the simplified line.
#[must_use]
pub fn simplify_line(line: &[Position], tolerance_m: f64) -> Vec<Position> {
    if line.len() < 3 {
        return line.to_vec();
    }
    let points = project(line);
    let last = points.len() - 1;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[last] = true;
    mark_kept(&points, 0, last, tolerance_m.max(0.0) / METRES_PER_DEGREE, &mut keep);
    line.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

/// The smallest `[west, south, east, north]` box around the positions, or `None` when
/// there are none.
#[must_use]
pub fn extent<'a>(positions: impl IntoIterator<Item = &'a Position>) -> Option<[f64; 4]> {
    positions.into_iter().fold(None, |acc, p| {
        Some(match acc {
            None => [p[0], p[1], p[0], p[1]],
            Some([w, s, e, n]) => [w.min(p[0]), s.min(p[1]), e.max(p[0]), n.max(p[1])],
        })
    })
}

/// The ground size of an extent in metres: (east–west at its mid latitude, north–south).
#[must_use]
pub fn extent_size_m(extent: [f64; 4]) -> (f64, f64) {
    let [west, south, east, north] = extent;
    let mid_latitude = f64::midpoint(south, north).to_radians();
    ((east - west) * mid_latitude.cos() * METRES_PER_DEGREE, (north - south) * METRES_PER_DEGREE)
}

/// The mean of a closed ring's corners (the repeated closing position counted once), or
/// `None` for an empty ring.
#[must_use]
pub fn vertex_mean(ring: &[Position]) -> Option<Position> {
    let corners = match ring {
        [] => return None,
        [only] => return Some(*only),
        [rest @ .., last] if Some(last) == rest.first() => rest,
        all => all,
    };
    #[allow(clippy::cast_precision_loss)] // A ring never has 2^52 corners.
    let count = corners.len() as f64;
    let (lon, lat) = corners.iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
    Some([lon / count, lat / count])
}

/// Projects positions to (x, y) in degrees of latitude: longitude is scaled by the cosine
/// of the mean latitude, so both axes measure the same ground distance.
fn project(positions: &[Position]) -> Vec<(f64, f64)> {
    #[allow(clippy::cast_precision_loss)]
    let mean_latitude = positions.iter().map(|p| p[1]).sum::<f64>() / positions.len() as f64;
    let scale = mean_latitude.to_radians().cos();
    positions.iter().map(|p| (p[0] * scale, p[1])).collect()
}

/// Marks the positions Douglas–Peucker keeps between `first` and `last` (both already
/// kept). Uses an explicit stack, so outlines with many thousands of corners cannot
/// overflow the call stack.
fn mark_kept(points: &[(f64, f64)], first: usize, last: usize, tolerance: f64, keep: &mut [bool]) {
    let mut pending = vec![(first, last)];
    while let Some((start, end)) = pending.pop() {
        let farthest = (start + 1..end)
            .map(|i| (i, distance_to_segment(points[i], points[start], points[end])))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((index, offset)) = farthest
            && offset > tolerance
        {
            keep[index] = true;
            pending.push((start, index));
            pending.push((index, end));
        }
    }
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
        return distance(p, a);
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_squared).clamp(0.0, 1.0);
    distance(p, (a.0 + t * dx, a.1 + t * dy))
}

#[cfg(test)]
mod tests;
