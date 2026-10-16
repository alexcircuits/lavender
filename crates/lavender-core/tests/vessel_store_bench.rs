//! Repeatable benchmark: map-area queries over a store of 50 000 vessels (ENG-953 asks
//! for at most 2 ms). Ignored by default; run in release mode:
//!
//! ```text
//! cargo test --release -p lavender-core --test vessel_store_bench -- --ignored --nocapture
//! ```
//!
//! The fleet is deterministic (seeded): 70 % of ships cluster around 24 busy shipping
//! areas, 30 % are spread over the oceans. Every ship has static data, so each query
//! clones realistic `Vessel` records exactly as the vessels feed does per map cell.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use jiff::Timestamp;
use lavender_core::config::AisStreamConfig;
use lavender_core::providers::aisstream::{WORLD_VIEW_LIMIT, store_limits};
use lavender_core::vessel_store::{Mmsi, VesselDetails, VesselPosition, VesselStore};
use lavender_domain::{BBox, GeoPoint};

const FLEET: u32 = 50_000;
const BUDGET: Duration = Duration::from_millis(2);
const RUNS: usize = 300;

/// Busy areas (lat, lon): North Sea, Channel, Gulf of Finland, Gibraltar, Bosporus,
/// Suez, Persian Gulf, Malacca, Singapore, Hong Kong, Shanghai, Busan, Tokyo Bay, Gulf of
/// Mexico, New York, Los Angeles, Panama, Santos, Cape Town, Mumbai, Sydney, Lagos,
/// Vancouver, Rotterdam.
const HUBS: [(f64, f64); 24] = [
    (55.0, 4.0),
    (50.2, -1.0),
    (59.8, 25.0),
    (36.0, -5.6),
    (41.1, 29.0),
    (30.5, 32.4),
    (26.5, 52.5),
    (3.0, 100.8),
    (1.2, 103.8),
    (22.2, 114.2),
    (31.2, 122.0),
    (35.0, 129.0),
    (35.3, 139.8),
    (28.5, -90.0),
    (40.4, -73.8),
    (33.6, -118.3),
    (9.0, -79.6),
    (-24.0, -46.3),
    (-34.0, 18.4),
    (18.9, 72.8),
    (-33.9, 151.3),
    (6.4, 3.4),
    (49.3, -123.2),
    (51.9, 4.1),
];

fn fleet(store: &VesselStore, now: Timestamp) {
    let mut rng = fastrand::Rng::with_seed(953);
    let spread = |rng: &mut fastrand::Rng| (rng.f64() + rng.f64() + rng.f64() - 1.5) * 2.0;
    for i in 0..FLEET {
        let (lat, lon) = if rng.u8(0..10) < 7 {
            let (lat, lon) = HUBS[rng.usize(0..HUBS.len())];
            (lat + spread(&mut rng), lon + spread(&mut rng))
        } else {
            (rng.f64() * 130.0 - 60.0, rng.f64() * 360.0 - 180.0)
        };
        let lon = if lon > 180.0 {
            lon - 360.0
        } else if lon < -180.0 {
            lon + 360.0
        } else {
            lon
        };
        let mmsi = Mmsi::from_number(u64::from(200_000_000 + i)).unwrap();
        let position = GeoPoint::new(lat.clamp(-89.0, 89.0), lon).unwrap();
        store.record_details(
            VesselDetails {
                name: Some(format!("BENCH VESSEL {i:05}")),
                imo: Some("9999993".to_owned()),
                vessel_type: Some("Cargo".to_owned()),
                destination: Some("NLRTM".to_owned()),
                ..VesselDetails::new(mmsi)
            },
            now,
        );
        let report = VesselPosition {
            mmsi,
            position,
            speed_knots: Some(12.0),
            course_deg: Some(90.0),
            heading_deg: Some(90.0),
            observed_at: Some(now),
            name_hint: None,
            navigational_status: None,
        };
        store.record_position(report, now);
    }
}

struct Measured {
    name: &'static str,
    results: usize,
    median: Duration,
    p95: Duration,
    max: Duration,
}

fn measure(name: &'static str, query: impl Fn() -> usize) -> Measured {
    for _ in 0..20 {
        std::hint::black_box(query());
    }
    let mut times = Vec::with_capacity(RUNS);
    let mut results = 0;
    for _ in 0..RUNS {
        let started = Instant::now();
        results = std::hint::black_box(query());
        times.push(started.elapsed());
    }
    times.sort_unstable();
    Measured {
        name,
        results,
        median: times[RUNS / 2],
        p95: times[RUNS * 95 / 100],
        max: times[RUNS - 1],
    }
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored --nocapture"]
fn map_area_queries_over_50_000_vessels_take_at_most_2_ms() {
    // Timings from a debug build mean nothing; the budget is checked in release only.
    let release = !cfg!(debug_assertions);
    let store = VesselStore::new(store_limits(&AisStreamConfig::default()));
    let now = Timestamp::now();
    let started = Instant::now();
    fleet(&store, now);
    let ingest = started.elapsed();
    assert_eq!(store.stats().positioned, 50_000);

    let area = |w, s, e, n| BBox::new(w, s, e, n).unwrap();
    let north_sea = area(0.0, 50.0, 10.0, 60.0);
    let harbour = area(3.5, 51.5, 4.5, 52.5);
    let date_line = area(175.0, -20.0, -175.0, -10.0);
    let europe = area(-10.0, 40.0, 30.0, 70.0);
    let cells = [
        measure("10° cell, busiest area (North Sea)", || {
            store.in_area(&north_sea, usize::MAX).len()
        }),
        measure("1° harbour view (Rotterdam)", || store.in_area(&harbour, usize::MAX).len()),
        measure("10° box across the antimeridian", || store.in_area(&date_line, usize::MAX).len()),
    ];
    let wide = [
        measure("40°x30° viewport (Europe, 12 cells' worth)", || {
            store.in_area(&europe, usize::MAX).len()
        }),
        measure("world view: most recent 10 000", || store.most_recent(WORLD_VIEW_LIMIT).len()),
    ];
    println!("ingest of {FLEET} details + {FLEET} positions: {ingest:?}");
    println!("{:<45} {:>8} {:>10} {:>10} {:>10}", "query", "vessels", "median", "p95", "max");
    for m in cells.iter().chain(&wide) {
        println!(
            "{:<45} {:>8} {:>10.3?} {:>10.3?} {:>10.3?}",
            m.name, m.results, m.median, m.p95, m.max
        );
    }
    if !release {
        println!("debug build: timings are not representative and the budget is not checked");
        return;
    }
    for m in &cells {
        assert!(m.p95 <= BUDGET, "{}: p95 {:?} exceeds {BUDGET:?}", m.name, m.p95);
    }
}
