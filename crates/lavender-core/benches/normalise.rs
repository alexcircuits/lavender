//! Normalisation benchmarks (ENG-1000, `docs/performance/specification.md` §2: "Normalise a
//! 10 MiB USGS/adsb.lol payload ≤ 50 ms"): upstream document bytes → validated domain items
//! through the providers' public parsers.
//!
//! Inputs, all offline and deterministic:
//! - the recorded USGS `2.5_day` feed (`tests/fixtures/usgs-2.5_day.geojson`, 45 events), as
//!   recorded and with its features repeated (unique ids) up to 10 MiB;
//! - a synthetic adsb.lol v2 document of 10 MiB with every field readsb publishes, so the
//!   parser skips realistic amounts of unused data. Its contacts are not real flights.

// Benchmarks are test code: a broken fixture should stop the run loudly.
#![allow(clippy::expect_used)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use lavender_core::providers::{adsb_lol, usgs};
use serde_json::Value;

const TEN_MIB: usize = 10 * 1024 * 1024;
const USGS_RECORDED: &[u8] = include_bytes!("../tests/fixtures/usgs-2.5_day.geojson");

/// The recorded USGS document with its features repeated until it is at least `size`
/// bytes. Each copy gets a new id so nothing is deduplicated downstream.
fn usgs_document(size: usize) -> (Vec<u8>, usize) {
    let mut document: Value = serde_json::from_slice(USGS_RECORDED).expect("recorded USGS JSON");
    let recorded = document["features"].as_array().cloned().expect("USGS features array");
    let per_copy = serde_json::to_vec(&recorded).expect("serialise features").len();
    let copies = size.div_ceil(per_copy);
    let features: Vec<Value> = (0..copies)
        .flat_map(|copy| {
            recorded.iter().map(move |feature| {
                let mut feature = feature.clone();
                let id = feature["id"].as_str().unwrap_or("event").to_owned();
                feature["id"] = Value::String(format!("{id}-{copy}"));
                feature
            })
        })
        .collect();
    let count = features.len();
    document["features"] = Value::Array(features);
    document["metadata"]["count"] = Value::from(count);
    (serde_json::to_vec(&document).expect("serialise USGS document"), count)
}

/// One synthetic readsb record with the full field set adsb.lol forwards.
fn readsb_record(i: usize) -> String {
    // Spread contacts over the inhabited latitudes; values vary deterministically with `i`.
    let step = u32::try_from(i % 100_000).unwrap_or(0);
    let lat = -50.0 + f64::from(step % 1_000) * 0.11;
    let lon = -179.0 + f64::from((step * 7) % 3_580) * 0.1;
    let alt = 1_000 + (step * 37) % 40_000;
    let track = f64::from((step * 13) % 3_600) / 10.0;
    format!(
        r#"{{"hex":"{i:06x}","type":"adsb_icao","flight":"SYN{n:05} ","r":"SY-{n:05}","t":"A320","desc":"SYNTHETIC A320","ownOp":"SYNTHETIC OPERATOR","year":"2015","alt_baro":{alt},"alt_geom":{geom},"gs":{gs:.1},"ias":250,"tas":460,"mach":0.780,"wd":270,"ws":40,"oat":-50,"tat":-25,"track":{track:.2},"track_rate":0.03,"roll":0.18,"mag_heading":{track:.2},"true_heading":{track:.2},"baro_rate":-64,"geom_rate":-32,"squawk":"{squawk:04}","emergency":"none","category":"A3","nav_qnh":1013.2,"nav_altitude_mcp":{alt},"nav_heading":{track:.2},"lat":{lat:.6},"lon":{lon:.6},"nic":8,"rc":186,"seen_pos":0.4,"version":2,"nic_baro":1,"nac_p":9,"nac_v":1,"sil":3,"sil_type":"perhour","gva":2,"sda":2,"alert":0,"spi":0,"mlat":[],"tisb":[],"messages":{messages},"seen":0.2,"rssi":-21.4,"dst":42.1,"dir":181.5}}"#,
        n = i % 100_000,
        geom = alt + 250,
        gs = 120.0 + f64::from(step % 380),
        squawk = step % 7_777,
        messages = 1_000 + step,
    )
}

/// A synthetic adsb.lol v2 response of at least `size` bytes.
fn adsb_lol_document(size: usize) -> (Vec<u8>, usize) {
    let mut body = String::from(r#"{"ac":["#);
    let mut count = 0;
    while body.len() < size {
        if count > 0 {
            body.push(',');
        }
        body.push_str(&readsb_record(count));
        count += 1;
    }
    body.push_str(
        r#"],"msg":"No error","now":1791633600000,"total":0,"ctime":1791633600000,"ptime":1}"#,
    );
    (body.into_bytes(), count)
}

fn normalise(c: &mut Criterion) {
    let mut group = c.benchmark_group("normalise");
    // Each iteration takes tens of milliseconds; 30 samples keep a run under a minute.
    group.sample_size(30);

    let recorded = usgs::parse(USGS_RECORDED).expect("recorded USGS parses");
    assert_eq!(recorded.0.len(), 45);
    group.throughput(Throughput::Bytes(USGS_RECORDED.len() as u64));
    group.bench_function("usgs_recorded_45_events", |b| b.iter(|| usgs::parse(USGS_RECORDED)));

    let (usgs_body, events) = usgs_document(TEN_MIB);
    assert_eq!(usgs::parse(&usgs_body).expect("10 MiB USGS parses").0.len(), events);
    group.throughput(Throughput::Bytes(usgs_body.len() as u64));
    group.bench_function("usgs_10mib", |b| b.iter(|| usgs::parse(&usgs_body)));

    let (adsb_body, aircraft) = adsb_lol_document(TEN_MIB);
    assert_eq!(
        adsb_lol::parse(&adsb_body, false).expect("10 MiB adsb.lol parses").0.len(),
        aircraft
    );
    group.throughput(Throughput::Bytes(adsb_body.len() as u64));
    group.bench_function("adsb_lol_10mib", |b| b.iter(|| adsb_lol::parse(&adsb_body, false)));
    group.finish();
    println!(
        "normalise inputs: USGS 10 MiB = {events} events ({} bytes); adsb.lol 10 MiB = {aircraft} aircraft ({} bytes)",
        usgs_body.len(),
        adsb_body.len()
    );
}

criterion_group!(benches, normalise);
criterion_main!(benches);
