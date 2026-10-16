//! Cached snapshot benchmarks (ENG-1000, `docs/performance/specification.md` §2: "Bbox filter
//! of 50 000 points ≤ 2 ms" and "Cached snapshot `GET /api/v1/feeds/{feed}` p95 ≤ 5 ms").
//!
//! `snapshot_50k_aircraft/*` is the hub path a `GET /api/v1/feeds/aircraft` takes once the
//! slot holds data: `LavenderCore::snapshot` subscribes, merges the slot's items, drops
//! duplicate ids (one `String` key and one clone per item), applies the bbox, caps the result
//! at the hub's `max_items` and builds the snapshot. `serialise_json/*` is the JSON encoding
//! the HTTP handler adds on top (it encodes the snapshot twice: once for the `ETag`, once
//! for the body).
//!
//! Input: 50 000 SYNTHETIC aircraft (not real flights), normalised by the real adsb.lol
//! parser and served by a provider that never refreshes during the run. No network.

// Benchmarks are test code: a broken setup should stop the run loudly.
#![allow(clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lavender_core::providers::{
    FeedProvider, FetchContext, Fetched, ProviderError, Region, adsb_lol,
};
use lavender_core::{CoreConfig, LavenderCore, sources};
use lavender_domain::{Aircraft, BBox, FeedId, FeedItems, FeedQuery, FeedSnapshot, ProviderScope};

/// Points in the spec's bbox target.
const AIRCRAFT: usize = 50_000;

/// Serves the same 50 000 aircraft worldwide, with a cadence longer than any run.
#[derive(Debug)]
struct SyntheticAircraft(Vec<Aircraft>);

#[async_trait]
impl FeedProvider for SyntheticAircraft {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_hours(24)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(
        &self,
        _ctx: &FetchContext<'_>,
        _region: Region,
    ) -> Result<Fetched, ProviderError> {
        Ok(Fetched::fresh(FeedItems::Aircraft(self.0.clone())))
    }
}

/// A synthetic adsb.lol document of `count` contacts spread evenly over the world
/// (a fixed lattice, so every run measures the same data), parsed by the real parser.
fn synthetic_aircraft(count: usize) -> Vec<Aircraft> {
    let records: Vec<String> = (0..count)
        .map(|i| {
            let step = u32::try_from(i).unwrap_or(0);
            // A 250 × 200 lattice: longitudes −180…180, latitudes −85…85.
            let lon = -180.0 + f64::from(step % 250) * 1.44 + 0.3;
            let lat = -85.0 + f64::from(step / 250) * 0.85 + 0.2;
            format!(
                r#"{{"hex":"{i:06x}","flight":"SYN{n:05} ","r":"SY-{n:05}","t":"A320","category":"A3","lat":{lat:.5},"lon":{lon:.5},"alt_baro":{alt},"alt_geom":{geom},"gs":431.5,"track":{track:.1},"baro_rate":-64,"squawk":"{squawk:04}","emergency":"none","seen_pos":0.4,"seen":0.2}}"#,
                n = i % 100_000,
                alt = 1_000 + (step * 37) % 40_000,
                geom = 1_250 + (step * 37) % 40_000,
                track = f64::from((step * 13) % 3_600) / 10.0,
                squawk = step % 7_777,
            )
        })
        .collect();
    let document =
        format!(r#"{{"ac":[{}],"msg":"No error","now":1791633600000}}"#, records.join(","));
    let (aircraft, dropped) =
        adsb_lol::parse(document.as_bytes(), false).expect("synthetic adsb.lol document parses");
    assert_eq!((aircraft.len(), dropped), (count, 0));
    aircraft
}

/// Queries a client sends: a city, a continent, one across the antimeridian, the whole world.
fn queries() -> [(&'static str, Option<BBox>); 4] {
    [
        ("city", Some(BBox { west: 24.0, south: 59.9, east: 25.5, north: 60.5 })),
        ("europe", Some(BBox { west: -12.0, south: 35.0, east: 32.0, north: 62.0 })),
        (
            "pacific_antimeridian",
            Some(BBox { west: 150.0, south: -40.0, east: -150.0, north: 20.0 }),
        ),
        ("world", None),
    ]
}

fn aircraft_query(bbox: Option<BBox>) -> FeedQuery {
    FeedQuery { bbox, ..FeedQuery::new(FeedId::Aircraft) }
}

fn snapshot(c: &mut Criterion) {
    let runtime =
        tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    let provider: Arc<dyn FeedProvider> = Arc::new(SyntheticAircraft(synthetic_aircraft(AIRCRAFT)));
    let core = runtime.block_on(async {
        LavenderCore::builder(CoreConfig::default())
            .providers(vec![provider])
            .build()
            .expect("core builds")
    });
    let take = |query: FeedQuery| -> FeedSnapshot {
        runtime.block_on(core.snapshot(query)).expect("aircraft snapshot")
    };
    // The first snapshot fetches; every later one is served from the (lingering) slot.
    let world = take(aircraft_query(None));
    assert_eq!(world.source_count, u32::try_from(AIRCRAFT).unwrap_or(u32::MAX));

    let mut group = c.benchmark_group("snapshot_50k_aircraft");
    group.throughput(Throughput::Elements(AIRCRAFT as u64));
    let mut answers = Vec::new();
    for (name, bbox) in queries() {
        let answer = take(aircraft_query(bbox));
        answers.push((
            name,
            answer.items.len(),
            serde_json::to_vec(&answer).map_or(0, |b| b.len()),
        ));
        group.bench_with_input(BenchmarkId::from_parameter(name), &bbox, |b, bbox| {
            b.iter(|| take(aircraft_query(*bbox)));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("serialise_json");
    for (name, bbox) in [("europe", queries()[1].1), ("world", None)] {
        let answer = take(aircraft_query(bbox));
        group.bench_with_input(BenchmarkId::from_parameter(name), &answer, |b, answer| {
            b.iter(|| serde_json::to_vec(answer).map(|bytes| bytes.len()));
        });
    }
    group.finish();
    core.shutdown();
    for (name, items, bytes) in answers {
        println!(
            "snapshot inputs: {name}: {items} of {AIRCRAFT} aircraft in the answer, {bytes} bytes of JSON"
        );
    }
}

criterion_group!(benches, snapshot);
criterion_main!(benches);
