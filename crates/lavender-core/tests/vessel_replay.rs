//! Replays the synthetic AISStream recording (`apps/web/e2e/fixtures/aisstream.jsonl`)
//! through the real parser and store: store size, removal of old ships, map-area
//! queries, tracks, fixture mode through the core, and vessel search. No AISStream key
//! was available, so the recording is synthetic but follows the published message models.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use lavender_core::config::{AisStreamConfig, CoreConfig};
use lavender_core::providers::aisstream::{ReplayCounts, replay, store_limits};
use lavender_core::providers::fixture::{
    FIXTURE_NOTICE, FixtureProvider, SYNTHETIC_VESSEL_NOTICE, VESSEL_NOTICE,
};
use lavender_core::vessel_store::{Mmsi, StoreStats, VesselLimits, VesselStore};
use lavender_core::{CoreError, LavenderCore, MemorySecretStore, SecretKey};
use lavender_domain::{
    BBox, FeedId, FeedItems, FeedQuery, SearchQuery, Timestamp, Vessel, VesselDimensions,
    VesselEta, VesselVoyage,
};

const RECORDING: &str = include_str!("../../../apps/web/e2e/fixtures/aisstream.jsonl");

fn store() -> VesselStore {
    VesselStore::new(store_limits(&AisStreamConfig::default()))
}

fn bbox(west: f64, south: f64, east: f64, north: f64) -> BBox {
    BBox::new(west, south, east, north).unwrap()
}

fn names(vessels: &[Vessel]) -> Vec<&str> {
    vessels.iter().map(|v| v.name.as_deref().unwrap_or("?")).collect()
}

fn mmsi(text: &str) -> Mmsi {
    Mmsi::parse(text).unwrap()
}

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

#[test]
fn replay_fills_the_store_removes_old_ships_and_answers_area_queries() {
    let store = store();
    let counts = replay(RECORDING, &store);
    assert_eq!(
        (counts.malformed, counts.unavailable_positions, counts.ignored),
        (4, 1, 3),
        "{counts:?}"
    );
    // Seven ships with positions; the static-only ship is known but not shown; the ship
    // last heard at 11:20 is gone by the recording's 12:00 end (30-minute retention).
    assert_eq!(store.stats(), StoreStats { vessels: 8, positioned: 7, track_points: 16 });
    let world = store.in_area(&BBox::WORLD, usize::MAX);
    assert_eq!(world.len(), 7);
    assert!(world.iter().all(|v| v.name.as_deref() != Some("TEST OLD TIMER")));
    assert!(store.track(mmsi("230990007")).is_none());

    let helsinki = store.in_area(&bbox(24.8, 60.0, 25.1, 60.2), usize::MAX);
    assert_eq!(names(&helsinki), ["TEST TRADER", "TEST SAILOR"]);
    let gulf = store.in_area(&bbox(22.0, 59.0, 26.0, 61.0), usize::MAX);
    assert_eq!(
        names(&gulf),
        ["TEST TRADER", "TEST FERRY", "TEST SAILOR", "TEST SENTINEL", "TEST TANKER"]
    );
    let date_line = store.in_area(&bbox(179.0, -18.0, -179.0, -16.0), usize::MAX);
    assert_eq!(names(&date_line), ["TEST EAST", "TEST WEST"]);
    assert_eq!(store.in_area(&bbox(-10.0, 30.0, 0.0, 40.0), usize::MAX), Vec::<Vessel>::new());

    let trader = &helsinki[0];
    assert_eq!(trader.mmsi, "230990001");
    assert_eq!(trader.imo.as_deref(), Some("9999981"));
    assert_eq!(trader.vessel_type.as_deref(), Some("Cargo"));
    assert_eq!(trader.destination.as_deref(), Some("FIHEL"));
    assert_eq!(trader.last_position_utc.as_deref(), Some("2026-10-15T12:00:00Z"));
    assert_eq!(
        store.voyage(mmsi("230990001")),
        Some(VesselVoyage {
            call_sign: Some("XTEST".to_owned()),
            navigational_status: Some("Under way using engine".to_owned()),
            eta: Some(VesselEta { month: 10, day: 15, hour: Some(18), minute: Some(0) }),
            draught_m: Some(6.2),
            dimensions: Some(VesselDimensions { length_m: Some(70), beam_m: Some(12) }),
        })
    );
    let tanker = store.voyage(mmsi("276990003")).unwrap();
    assert_eq!(tanker.navigational_status.as_deref(), Some("At anchor"));
    let sailor = store.voyage(mmsi("230990004"));
    assert_eq!(sailor, None, "Class B, no static data: nothing known");
    let sentinel = gulf.iter().find(|v| v.mmsi == "230990005").unwrap();
    assert_eq!(
        (sentinel.speed_knots, sentinel.course_deg, sentinel.heading_deg),
        (None, None, None)
    );
    let sailor = &helsinki[1];
    assert_eq!((sailor.speed_knots, sailor.heading_deg), (Some(4.2), None));

    assert_eq!(store.track(mmsi("230990001")).unwrap().len(), 7);
    assert_eq!(store.track(mmsi("230990002")).unwrap().len(), 4);
    assert_eq!(store.track(mmsi("276990003")).unwrap().len(), 1, "at anchor: one point");

    // Replaying the same recording again changes nothing.
    let again = replay(RECORDING, &store);
    assert_eq!(again.stored, 4, "only the four static reports (same time, same values) re-apply");
    assert_eq!(store.stats(), StoreStats { vessels: 8, positioned: 7, track_points: 16 });

    // Time passes with no new reports: ships drop out as their last report ages.
    store.advance(at("2026-10-15T12:27:30Z"));
    let later = store.in_area(&BBox::WORLD, usize::MAX);
    assert!(later.iter().all(|v| v.mmsi != "276990003"), "tanker last heard 11:55");
    assert!(later.iter().all(|v| v.mmsi != "230990005"), "sentinel last heard 11:57");
    assert_eq!(later.len(), 5);
    // Expiry is bucketed (one minute here); queries are exact.
    assert_eq!(store.stats().vessels, 7);
    store.advance(at("2026-10-15T12:31:00Z"));
    assert_eq!(store.stats(), StoreStats::default());
}

#[test]
fn replay_respects_the_vessel_cap() {
    let store = VesselStore::new(VesselLimits {
        max_vessels: 3,
        ..store_limits(&AisStreamConfig::default())
    });
    let ReplayCounts { malformed, .. } = replay(RECORDING, &store);
    assert_eq!(malformed, 4);
    assert_eq!(store.stats().vessels, 3);
    // The most recently heard ships survive: the 12:00 trader and two from 11:59.
    let kept = store.in_area(&BBox::WORLD, usize::MAX);
    assert!(kept.iter().any(|v| v.mmsi == "230990001"), "{:?}", names(&kept));
}

fn fixture_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("aisstream.jsonl"), RECORDING).unwrap();
    dir
}

fn fixture_core(dir: &Path, key: bool) -> LavenderCore {
    let secrets = if key {
        MemorySecretStore::with([(SecretKey::AisStreamApiKey, "fixture-not-a-key")])
    } else {
        MemorySecretStore::default()
    };
    LavenderCore::builder(CoreConfig::default())
        .providers(FixtureProvider::all(dir))
        .secrets(Arc::new(secrets))
        .build()
        .unwrap()
}

#[tokio::test]
async fn fixture_mode_serves_vessels_tracks_and_search_from_the_store() {
    let dir = fixture_dir();
    let core = fixture_core(dir.path(), true);
    let snapshot = core
        .snapshot(FeedQuery::new(FeedId::Vessels).with_bbox(bbox(22.0, 59.0, 26.0, 61.0)))
        .await
        .unwrap();
    let FeedItems::Vessels(vessels) = &snapshot.items else { panic!("vessels") };
    assert_eq!(vessels.len(), 5);
    assert_eq!(snapshot.source_count, 7);
    assert_eq!(snapshot.notices.first().map(String::as_str), Some(FIXTURE_NOTICE));
    // The checked-in replay declares itself synthetic, so it is not called a recording.
    assert!(snapshot.notices.iter().any(|n| n == SYNTHETIC_VESSEL_NOTICE));
    assert!(!snapshot.notices.iter().any(|n| n == VESSEL_NOTICE));
    assert!(snapshot.notices.iter().any(|n| n.contains("4 malformed records skipped")));

    let track = core.vessel_track("230990001").unwrap();
    assert_eq!((track.mmsi.as_str(), track.samples.len()), ("230990001", 7));
    assert_eq!((track.source.as_str(), track.retained_sec), ("aisstream", 1_800));
    assert!(track.samples.windows(2).all(|w| w[0].epoch_sec < w[1].epoch_sec));
    assert!(matches!(core.vessel_track("23099001"), Err(CoreError::BadRequest(_))));
    assert!(matches!(core.vessel_track("23099000a"), Err(CoreError::BadRequest(_))));
    assert!(matches!(core.vessel_track("230990099"), Err(CoreError::NotFound(_))));

    let by_name = core
        .search(&SearchQuery { q: "test trader".into(), near: None, limit: None, places: None })
        .await
        .unwrap();
    let hit = by_name.entities.iter().find(|h| h.feed == FeedId::Vessels).unwrap();
    assert_eq!((hit.id.as_str(), hit.label.as_str()), ("230990001", "TEST TRADER"));
    assert_eq!(hit.detail.as_deref(), Some("Cargo"));
    let by_mmsi = core
        .search(&SearchQuery { q: "276990003".into(), near: None, limit: None, places: None })
        .await
        .unwrap();
    assert_eq!(by_mmsi.entities[0].label, "TEST TANKER");
    let by_prefix = core
        .search(&SearchQuery { q: "51299".into(), near: None, limit: None, places: None })
        .await
        .unwrap();
    let mut ids: Vec<&str> = by_prefix.entities.iter().map(|h| h.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["512990008", "512990009"]);
    core.shutdown();
}

#[tokio::test]
async fn without_a_key_the_vessels_layer_is_unconfigured() {
    let dir = fixture_dir();
    let core = fixture_core(dir.path(), false);
    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Vessels).unwrap();
    assert_eq!(
        layer.availability,
        lavender_domain::Availability::Unconfigured {
            missing: vec!["AISStream API key".to_owned()]
        }
    );
    let error = core.snapshot(FeedQuery::new(FeedId::Vessels)).await.unwrap_err();
    assert!(matches!(error, CoreError::Unconfigured { feed: FeedId::Vessels, .. }), "{error:?}");
    assert!(matches!(core.vessel_track("230990001"), Err(CoreError::Unconfigured { .. })));
    tokio::time::sleep(Duration::from_millis(10)).await;
    core.shutdown();
}
