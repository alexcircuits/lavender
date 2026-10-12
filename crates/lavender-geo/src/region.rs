//! Region keys: a fixed degree grid that lets the feed hub share one upstream request per
//! cell among every subscriber whose viewport touches it.

use lavender_domain::{BBox, GeoPoint};

use crate::geodesy::{METRES_PER_NM, distance_m};

/// One cell of a regular latitude/longitude grid. The cell size is stored in
/// millidegrees so cells are hashable and comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    pub size_mdeg: u32,
    pub lat_idx: i32,
    pub lon_idx: i32,
}

impl Cell {
    #[must_use]
    pub fn size_deg(&self) -> f64 {
        f64::from(self.size_mdeg) / 1000.0
    }

    /// The cell's extent, clamped to valid coordinates.
    #[must_use]
    pub fn bbox(&self) -> BBox {
        let s = self.size_deg();
        let south = (-90.0 + f64::from(self.lat_idx) * s).clamp(-90.0, 90.0);
        let north = (south + s).clamp(-90.0, 90.0);
        let west = (-180.0 + f64::from(self.lon_idx) * s).clamp(-180.0, 180.0);
        let east = (west + s).clamp(-180.0, 180.0);
        BBox { west, south, east, north }
    }

    #[must_use]
    pub fn center(&self) -> GeoPoint {
        self.bbox().center()
    }

    /// Radius in nautical miles of the smallest circle around the centre that covers the
    /// whole cell (used for point-radius upstream APIs).
    #[must_use]
    pub fn covering_radius_nm(&self) -> f64 {
        let b = self.bbox();
        let c = self.center();
        [(b.south, b.west), (b.south, b.east), (b.north, b.west), (b.north, b.east)]
            .iter()
            .map(|&(lat, lon)| distance_m(&c, &GeoPoint { lat, lon, alt_m: None }))
            .fold(0.0, f64::max)
            / METRES_PER_NM
    }
}

/// The cells covering a viewport, or why they were limited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionPlan {
    pub cells: Vec<Cell>,
    /// `true` when the viewport needed more than `max_cells`; `cells` then holds the
    /// cells nearest the viewport centre, up to the limit.
    pub truncated: bool,
}

/// Plans the grid cells of `size_deg` degrees that cover `bbox`, keeping at most
/// `max_cells` (nearest to the centre first).
///
/// The work is bounded by `max_cells`, not by the size of the box: when the box needs
/// more cells than allowed, only a window of cells around the centre is built and ranked
/// (a world-sized box at 0.1° would otherwise mean millions of cells per viewport update).
#[must_use]
pub fn plan_cells(bbox: &BBox, size_deg: f64, max_cells: usize) -> RegionPlan {
    let size = size_deg.clamp(0.1, 90.0);
    // Rounding to millidegrees is the point: cells must be hashable.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let millideg = (size * 1000.0).round() as u32;
    let cell = f64::from(millideg) / 1000.0;
    let lat_cells = (180.0 / cell).ceil();
    let lon_cells = (360.0 / cell).ceil();
    let index = |v: f64, origin: f64, count: f64| -> i64 {
        // Indices are bounded by `count` (at most 3 600 for 0.1° cells).
        #[allow(clippy::cast_possible_truncation)]
        let i = ((v - origin) / cell).floor().clamp(0.0, count - 1.0) as i64;
        i
    };
    // An upper edge lying exactly on a cell boundary does not reach into the next cell.
    let upper = |v: f64, origin: f64, count: f64, lower: i64| -> i64 {
        #[allow(clippy::cast_possible_truncation)]
        let i = (((v - origin) / cell).ceil() - 1.0).clamp(0.0, count - 1.0) as i64;
        i.max(lower)
    };
    #[allow(clippy::cast_possible_truncation)]
    let lon_count = lon_cells as i64;
    let lat0 = index(bbox.south, -90.0, lat_cells);
    let lat1 = upper(bbox.north, -90.0, lat_cells, lat0);
    // Longitude indices as one contiguous run: across the antimeridian the eastern part is
    // shifted by a full turn, and indices are wrapped back when cells are built.
    let lon_start = index(bbox.lon_ranges()[0].0, -180.0, lon_cells);
    let lon_end = if bbox.crosses_antimeridian() {
        lon_count + upper(bbox.east, -180.0, lon_cells, 0)
    } else {
        upper(bbox.east, -180.0, lon_cells, lon_start)
    };
    let span_lat = lat1 - lat0 + 1;
    let span_lon = (lon_end - lon_start + 1).min(lon_count);
    let make = |lat_idx: i64, lon: i64| Cell {
        size_mdeg: millideg,
        // Both indices are below 3 600, so they fit in i32.
        lat_idx: i32::try_from(lat_idx).unwrap_or(i32::MAX),
        lon_idx: i32::try_from(lon.rem_euclid(lon_count)).unwrap_or(i32::MAX),
    };
    let window = |lats: std::ops::Range<i64>, lons: std::ops::Range<i64>| -> Vec<Cell> {
        let mut cells: Vec<Cell> =
            lats.flat_map(|lat| lons.clone().map(move |lon| make(lat, lon))).collect();
        cells.sort_unstable();
        cells.dedup();
        cells
    };

    let want = i64::try_from(max_cells).unwrap_or(i64::MAX);
    if span_lat.saturating_mul(span_lon) <= want {
        let cells = window(lat0..lat1 + 1, lon_start..lon_start + span_lon);
        return RegionPlan { cells, truncated: false };
    }

    // Too many: rank only a window around the centre cell that holds at least `want`
    // cells (roughly square, stretched along an axis where the box is narrow).
    let centre = bbox.center();
    let clat = index(centre.lat, -90.0, lat_cells);
    let mut clon = index(centre.lon, -180.0, lon_cells);
    if clon < lon_start {
        clon += lon_count;
    }
    let square = 2 * (want.isqrt() + 1) + 1;
    let lon_w0 = span_lon.min(square);
    let ceil_div = |a: i64, b: i64| a / b + i64::from(a % b != 0);
    let lat_w = span_lat.min(square.max(ceil_div(want, lon_w0)));
    let lon_w = span_lon.min(lon_w0.max(ceil_div(want, lat_w)));
    let lat_s = (clat - lat_w / 2).clamp(lat0, lat1 - lat_w + 1);
    let lon_s = (clon - lon_w / 2).clamp(lon_start, lon_start + span_lon - lon_w);
    let mut cells = window(lat_s..lat_s + lat_w, lon_s..lon_s + lon_w);
    cells.sort_by(|a, b| {
        distance_m(&a.center(), &centre).total_cmp(&distance_m(&b.center(), &centre))
    });
    cells.truncate(max_cells);
    cells.sort_unstable();
    RegionPlan { cells, truncated: true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn small_box_is_one_cell() {
        let b = BBox::new(24.0, 60.0, 25.0, 60.5).unwrap();
        let plan = plan_cells(&b, 5.0, 9);
        assert_eq!(plan.cells.len(), 1);
        assert!(!plan.truncated);
        assert!(plan.cells[0].bbox().contains(&b.center()));
    }

    #[test]
    fn antimeridian_box_takes_cells_on_both_sides() {
        let b = BBox::new(178.0, -2.0, -178.0, 2.0).unwrap();
        let plan = plan_cells(&b, 5.0, 9);
        let lons: Vec<f64> = plan.cells.iter().map(|c| c.bbox().west).collect();
        assert!(lons.contains(&175.0) && lons.contains(&-180.0), "{lons:?}");
    }

    #[test]
    fn large_box_is_truncated_to_centre() {
        let plan = plan_cells(&BBox::WORLD, 5.0, 9);
        assert!(plan.truncated);
        assert_eq!(plan.cells.len(), 9);
        assert!(plan.cells.iter().any(|c| c.bbox().contains(&BBox::WORLD.center())));
    }

    #[test]
    fn planning_work_is_bounded_by_the_cell_limit() {
        // Review LOW: every cell of the box was built and sorted before truncating
        // (6.48 million cells for the world at 0.1°).
        let started = std::time::Instant::now();
        for _ in 0..200 {
            let plan = plan_cells(&BBox::WORLD, 0.1, 25);
            assert!(plan.truncated);
            assert_eq!(plan.cells.len(), 25);
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
        let plan = plan_cells(&BBox::WORLD, 1.0, 9);
        assert!(plan.cells.iter().any(|c| c.bbox().contains(&BBox::WORLD.center())));
    }

    #[test]
    fn narrow_boxes_and_antimeridian_boxes_still_fill_the_limit() {
        let strip = BBox::new(-180.0, 10.0, 180.0, 10.5).unwrap();
        let plan = plan_cells(&strip, 1.0, 25);
        assert!(plan.truncated);
        assert_eq!(plan.cells.len(), 25, "one row tall: the window stretches along it");
        let pacific = BBox::new(100.0, -60.0, -60.0, 60.0).unwrap();
        let plan = plan_cells(&pacific, 1.0, 16);
        assert_eq!(plan.cells.len(), 16);
        let centre = pacific.center();
        assert!(plan.cells.iter().any(|c| c.bbox().contains(&centre)), "{centre:?}");
        for c in &plan.cells {
            assert!(c.bbox().intersects(&pacific));
        }
    }

    #[test]
    fn five_degree_cells_fit_250_nm_radius() {
        for lat_idx in 0..36 {
            let c = Cell { size_mdeg: 5000, lat_idx, lon_idx: 10 };
            assert!(c.covering_radius_nm() <= 250.0, "{lat_idx}: {}", c.covering_radius_nm());
        }
    }

    proptest! {
        #[test]
        fn truncated_plans_hold_exactly_the_limit_inside_the_box(w in -180.0f64..180.0, e in -180.0f64..180.0, s in -89.0f64..0.0, n in 0.0f64..89.0, max in 1usize..30) {
            let b = BBox::new(w, s, e, n).unwrap();
            let plan = plan_cells(&b, 1.0, max);
            prop_assert!(plan.cells.len() <= max);
            if plan.truncated {
                prop_assert_eq!(plan.cells.len(), max);
            }
            for c in &plan.cells {
                prop_assert!(c.bbox().intersects(&b));
            }
        }

        #[test]
        fn every_cell_intersects_the_box(w in -180.0f64..180.0, e in -180.0f64..180.0, s in -89.0f64..0.0, n in 0.0f64..89.0) {
            let b = BBox::new(w, s, e, n).unwrap();
            let plan = plan_cells(&b, 5.0, 2_000);
            prop_assert!(!plan.cells.is_empty());
            for c in &plan.cells {
                prop_assert!(c.bbox().intersects(&b));
            }
        }
    }
}
