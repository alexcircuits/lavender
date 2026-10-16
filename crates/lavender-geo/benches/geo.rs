//! Hot-path benchmarks for `lavender-geo` (ENG-1000, `docs/performance/specification.md`
//! §2 backend targets): map-area (bbox) filtering of 50 000 points, region-key planning and
//! batch SGP4 propagation of 5 000 satellites.
//!
//! All inputs are deterministic synthetic data generated here; nothing is read from the
//! network. Run with `cargo bench -p lavender-geo` (or `scripts/perf.sh bench`).

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jiff::{SignedDuration, Timestamp};
use lavender_domain::{BBox, GeoPoint, SatelliteElements};
use lavender_geo::{PointIndex, Propagator, plan_cells};

/// Points in the spec's bbox target ("Bbox filter of 50 000 points ≤ 2 ms").
const POINTS: usize = 50_000;
/// Satellites in the ENG-976 scenario ("5 000 visible satellites").
const SATELLITES: usize = 5_000;
/// 2026-10-10T12:00:00Z, the epoch of every synthetic element set.
const EPOCH: Timestamp = Timestamp::constant(1_791_633_600, 0);

/// A small deterministic generator (xorshift64*), so every run measures the same data.
struct Sequence(u64);

impl Sequence {
    fn next_unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11;
        // 53 random bits are exactly representable in an f64 mantissa.
        #[allow(clippy::cast_precision_loss)]
        let unit = bits as f64 / (1_u64 << 53) as f64;
        unit
    }

    fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next_unit()
    }
}

/// Points spread uniformly in latitude and longitude over the whole world.
fn world_points(count: usize) -> Vec<GeoPoint> {
    let mut seq = Sequence(0x5eed_0001);
    (0..count)
        .map(|_| GeoPoint {
            lat: seq.range(-85.0, 85.0),
            lon: seq.range(-180.0, 180.0),
            alt_m: None,
        })
        .collect()
}

/// Viewports a client sends: a city, a continent, and one crossing the antimeridian.
fn viewports() -> [(&'static str, BBox); 3] {
    [
        ("city", BBox { west: 24.0, south: 59.9, east: 25.5, north: 60.5 }),
        ("europe", BBox { west: -12.0, south: 35.0, east: 32.0, north: 62.0 }),
        ("pacific_antimeridian", BBox { west: 150.0, south: -40.0, east: -150.0, north: 20.0 }),
    ]
}

fn bbox_filter(c: &mut Criterion) {
    let points = world_points(POINTS);
    let index = PointIndex::build(&points);
    let mut group = c.benchmark_group("bbox_filter_50k");
    group.throughput(Throughput::Elements(POINTS as u64));
    for (name, bbox) in viewports() {
        // The hub's current strategy: one `BBox::contains` per item.
        group.bench_with_input(BenchmarkId::new("linear_scan", name), &bbox, |b, bbox| {
            b.iter(|| points.iter().filter(|p| bbox.contains(p)).count());
        });
        group.bench_with_input(BenchmarkId::new("rtree_query", name), &bbox, |b, bbox| {
            b.iter(|| index.query(bbox).len());
        });
    }
    // The R-tree only pays off if it is built once per refresh, not per query.
    group.bench_function("rtree_build", |b| b.iter(|| PointIndex::build(black_box(&points))));
    group.finish();
}

fn region_keys(c: &mut Criterion) {
    let mut group = c.benchmark_group("region_keys");
    // (name, viewport, cell size in degrees, cell limit): adsb.lol's defaults are 5° and 4.
    let cases = [
        ("city_5deg_max4", BBox { west: 24.0, south: 59.9, east: 25.5, north: 60.5 }, 5.0, 4),
        ("europe_5deg_max25", BBox { west: -12.0, south: 35.0, east: 32.0, north: 62.0 }, 5.0, 25),
        ("world_1deg_max25", BBox::WORLD, 1.0, 25),
        ("world_0.1deg_max25", BBox::WORLD, 0.1, 25),
        (
            "pacific_antimeridian_1deg_max16",
            BBox { west: 100.0, south: -60.0, east: -60.0, north: 60.0 },
            1.0,
            16,
        ),
    ];
    for (name, bbox, size, max_cells) in cases {
        group.bench_function(name, |b| {
            b.iter(|| plan_cells(black_box(&bbox), black_box(size), black_box(max_cells)));
        });
    }
    group.finish();
}

/// A synthetic catalogue: mostly low Earth orbits with a share of medium and
/// geosynchronous ones, all with the same epoch.
fn satellite_elements(count: usize) -> Vec<SatelliteElements> {
    let mut seq = Sequence(0x5eed_0002);
    (0..count)
        .map(|i| {
            let class = seq.next_unit();
            let mean_motion = if class < 0.85 {
                seq.range(13.0, 16.0)
            } else if class < 0.95 {
                seq.range(1.9, 2.1)
            } else {
                seq.range(0.99, 1.01)
            };
            SatelliteElements {
                norad_id: 90_000 + u32::try_from(i).unwrap_or(0),
                name: format!("SYNTHETIC {i:05}"),
                intl_designator: None,
                group: "synthetic".to_owned(),
                epoch: EPOCH,
                mean_motion,
                eccentricity: seq.range(0.0001, 0.02),
                inclination_deg: seq.range(0.0, 99.0),
                raan_deg: seq.range(0.0, 360.0),
                arg_of_pericenter_deg: seq.range(0.0, 360.0),
                mean_anomaly_deg: seq.range(0.0, 360.0),
                bstar: seq.range(0.0, 0.0005),
                mean_motion_dot: 0.0,
                mean_motion_ddot: 0.0,
                rev_at_epoch: None,
                classification: None,
            }
        })
        .collect()
}

fn sgp4_batch(c: &mut Criterion) {
    let elements = satellite_elements(SATELLITES);
    let propagators: Vec<Propagator> =
        elements.iter().filter_map(|e| Propagator::new(e).ok()).collect();
    assert_eq!(propagators.len(), SATELLITES, "every synthetic element set must be valid");
    // Three days after epoch: a realistic age for a CelesTrak element set.
    let at = EPOCH + SignedDuration::from_hours(72);
    let propagated = propagators.iter().filter(|p| p.position(at).is_ok()).count();
    assert_eq!(propagated, SATELLITES, "every synthetic orbit must propagate");
    let mut group = c.benchmark_group("sgp4_batch_5k");
    group.throughput(Throughput::Elements(SATELLITES as u64));
    group.bench_function("propagator_new", |b| {
        b.iter_batched(
            || elements.clone(),
            |sets| sets.iter().filter_map(|e| Propagator::new(e).ok()).count(),
            BatchSize::LargeInput,
        );
    });
    group.bench_function("geodetic_position", |b| {
        b.iter(|| propagators.iter().filter_map(|p| p.position(black_box(at)).ok()).count());
    });
    group.finish();
}

criterion_group!(benches, bbox_filter, region_keys, sgp4_batch);
criterion_main!(benches);
