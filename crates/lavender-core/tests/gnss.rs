#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::cast_precision_loss
)]

use jiff::{SignedDuration, Timestamp};
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_core::gnss::records::{
    GNSS_ANOMALY_STATE, GNSS_CELL_DEG, GNSS_INTERPRETATION, GNSS_NOMINAL_STATE,
    GnssAircraftObservation, evaluate_gnss_level, gnss_cell_bbox, gnss_cell_center,
    gnss_cell_coords, gnss_cell_id, is_degraded,
};
use lavender_core::gnss::store::GnssObservationStore;
use lavender_domain::{BBox, GeoPoint, GnssLevel};
use serde_json::json;

#[test]
fn test_is_degraded_classification() {
    // 1. High accuracy nominal
    assert!(!is_degraded(Some(8), Some(9), false));

    // 2. Low NIC < 7 is degraded
    assert!(is_degraded(Some(6), Some(9), false));
    assert!(is_degraded(Some(1), Some(10), false));

    // 3. Low NACp < 8 is degraded
    assert!(is_degraded(Some(8), Some(7), false));
    assert!(is_degraded(Some(8), Some(2), false));

    // 4. Exception: NIC 0 with NACp >= 10 is NOT degraded
    assert!(!is_degraded(Some(0), Some(10), false));
    assert!(!is_degraded(Some(0), Some(11), false));
    // But NIC 0 with NACp 9 IS degraded
    assert!(is_degraded(Some(0), Some(9), false));

    // 5. GPS loss flag overrides all
    assert!(is_degraded(Some(8), Some(9), true));
    assert!(is_degraded(None, None, true));
}

#[test]
fn test_gnss_grid_coordinates_and_geometry() {
    let lat = 60.1699;
    let lon = 24.9384;
    let (row, col) = gnss_cell_coords(lat, lon, GNSS_CELL_DEG);
    let id = gnss_cell_id(row, col);

    let bbox = gnss_cell_bbox(row, col, GNSS_CELL_DEG);
    let center = gnss_cell_center(row, col, GNSS_CELL_DEG);

    assert!(bbox.contains(&GeoPoint::new(lat, lon).unwrap()));
    assert!(bbox.contains(&center));
    assert_eq!(id, format!("{row}:{col}"));
    assert!((bbox.north - bbox.south - GNSS_CELL_DEG).abs() < 1e-6);
    assert!((bbox.east - bbox.west - GNSS_CELL_DEG).abs() < 1e-6);
}

#[test]
fn test_gnss_level_and_false_positive_discount() {
    // Total 0
    let (pct, level, state, interp) = evaluate_gnss_level(0, 0);
    assert_eq!(pct, 0.0);
    assert_eq!(level, GnssLevel::Low);
    assert_eq!(state, GNSS_NOMINAL_STATE);
    assert!(interp.is_none());

    // 10 aircraft, 0 degraded
    let (pct, level, state, interp) = evaluate_gnss_level(10, 0);
    assert_eq!(pct, 0.0);
    assert_eq!(level, GnssLevel::Low);
    assert_eq!(state, GNSS_NOMINAL_STATE);
    assert!(interp.is_none());

    // 10 aircraft, 1 degraded -> discounted to 0!
    let (pct, level, state, interp) = evaluate_gnss_level(10, 1);
    assert_eq!(pct, 0.0);
    assert_eq!(level, GnssLevel::Low);
    assert_eq!(state, GNSS_NOMINAL_STATE);
    assert!(interp.is_none());

    // 50 aircraft, 2 degraded -> 100 * (2 - 1) / 50 = 2.0% -> Medium anomaly
    let (pct, level, state, interp) = evaluate_gnss_level(50, 2);
    assert_eq!(pct, 2.0);
    assert_eq!(level, GnssLevel::Medium);
    assert_eq!(state, GNSS_ANOMALY_STATE);
    assert_eq!(interp.as_deref(), Some(GNSS_INTERPRETATION));

    // 20 aircraft, 4 degraded -> 100 * (4 - 1) / 20 = 15.0% -> High anomaly
    let (pct, level, state, interp) = evaluate_gnss_level(20, 4);
    assert_eq!(pct, 15.0);
    assert_eq!(level, GnssLevel::High);
    assert_eq!(state, GNSS_ANOMALY_STATE);
    assert_eq!(interp.as_deref(), Some(GNSS_INTERPRETATION));
}

#[test]
fn test_gnss_observation_store_lifecycle() {
    let mut store = GnssObservationStore::default();
    let now = Timestamp::now();

    let p1 = GeoPoint::new(60.1, 24.9).unwrap();
    let p2 = GeoPoint::new(60.2, 24.8).unwrap();
    let p3 = GeoPoint::new(60.3, 24.7).unwrap();

    let obs = vec![
        GnssAircraftObservation {
            hex: "4ca001".to_owned(),
            position: p1,
            nic: Some(8),
            nac_p: Some(9),
            gps_lost: false,
            degraded: false,
            observed_at: now,
        },
        GnssAircraftObservation {
            hex: "4ca002".to_owned(),
            position: p2,
            nic: Some(5), // degraded
            nac_p: Some(9),
            gps_lost: false,
            degraded: true,
            observed_at: now,
        },
        GnssAircraftObservation {
            hex: "4ca003".to_owned(),
            position: p3,
            nic: Some(4), // degraded
            nac_p: Some(7),
            gps_lost: false,
            degraded: true,
            observed_at: now,
        },
    ];

    store.accumulate(&obs, now);
    assert_eq!(store.len(), 3);

    // 3 aircraft in same 0.5° cell (lat 60.0-60.5, lon 24.5-25.0)
    let snapshot = store.bin_cells(now, None);
    assert_eq!(snapshot.total_cells, 1);
    let cell = &snapshot.cells[0];
    assert_eq!(cell.aircraft, 3);
    assert_eq!(cell.degraded, 2);
    // 100 * (2 - 1) / 3 = 33.3% -> High anomaly
    assert_eq!(cell.level, GnssLevel::High);
    assert_eq!(cell.state, GNSS_ANOMALY_STATE);

    // BBox filtering: query outside cell
    let outside_bbox = BBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
    let empty_snapshot = store.bin_cells(now, Some(&outside_bbox));
    assert_eq!(empty_snapshot.total_cells, 0);

    // BBox filtering: query covering cell
    let covering_bbox = BBox::new(24.0, 60.0, 26.0, 61.0).unwrap();
    let matched_snapshot = store.bin_cells(now, Some(&covering_bbox));
    assert_eq!(matched_snapshot.total_cells, 1);

    // Fast-forward past rolling window (31 mins)
    let future_now = now.checked_add(SignedDuration::from_mins(31)).unwrap();
    store.prune(future_now);
    assert_eq!(store.len(), 0);
}

#[test]
fn test_core_gnss_readsb_ingest_and_snapshot() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();

    let current_epoch = Timestamp::now().as_second() as f64;
    let readsb_payload = json!({
        "now": current_epoch,
        "ac": [
            {
                "hex": "4ca111",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 32000,
                "lat": 59.4370,
                "lon": 24.7536,
                "seen_pos": 2.0,
                "nic": 8,
                "nac_p": 9,
                "gpsOkBefore": 0
            },
            {
                "hex": "4ca222",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 34000,
                "lat": 59.4400,
                "lon": 24.7600,
                "seen_pos": 1.0,
                "nic": 5,
                "nac_p": 8,
                "gpsOkBefore": 0
            },
            {
                "hex": "4ca333",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 36000,
                "lat": 59.4350,
                "lon": 24.7500,
                "seen_pos": 3.0,
                "nic": 4,
                "nac_p": 6,
                "gpsOkBefore": 0
            }
        ]
    });

    let bytes = serde_json::to_vec(&readsb_payload).unwrap();
    let count = core.ingest_gnss_readsb(&bytes).unwrap();
    assert_eq!(count, 3);

    let snapshot = core.gnss_snapshot(None).unwrap();
    assert_eq!(snapshot.total_cells, 1);
    assert_eq!(snapshot.cells[0].aircraft, 3);
    assert_eq!(snapshot.cells[0].degraded, 2);
    assert_eq!(snapshot.cells[0].level, GnssLevel::High);

    let prov = core.gnss_provenance();
    assert_eq!(prov.classifier_id, "gev-nic-nacp-v2");
    assert_eq!(prov.window_minutes, 30);
    assert_eq!(prov.min_aircraft, 3);
}
