//! Map aggregation of fire detections: exact-duplicate removal and adaptive grid cells.
//!
//! No valid detection is ever lost. A small view receives individual detections when
//! they fit; otherwise detections are grouped into grid cells whose `count`, `frp`
//! (total), `max_frp` (peak) and `latest_acquired_at` (newest) summarise every detection
//! inside. Cells double in size until the JSON fits the snapshot budget, so the
//! worldwide snapshot stays under 1 MB however dense the input is.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use jiff::Timestamp;
use lavender_domain::{BBox, FireCell, GeoPoint};

/// JSON budget for the cell list. The remaining 50 kB of the 1 MB snapshot covers the
/// envelope: timestamps, source metadata, freshness and the capped notices.
pub(super) const MAX_CELL_JSON_BYTES: usize = 950_000;
/// Views no wider or taller than this (degrees) get individual detections when they fit.
const INDIVIDUAL_SPAN_DEG: f64 = 5.0;

/// Removes exact duplicates, keeping the first of each. Overlapping regional answers and
/// repeated upstream rows describe the same detections; records that differ in any field
/// stay distinct.
#[must_use]
pub fn distinct_cells<'a>(cells: impl IntoIterator<Item = &'a FireCell>) -> Vec<&'a FireCell> {
    let mut seen = HashSet::new();
    cells.into_iter().filter(|cell| seen.insert(Exact(cell))).collect()
}

/// A fire record compared and hashed by every field (numbers by bit pattern), so
/// duplicate removal needs no allocation per record.
struct Exact<'a>(&'a FireCell);

type Fields<'a> = (u64, u64, u64, u64, u64, u32, Timestamp, Timestamp, &'a str, &'a str, &'a str);

impl<'a> Exact<'a> {
    fn fields(&self) -> Fields<'a> {
        // Destructured so that a new field cannot be forgotten here.
        let FireCell {
            lat,
            lon,
            brightness,
            frp,
            confidence,
            satellite,
            acquired_at,
            day_night,
            count,
            max_frp,
            latest_acquired_at,
        } = self.0;
        (
            lat.to_bits(),
            lon.to_bits(),
            brightness.to_bits(),
            frp.to_bits(),
            max_frp.to_bits(),
            *count,
            *acquired_at,
            *latest_acquired_at,
            confidence,
            satellite,
            day_night,
        )
    }
}

impl PartialEq for Exact<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.fields() == other.fields()
    }
}

impl Eq for Exact<'_> {}

impl Hash for Exact<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fields().hash(state);
    }
}

/// Selects the detections a view asks for and groups them for the map.
///
/// The sum of the returned counts always equals the sum of the eligible input counts.
/// Eligible means: a positive count, a valid position inside `bbox`, finite FRP values,
/// and a peak FRP (`max_frp`) of at least `min_frp`. The boolean reports that something
/// was left out, which happens only when `max_items` is zero.
#[must_use]
pub fn aggregate_fires<'a>(
    cells: impl IntoIterator<Item = &'a FireCell>,
    bbox: Option<&BBox>,
    min_frp: Option<f64>,
    max_items: usize,
) -> (Vec<FireCell>, bool) {
    let eligible: Vec<&FireCell> =
        cells.into_iter().filter(|cell| is_eligible(cell, bbox, min_frp)).collect();
    if max_items == 0 {
        return (Vec::new(), !eligible.is_empty());
    }
    let span = bbox.map(|b| b.lon_span().max(b.lat_span()));
    if span.is_some_and(|s| s <= INDIVIDUAL_SPAN_DEG) && fits_payload(&eligible, max_items) {
        return (eligible.into_iter().cloned().collect(), false);
    }
    // Anchor longitude at a dateline-crossing view's western edge, so neighbours at
    // +179.9 and -179.9 are grouped locally, not across the globe.
    let origin_lon = bbox.filter(|b| b.crosses_antimeridian()).map_or(-180.0, |b| b.west);
    let mut cell_deg = starting_cell_deg(span);
    loop {
        let mut grouped = group(&eligible, cell_deg, origin_lon);
        // A single cell holds everything and cannot be coarsened any further. (It can
        // only exceed the byte budget with far longer text than the CSV parser keeps.)
        if grouped.len() <= 1 || fits_payload(&grouped, max_items) {
            grouped.sort_by(|a, b| {
                b.max_frp
                    .total_cmp(&a.max_frp)
                    .then_with(|| a.lat.total_cmp(&b.lat))
                    .then_with(|| a.lon.total_cmp(&b.lon))
            });
            return (grouped, false);
        }
        cell_deg *= 2.0;
    }
}

fn is_eligible(cell: &FireCell, bbox: Option<&BBox>, min_frp: Option<f64>) -> bool {
    cell.count > 0
        && cell.frp.is_finite()
        && cell.max_frp.is_finite()
        // A cell's strongest detection decides; a total over many weak ones does not.
        && min_frp.is_none_or(|min| cell.max_frp >= min)
        && GeoPoint::new(cell.lat, cell.lon).is_ok_and(|p| bbox.is_none_or(|b| b.contains(&p)))
}

/// The first grid tried for a view: finer for smaller views.
fn starting_cell_deg(span: Option<f64>) -> f64 {
    match span {
        Some(s) if s <= INDIVIDUAL_SPAN_DEG => 0.1,
        Some(s) if s <= 10.0 => 0.25,
        Some(s) if s <= 20.0 => 0.5,
        _ => 1.0,
    }
}

/// Groups detections into `cell_deg` grid cells whose columns start at `origin_lon`.
fn group(cells: &[&FireCell], cell_deg: f64, origin_lon: f64) -> Vec<FireCell> {
    let mut grid: HashMap<(i32, i32), FireGroup> = HashMap::new();
    for cell in cells {
        // Eligible positions are finite and within ±90/±180, so the indices are small.
        #[allow(clippy::cast_possible_truncation)]
        let key = (
            ((cell.lat + 90.0) / cell_deg).floor() as i32,
            ((cell.lon - origin_lon).rem_euclid(360.0) / cell_deg).floor() as i32,
        );
        grid.entry(key).or_insert_with(|| FireGroup::new(cell)).add(cell);
    }
    grid.into_values().map(FireGroup::finish).collect()
}

/// Whether `cells` fit both the item cap and the JSON byte budget.
fn fits_payload<T: serde::Serialize>(cells: &[T], max_items: usize) -> bool {
    if cells.len() > max_items {
        return false;
    }
    let mut bytes = 2_usize;
    for cell in cells {
        let Ok(json) = serde_json::to_vec(cell) else { return false };
        bytes = bytes.saturating_add(json.len() + 1);
        if bytes > MAX_CELL_JSON_BYTES {
            return false;
        }
    }
    true
}

/// Running totals for one grid cell. The strongest member supplies the descriptive
/// fields (satellite, confidence, day/night, acquisition time).
struct FireGroup {
    dominant: FireCell,
    count: u32,
    /// The unsaturated count, as the centroid weight.
    weight: f64,
    sum_lat: f64,
    sin_lon: f64,
    cos_lon: f64,
    frp: f64,
    brightness: f64,
    latest: Timestamp,
}

impl FireGroup {
    fn new(first: &FireCell) -> Self {
        Self {
            dominant: first.clone(),
            count: 0,
            weight: 0.0,
            sum_lat: 0.0,
            sin_lon: 0.0,
            cos_lon: 0.0,
            frp: 0.0,
            brightness: 0.0,
            latest: first.latest_acquired_at,
        }
    }

    fn add(&mut self, cell: &FireCell) {
        // The domain count is a u32; saturating keeps an absurd input from panicking.
        self.count = self.count.saturating_add(cell.count);
        let weight = f64::from(cell.count);
        self.weight += weight;
        self.sum_lat += cell.lat * weight;
        self.sin_lon += cell.lon.to_radians().sin() * weight;
        self.cos_lon += cell.lon.to_radians().cos() * weight;
        // Totals stay finite even for extreme but valid CSV numbers.
        self.frp = (self.frp + cell.frp).min(f64::MAX);
        self.brightness = self.brightness.max(cell.brightness);
        self.latest = self.latest.max(cell.latest_acquired_at);
        if cell.max_frp > self.dominant.max_frp {
            self.dominant = cell.clone();
        }
    }

    fn finish(mut self) -> FireCell {
        // Weighted centroid; longitude as a circular mean so dateline cells stay local.
        self.dominant.lat = (self.sum_lat / self.weight).clamp(-90.0, 90.0);
        if self.sin_lon.hypot(self.cos_lon) > 1e-9 {
            self.dominant.lon = self.sin_lon.atan2(self.cos_lon).to_degrees();
        }
        self.dominant.frp = self.frp;
        self.dominant.brightness = self.brightness;
        self.dominant.count = self.count;
        self.dominant.latest_acquired_at = self.latest;
        self.dominant
    }
}

#[cfg(test)]
mod tests {
    use lavender_domain::FeedItems;

    use super::*;
    use crate::providers::firms::parse_csv;

    const SAMPLE_CSV: &str =
        include_str!("../../../../../legacy/src/data/fixtures/firms-viirs-noaa20-sample.csv");

    #[test]
    fn spatial_aggregation_preserves_total_detection_count() {
        let (detections, _) = parse_csv(SAMPLE_CSV.as_bytes()).unwrap();
        let cells: Vec<FireCell> = detections.into_iter().map(Into::into).collect();
        let original_count = cells.len();

        let (aggregated, _) = aggregate_fires(&cells, None, None, 20_000);
        let preserved_count: u32 = aggregated.iter().map(|c| c.count).sum();
        assert_eq!(preserved_count as usize, original_count);
        assert!(aggregated.len() < original_count);
    }

    #[test]
    fn small_viewport_sends_individual_detections() {
        let (detections, _) = parse_csv(SAMPLE_CSV.as_bytes()).unwrap();
        let cells: Vec<FireCell> = detections.into_iter().map(Into::into).collect();

        // Small 2° box around Southern California detections in the sample
        let bbox = BBox::new(-118.0, 33.0, -116.0, 35.0).unwrap();
        let (result, _) = aggregate_fires(&cells, Some(&bbox), None, 20_000);
        assert_ne!(result.len(), 0);
        // For individual detections, every cell count is 1
        assert!(result.iter().all(|c| c.count == 1));
    }

    #[test]
    fn world_zoom_payload_is_under_one_megabyte() {
        // Synthesise 5,000 detections worldwide
        let mut cells = Vec::new();
        let ts: Timestamp = "2026-07-16T12:00:00Z".parse().unwrap();
        for i in 0..5_000 {
            cells.push(FireCell {
                lat: -60.0 + (f64::from(i) * 0.05) % 120.0,
                lon: -180.0 + (f64::from(i) * 0.1) % 360.0,
                brightness: 310.0,
                frp: 5.0,
                confidence: "n".into(),
                satellite: "N20".into(),
                acquired_at: ts,
                day_night: "N".into(),
                count: 1,
                max_frp: 5.0,
                latest_acquired_at: ts,
            });
        }
        let (aggregated, _) = aggregate_fires(&cells, None, None, 20_000);
        let json = serde_json::to_string(&aggregated).unwrap();
        assert!(json.len() < 1_000_000, "payload was {} bytes", json.len());
        let total_count: u32 = aggregated.iter().map(|c| c.count).sum();
        assert_eq!(total_count, 5_000);
    }
    fn cell(lat: f64, lon: f64) -> FireCell {
        FireCell {
            lat,
            lon,
            brightness: 320.0,
            frp: 5.0,
            confidence: "n".into(),
            satellite: "N20".into(),
            acquired_at: Timestamp::now(),
            day_night: "N".into(),
            count: 1,
            max_frp: 5.0,
            latest_acquired_at: Timestamp::now(),
        }
    }

    #[test]
    fn dense_world_adapts_without_losing_counts_and_leaves_envelope_room() {
        // Synthetic: one cell per degree plus a second detection in every cell.
        // Long escaped strings and extreme finite numbers exercise serialized bytes,
        // rather than the deceptively small 5,000-point sample above.
        let mut cells = Vec::new();
        for lat in -90..90 {
            for lon in -180..180 {
                let mut c = cell(f64::from(lat) + 0.123_456_789, f64::from(lon) + 0.123_456_789);
                c.confidence = "\\".repeat(16);
                c.satellite = "𐀀".repeat(16);
                c.day_night = "\\".repeat(8);
                c.frp = f64::MAX;
                c.max_frp = f64::MAX;
                c.brightness = f64::MAX;
                cells.push(c.clone());
                c.count = 2;
                cells.push(c);
            }
        }
        let (aggregated, truncated) = aggregate_fires(&cells, None, None, 20_000);
        assert!(!truncated);
        assert_eq!(aggregated.iter().map(|c| u64::from(c.count)).sum::<u64>(), 194_400);
        let bytes = serde_json::to_vec(&FeedItems::Fires(aggregated.clone())).unwrap().len();
        assert!(bytes + 50_000 < 1_000_000, "{bytes} cell bytes");
        assert!(aggregated.len() <= 20_000);
        assert!(aggregated.iter().all(|c| c.frp.is_finite()));
        // A lower operator cap coarsens further; it must never truncate counts.
        let (coarse, truncated) = aggregate_fires(&aggregated, None, None, 3);
        assert!(!truncated);
        assert!(coarse.len() <= 3);
        assert_eq!(coarse.iter().map(|c| c.count).sum::<u32>(), 194_400);
    }

    #[test]
    fn small_viewport_also_groups_when_individual_json_exceeds_budget() {
        let mut cells = Vec::new();
        for i in 0..10_000 {
            let mut c = cell(f64::from(i % 100) * 0.01, f64::from(i / 100) * 0.01);
            // Four-byte scalars are the largest UTF-8 representation per permitted
            // character; metadata is capped at 16/16/8 characters by the parser.
            c.confidence = "𐀀".repeat(16);
            c.satellite = "𐀀".repeat(16);
            c.day_night = "𐀀".repeat(8);
            cells.push(c);
        }
        assert!(serde_json::to_vec(&cells).unwrap().len() > MAX_CELL_JSON_BYTES);
        let bbox = BBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let (grouped, truncated) = aggregate_fires(&cells, Some(&bbox), None, 20_000);
        assert!(!truncated);
        assert!(grouped.len() < cells.len());
        assert_eq!(grouped.iter().map(|c| c.count).sum::<u32>(), 10_000);
        assert!(serde_json::to_vec(&grouped).unwrap().len() < MAX_CELL_JSON_BYTES);
    }

    #[test]
    fn dateline_coalescing_preserves_weighted_counts_and_local_centroid() {
        // Neighbours 0.2° apart across the dateline, and a point 0.8° further west.
        let mut west = cell(10.0, 179.9);
        west.count = 7;
        let mut east = cell(10.0, -179.9);
        east.count = 3;
        let farther = cell(10.0, 179.1);
        let excluded = cell(10.0, 0.0);
        let bbox = BBox::new(179.0, 9.0, -179.0, 11.0).unwrap();
        let input = [west, east, farther, excluded];
        let (cells, truncated) = aggregate_fires(&input, Some(&bbox), None, 2);
        assert!(!truncated);
        // Columns start at the view's western edge, so the neighbours share a cell; a
        // grid anchored at -180° would split them and merge 179.9° with 179.1° instead.
        let counts: Vec<u32> = cells.iter().map(|c| c.count).collect();
        assert_eq!(cells.len(), 2, "{counts:?}");
        let merged = cells.iter().find(|c| c.count == 10).expect("neighbours merged");
        assert!(merged.lon.abs() > 179.8, "centroid {}", merged.lon);
        assert!(bbox.contains(&GeoPoint::new(merged.lat, merged.lon).unwrap()));
        assert!(cells.iter().any(|c| c.count == 1 && (c.lon - 179.1).abs() < 1e-9));
    }

    #[test]
    fn min_frp_selects_by_peak_detection_not_cell_total() {
        // Ten weak detections (1.2 MW each, 12 MW in total) must not pass a 10 MW filter.
        let mut weak = cell(10.0, 10.0);
        weak.count = 10;
        weak.frp = 12.0;
        weak.max_frp = 1.2;
        let strong = cell(20.0, 20.0);
        let (cells, _) = aggregate_fires(&[weak, strong], None, Some(5.0), 100);
        assert_eq!(cells.iter().map(|c| c.count).sum::<u32>(), 1);
    }

    #[test]
    fn merged_counts_saturate_instead_of_overflowing() {
        let mut big = cell(10.0, 10.0);
        big.count = u32::MAX - 1;
        let mut more = cell(10.1, 10.1);
        more.count = 5;
        let (cells, _) = aggregate_fires(&[big, more], None, None, 100);
        assert_eq!(cells.iter().map(|c| u64::from(c.count)).sum::<u64>(), u64::from(u32::MAX));
    }

    #[test]
    fn coarsening_stops_at_one_cell_even_when_it_exceeds_the_byte_budget() {
        let mut huge = cell(10.0, 10.0);
        huge.satellite = "x".repeat(MAX_CELL_JSON_BYTES);
        huge.max_frp = 50.0;
        let other = cell(-40.0, -100.0);
        let (cells, truncated) = aggregate_fires(&[huge, other], None, None, 1);
        assert!(!truncated);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].count, 2);
    }

    #[test]
    fn distinct_cells_drop_only_exact_duplicates() {
        let base = cell(10.0, 20.0);
        let brighter = FireCell { brightness: 321.0, ..base.clone() };
        let other_satellite = FireCell { satellite: "N21".into(), ..base.clone() };
        let input = [base.clone(), base.clone(), brighter, other_satellite, base];
        let distinct = distinct_cells(&input);
        assert_eq!(distinct.len(), 3);
        assert!(std::ptr::eq(distinct[0], &raw const input[0]), "the first copy is kept");
    }

    /// Every 1° world cell occupied, clustered hot spots, points on the poles, the
    /// antimeridian and the corners, extreme FRP, plus records the aggregator must skip.
    fn dense_adversarial_world() -> Vec<FireCell> {
        let mut cells = Vec::new();
        for lat in -90..90 {
            for lon in -180..180 {
                cells.push(cell(f64::from(lat) + 0.5, f64::from(lon) + 0.5));
            }
        }
        for (hot_lat, hot_lon) in [(-15.0, 130.0), (60.0, 100.0), (0.0, 20.0), (40.0, -120.0)] {
            for i in 0..5_000_u32 {
                let step = f64::from(i);
                cells.push(cell(
                    hot_lat + (step % 70.0) * 0.003,
                    hot_lon + (step / 70.0).floor() * 0.003,
                ));
            }
        }
        for (lat, lon) in [
            (90.0, 180.0),
            (90.0, -180.0),
            (-90.0, 180.0),
            (-90.0, -180.0),
            (0.0, 180.0),
            (0.0, -180.0),
            (10.0, 179.999_99),
            (10.0, -179.999_99),
        ] {
            let mut extreme = cell(lat, lon);
            extreme.frp = f64::MAX;
            extreme.max_frp = f64::MAX;
            extreme.brightness = f64::MAX;
            cells.push(extreme);
        }
        let skipped = [
            FireCell { lat: f64::NAN, ..cell(0.0, 0.0) },
            FireCell { lon: f64::INFINITY, ..cell(0.0, 0.0) },
            FireCell { lat: 90.000_01, ..cell(0.0, 0.0) },
            FireCell { lon: -180.5, ..cell(0.0, 0.0) },
            FireCell { frp: f64::NAN, ..cell(0.0, 0.0) },
            FireCell { max_frp: f64::NEG_INFINITY, ..cell(0.0, 0.0) },
            FireCell { count: 0, ..cell(0.0, 0.0) },
        ];
        cells.extend(skipped);
        cells
    }

    #[test]
    fn every_view_bucket_preserves_counts_on_dense_adversarial_cells() {
        let cells = dense_adversarial_world();
        assert!(cells.len() >= 84_800);
        let views = [
            None,
            Some(BBox::WORLD),
            Some(BBox::new(-30.0, -20.0, 30.0, 20.0).unwrap()),
            Some(BBox::new(-10.0, -10.0, 10.0, 10.0).unwrap()),
            Some(BBox::new(125.0, -20.0, 135.0, -10.0).unwrap()),
            Some(BBox::new(128.0, -16.0, 133.0, -11.0).unwrap()),
            Some(BBox::new(129.5, -15.5, 131.5, -13.5).unwrap()),
            Some(BBox::new(130.0, -15.0, 130.1, -14.9).unwrap()),
            Some(BBox::new(170.0, -10.0, -170.0, 10.0).unwrap()),
            Some(BBox::new(179.0, 9.0, -179.0, 11.0).unwrap()),
            Some(BBox::new(-180.0, 85.0, 180.0, 90.0).unwrap()),
            Some(BBox::new(-180.0, -90.0, -175.0, -85.0).unwrap()),
        ];
        for view in &views {
            for min_frp in [None, Some(10.0)] {
                let expected: u64 = cells
                    .iter()
                    .filter(|c| c.count > 0 && c.frp.is_finite() && c.max_frp.is_finite())
                    .filter(|c| min_frp.is_none_or(|min| c.max_frp >= min))
                    .filter(|c| {
                        GeoPoint::new(c.lat, c.lon)
                            .is_ok_and(|p| view.is_none_or(|b| b.contains(&p)))
                    })
                    .map(|c| u64::from(c.count))
                    .sum();
                let (out, truncated) = aggregate_fires(&cells, view.as_ref(), min_frp, 20_000);
                let label = format!("{view:?} min_frp {min_frp:?}");
                assert!(!truncated, "{label}");
                assert_eq!(
                    out.iter().map(|c| u64::from(c.count)).sum::<u64>(),
                    expected,
                    "{label}"
                );
                assert!(out.len() <= 20_000, "{label}");
                assert!(serde_json::to_vec(&out).unwrap().len() <= MAX_CELL_JSON_BYTES, "{label}");
                assert!(out.iter().all(|c| c.frp.is_finite() && c.max_frp.is_finite()), "{label}");
                assert!(out.iter().all(|c| GeoPoint::new(c.lat, c.lon).is_ok()), "{label}");
            }
        }
    }

    #[test]
    fn cells_keep_peak_frp_and_newest_time() {
        let mut old = cell(10.0, 10.0);
        old.latest_acquired_at = "2026-07-16T10:00:00Z".parse().unwrap();
        old.max_frp = 40.0;
        old.frp = 40.0;
        let mut new = cell(10.2, 10.2);
        new.latest_acquired_at = "2026-07-16T11:00:00Z".parse().unwrap();
        new.max_frp = 3.0;
        new.frp = 3.0;
        let (cells, _) = aggregate_fires(&[old.clone(), new.clone()], None, None, 100);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].count, 2);
        assert!((cells[0].frp - 43.0).abs() < 1e-9);
        assert!((cells[0].max_frp - 40.0).abs() < 1e-9);
        assert_eq!(cells[0].latest_acquired_at, new.latest_acquired_at);
        assert_eq!(cells[0].acquired_at, old.acquired_at, "descriptive fields of the peak");
    }
}
