//! Detail levels. Each perimeter is simplified once per level when it is fetched; a
//! snapshot then serves the level that suits its view, within explicit size limits.

use lavender_domain::{BBox, FeedItems, FirePerimeter, PerimeterDetail};
use lavender_geo::polygon::{self, Position};

use super::{largest_first, vertices};

/// One detail level: how far outlines are simplified and the widest view it serves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub tolerance_m: f64,
    /// The largest viewport span (the wider of its longitude and latitude extents, in
    /// degrees) this level serves. On a 1500-pixel-wide view the simplification error is
    /// about tolerance × 1500 / (span × 111 km): at the widest span each level serves,
    /// about half a pixel for levels 0–2 (level 0 counting WFIGS's own ~20 m
    /// generalisation) and about two pixels for level 3 at 15°, less on wider views.
    pub max_view_span_deg: f64,
}

/// Finest first. Level 0 is the outline as WFIGS publishes it (generalised to ~20 m).
pub const LEVELS: [Level; 4] = [
    Level { tolerance_m: 0.0, max_view_span_deg: 0.6 },
    Level { tolerance_m: 100.0, max_view_span_deg: 3.0 },
    Level { tolerance_m: 500.0, max_view_span_deg: 15.0 },
    Level { tolerance_m: 2_500.0, max_view_span_deg: f64::INFINITY },
];

/// The most JSON one snapshot's perimeters may take (its `items` value, envelope
/// included). A view whose level would exceed it, or [`MAX_RESPONSE_VERTICES`], gets the
/// next coarser level; at the coarsest, the largest fires that fit.
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// The most outline positions one snapshot may carry, for drawing cost.
pub const MAX_RESPONSE_VERTICES: usize = 150_000;

/// Every detail level of one full-detail perimeter, finest first.
#[must_use]
pub fn all_levels(mut full: FirePerimeter) -> Vec<FirePerimeter> {
    // Take the outline out first, so copying the facts for each level stays cheap.
    let outline = std::mem::take(&mut full.polygons);
    let coarser: Vec<FirePerimeter> = LEVELS
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, level)| FirePerimeter {
            detail: PerimeterDetail {
                level: u8::try_from(index).unwrap_or(u8::MAX),
                tolerance_m: level.tolerance_m,
            },
            polygons: simplify_polygons(&outline, level.tolerance_m),
            ..full.clone()
        })
        .collect();
    std::iter::once(FirePerimeter { polygons: outline, ..full }).chain(coarser).collect()
}

/// Simplified polygons. Polygons and holes smaller than the tolerance in both directions
/// are left out, except the fire's largest polygon, which always stays.
fn simplify_polygons(polygons: &[Vec<Vec<Position>>], tolerance_m: f64) -> Vec<Vec<Vec<Position>>> {
    let area =
        |rings: &Vec<Vec<Position>>| rings.first().map_or(0.0, |r| polygon::signed_area(r).abs());
    let largest = polygons
        .iter()
        .enumerate()
        .max_by(|a, b| area(a.1).total_cmp(&area(b.1)))
        .map_or(0, |(index, _)| index);
    let simplified: Vec<_> = polygons
        .iter()
        .enumerate()
        .filter_map(|(index, rings)| simplify_polygon(rings, tolerance_m, index == largest))
        .collect();
    if simplified.is_empty() {
        polygons.get(largest).cloned().into_iter().collect()
    } else {
        simplified
    }
}

fn simplify_polygon(
    rings: &[Vec<Position>],
    tolerance_m: f64,
    keep: bool,
) -> Option<Vec<Vec<Position>>> {
    let (exterior, holes) = rings.split_first()?;
    if !keep && smaller_than(exterior, tolerance_m) {
        return None;
    }
    let exterior =
        simplify(exterior, tolerance_m, true).or_else(|| keep.then(|| exterior.clone()))?;
    let holes = holes
        .iter()
        .filter(|hole| !smaller_than(hole, tolerance_m))
        .filter_map(|hole| simplify(hole, tolerance_m, false));
    Some(std::iter::once(exterior).chain(holes).collect())
}

fn simplify(ring: &[Position], tolerance_m: f64, exterior: bool) -> Option<Vec<Position>> {
    polygon::simplify_ring(ring, tolerance_m).and_then(|r| polygon::clean_ring(&r, exterior))
}

fn smaller_than(ring: &[Position], tolerance_m: f64) -> bool {
    polygon::extent(ring).is_some_and(|extent| {
        let (width, height) = polygon::extent_size_m(extent);
        width < tolerance_m && height < tolerance_m
    })
}

/// The perimeters to serve for a view: one detail level, chosen by the viewport's span
/// (no viewport means the whole world), the fires whose extent meets the viewport, largest
/// first, at most `max_items`. Returns the items, how many fires the source holds, and
/// whether fires were left out.
#[must_use]
pub fn select_for_view(
    perimeters: &[FirePerimeter],
    bbox: Option<&BBox>,
    max_items: usize,
) -> (Vec<FirePerimeter>, u32, bool) {
    let fires = perimeters.iter().filter(|p| p.detail.level == 0).count();
    let fires = u32::try_from(fires).unwrap_or(u32::MAX);
    let span = bbox.map_or(f64::INFINITY, |b| b.lon_span().max(b.lat_span()));
    let first = LEVELS.iter().position(|l| span <= l.max_view_span_deg).unwrap_or(LEVELS.len() - 1);
    for level in first..LEVELS.len() {
        let mut candidates: Vec<&FirePerimeter> = perimeters
            .iter()
            .filter(|p| usize::from(p.detail.level) == level)
            .filter(|p| bbox.is_none_or(|b| b.intersects(&p.extent)))
            .collect();
        candidates.sort_by(|a, b| largest_first(a, b));
        // The item cap is the same at every level; only the size limits warrant coarser
        // outlines.
        let wanted = candidates.len().min(max_items);
        let fitting = fitting_prefix(&candidates[..wanted]);
        if fitting == wanted || level == LEVELS.len() - 1 {
            let items = candidates[..fitting].iter().map(|p| (*p).clone()).collect();
            return (items, fires, fitting < candidates.len());
        }
    }
    (Vec::new(), fires, false)
}

/// How many of `candidates`, from the start, fit the byte and position limits. Bytes
/// are those the snapshot's `items` value takes: its envelope, each perimeter and the
/// commas between them.
fn fitting_prefix(candidates: &[&FirePerimeter]) -> usize {
    let Ok(envelope) = serde_json::to_vec(&FeedItems::Perimeters(Vec::new())) else { return 0 };
    let (mut bytes, mut positions) = (envelope.len(), 0_usize);
    for (count, perimeter) in candidates.iter().enumerate() {
        let Ok(json) = serde_json::to_vec(perimeter) else { return count };
        bytes = bytes.saturating_add(json.len() + usize::from(count > 0));
        positions = positions.saturating_add(vertices(perimeter));
        if bytes > MAX_RESPONSE_BYTES || positions > MAX_RESPONSE_VERTICES {
            return count;
        }
    }
    candidates.len()
}
