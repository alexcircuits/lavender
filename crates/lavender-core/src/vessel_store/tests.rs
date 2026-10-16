//! Store behaviour on a simulated clock: area queries, retention, capacity and tracks.

use super::*;

/// Simulated time: seconds after an instant that starts a retention bucket.
fn t(second: i64) -> Timestamp {
    Timestamp::from_second(1_759_999_980 + second).unwrap()
}

fn mmsi(n: u32) -> Mmsi {
    Mmsi::from_number(u64::from(n)).unwrap()
}

fn limits() -> VesselLimits {
    VesselLimits {
        max_vessels: 100,
        max_track_points: 8,
        max_total_track_points: 1_000,
        retention: Duration::from_mins(30),
    }
}

fn at(n: u32, lat: f64, lon: f64, second: i64) -> VesselPosition {
    VesselPosition {
        mmsi: mmsi(n),
        position: GeoPoint::new(lat, lon).unwrap(),
        speed_knots: Some(10.0),
        course_deg: Some(90.0),
        heading_deg: Some(91.0),
        observed_at: Some(t(second)),
        name_hint: None,
        navigational_status: None,
    }
}

fn ids(vessels: &[Vessel]) -> Vec<&str> {
    vessels.iter().map(|v| v.mmsi.as_str()).collect()
}

fn bbox(west: f64, south: f64, east: f64, north: f64) -> BBox {
    BBox::new(west, south, east, north).unwrap()
}

#[test]
fn mmsi_must_be_nine_digits_and_keeps_leading_zeros() {
    assert_eq!(Mmsi::parse("230123456").map(|m| m.to_string()), Some("230123456".to_owned()));
    assert_eq!(Mmsi::parse("002301234").map(|m| m.to_string()), Some("002301234".to_owned()));
    for bad in ["", "23012345", "2301234567", "23012345a", "000000000", " 23012345", "+23012345"] {
        assert_eq!(Mmsi::parse(bad), None, "{bad:?}");
    }
    assert_eq!(Mmsi::from_number(0), None);
    assert_eq!(Mmsi::from_number(1_000_000_000), None);
    assert_eq!(Mmsi::from_number(2_301_234).map(|m| m.to_string()), Some("002301234".to_owned()));
}

#[test]
fn area_queries_use_current_positions_including_across_the_antimeridian() {
    let store = VesselStore::new(limits());
    store.record_position(at(230_000_001, 60.1, 24.9, 0), t(0));
    store.record_position(at(230_000_002, 59.4, 24.7, 0), t(0));
    store.record_position(at(512_000_003, -17.0, 179.5, 0), t(0));
    store.record_position(at(512_000_004, -17.2, -179.6, 0), t(0));
    assert_eq!(ids(&store.in_area(&bbox(24.0, 59.0, 26.0, 61.0), 10)), ["230000001", "230000002"]);
    assert_eq!(
        ids(&store.in_area(&bbox(179.0, -18.0, -179.0, -16.0), 10)),
        ["512000003", "512000004"]
    );
    assert_eq!(store.in_area(&BBox::WORLD, 3).len(), 3, "the limit applies");
    // A ship that moves leaves its old area.
    store.record_position(at(230_000_001, 10.0, 10.0, 60), t(60));
    assert_eq!(ids(&store.in_area(&bbox(24.0, 59.0, 26.0, 61.0), 10)), ["230000002"]);
    assert_eq!(ids(&store.in_area(&bbox(9.0, 9.0, 11.0, 11.0), 10)), ["230000001"]);
    assert_eq!(store.stats().positioned, 4);
}

#[test]
fn ships_not_heard_within_the_retention_window_are_removed() {
    let store = VesselStore::new(limits());
    store.record_position(at(230_000_001, 60.1, 24.9, 0), t(0));
    store.record_position(at(230_000_002, 60.2, 24.9, 0), t(0));
    store.record_position(at(230_000_002, 60.3, 24.9, 1_200), t(1_200));
    store.advance(t(1_800 + 59));
    assert_eq!(store.stats().vessels, 2, "expiry is bucketed: at most one bucket late");
    assert_eq!(ids(&store.in_area(&BBox::WORLD, 10)), ["230000002"], "queries are exact");
    store.advance(t(1_800 + 60));
    assert_eq!(store.stats(), StoreStats { vessels: 1, positioned: 1, track_points: 2 });
    assert!(store.track(mmsi(230_000_001)).is_none());
    store.advance(t(3_000 + 60));
    assert_eq!(store.stats(), StoreStats::default());
}

#[test]
fn capacity_evicts_the_least_recently_heard_ship() {
    let store = VesselStore::new(VesselLimits { max_vessels: 3, ..limits() });
    for (i, n) in [1, 2, 3].into_iter().enumerate() {
        let second = i64::try_from(i).unwrap() * 120;
        store.record_position(at(230_000_000 + n, 60.0, 24.0 + f64::from(n), second), t(second));
    }
    // Ship 1 is heard again, so ship 2 is now the least recently heard.
    store.record_position(at(230_000_001, 60.5, 24.0, 400), t(400));
    store.record_position(at(230_000_004, 61.0, 25.0, 500), t(500));
    assert_eq!(ids(&store.in_area(&BBox::WORLD, 10)), ["230000001", "230000003", "230000004"]);
    assert_eq!(store.stats().vessels, 3);
}

#[test]
fn old_out_of_order_and_future_reports_are_handled_honestly() {
    let store = VesselStore::new(limits());
    store.advance(t(10_000));
    assert_eq!(
        store.record_position(at(230_000_001, 60.0, 24.0, 8_000), t(10_000)),
        Recorded::TooOld
    );
    assert_eq!(store.stats().vessels, 0);
    assert_eq!(
        store.record_position(at(230_000_001, 60.0, 24.0, 9_990), t(10_000)),
        Recorded::Stored
    );
    assert_eq!(
        store.record_position(at(230_000_001, 61.0, 24.0, 9_980), t(10_000)),
        Recorded::Superseded,
        "an older report never moves the ship back"
    );
    assert_eq!(store.in_area(&BBox::WORLD, 1)[0].position.lat, 60.0);
    // A report stamped in the future is clamped to the store clock, not trusted.
    store.record_position(at(230_000_002, 60.0, 25.0, 99_999), t(10_001));
    let shown = store.in_area(&bbox(24.5, 59.0, 25.5, 61.0), 1);
    assert_eq!(shown[0].last_position_epoch, Some(t(10_001).as_second()));
    // The clock never runs backwards.
    store.advance(t(0));
    assert_eq!(store.record_position(at(230_000_003, 60.0, 26.0, 0), t(0)), Recorded::TooOld);
}

#[test]
fn missing_observation_time_means_received_now() {
    let store = VesselStore::new(limits());
    let mut report = at(230_000_001, 60.0, 24.0, 0);
    report.observed_at = None;
    store.record_position(report, t(42));
    let vessel = &store.in_area(&BBox::WORLD, 1)[0];
    assert_eq!(vessel.last_position_epoch, Some(t(42).as_second()));
    assert_eq!(vessel.last_position_utc.as_deref(), Some(t(42).to_string().as_str()));
}

#[test]
fn details_name_a_ship_and_wait_for_its_first_position() {
    let store = VesselStore::new(limits());
    let mut hinted = at(230_000_001, 60.0, 24.0, 0);
    hinted.name_hint = Some("HINT".to_owned());
    store.record_position(hinted, t(0));
    assert_eq!(store.in_area(&BBox::WORLD, 1)[0].name.as_deref(), Some("HINT"));
    let details = VesselDetails {
        name: Some("TEST CARRIER".to_owned()),
        imo: Some("9074729".to_owned()),
        vessel_type: Some("Cargo".to_owned()),
        destination: Some("HELSINKI".to_owned()),
        observed_at: Some(t(10)),
        ..VesselDetails::new(mmsi(230_000_001))
    };
    store.record_details(details, t(10));
    let partial = VesselDetails {
        destination: Some("TALLINN".to_owned()),
        observed_at: Some(t(20)),
        ..VesselDetails::new(mmsi(230_000_001))
    };
    store.record_details(partial, t(20));
    let vessel = &store.in_area(&BBox::WORLD, 1)[0];
    assert_eq!(vessel.name.as_deref(), Some("TEST CARRIER"));
    assert_eq!(vessel.imo.as_deref(), Some("9074729"));
    assert_eq!(vessel.vessel_type.as_deref(), Some("Cargo"));
    assert_eq!(vessel.destination.as_deref(), Some("TALLINN"), "newer details replace older");

    store.record_details(
        VesselDetails {
            name: Some("NO FIX YET".to_owned()),
            ..VesselDetails::new(mmsi(230_000_002))
        },
        t(30),
    );
    assert_eq!(store.stats(), StoreStats { vessels: 2, positioned: 1, track_points: 1 });
    assert_eq!(store.most_recent(10).len(), 1, "a ship without a position is not shown");
    store.record_position(at(230_000_002, 59.0, 24.0, 40), t(40));
    let shown = store.most_recent(10);
    let named = shown.iter().find(|v| v.mmsi == "230000002").unwrap();
    assert_eq!(named.name.as_deref(), Some("NO FIX YET"));
}

#[test]
fn voyage_details_and_navigational_status_are_kept_for_the_selected_ship() {
    let store = VesselStore::new(limits());
    let id = mmsi(230_000_001);
    assert_eq!(store.voyage(id), None, "unknown ship");
    store.record_position(at(230_000_001, 60.0, 24.0, 0), t(0));
    assert_eq!(store.voyage(id), None, "nothing known yet");
    let mut anchored = at(230_000_001, 60.0, 24.0, 5);
    anchored.navigational_status = Some("At anchor");
    store.record_position(anchored, t(5));
    let eta = VesselEta { month: 10, day: 17, hour: Some(8), minute: None };
    let dimensions = VesselDimensions { length_m: Some(70), beam_m: Some(12) };
    store.record_details(
        VesselDetails {
            call_sign: Some("XTEST".to_owned()),
            eta: Some(eta),
            draught_m: Some(6.2),
            dimensions: Some(dimensions),
            observed_at: Some(t(10)),
            ..VesselDetails::new(id)
        },
        t(10),
    );
    // Later static data without these fields keeps what is known.
    store.record_details(
        VesselDetails { destination: Some("TALLINN".to_owned()), ..VesselDetails::new(id) },
        t(20),
    );
    assert_eq!(
        store.voyage(id),
        Some(VesselVoyage {
            call_sign: Some("XTEST".to_owned()),
            navigational_status: Some("At anchor".to_owned()),
            eta: Some(eta),
            draught_m: Some(6.2),
            dimensions: Some(dimensions),
        })
    );
    // Feed views stay lean: the voyage is not part of the streamed ship.
    let shown = serde_json::to_value(&store.in_area(&BBox::WORLD, 1)[0]).unwrap();
    assert!(shown.get("callSign").is_none() && shown.get("eta").is_none());
    // The status belongs to the report: a newer one without it (Class B) clears it.
    store.record_position(at(230_000_001, 60.1, 24.0, 60), t(60));
    let voyage = store.voyage(id).unwrap();
    assert_eq!((voyage.navigational_status, voyage.call_sign.as_deref()), (None, Some("XTEST")));
    // Once the ship expires, nothing is reported.
    store.advance(t(60 + 31 * 60));
    assert_eq!(store.voyage(id), None);
}

#[test]
fn tracks_are_thinned_capped_and_windowed() {
    let store = VesselStore::new(limits());
    let id = mmsi(230_000_001);
    store.record_position(at(230_000_001, 60.0, 24.0, 0), t(0));
    // Too soon, then too close: neither adds a point.
    store.record_position(at(230_000_001, 60.01, 24.0, 10), t(10));
    store.record_position(at(230_000_001, 60.0001, 24.0, 100), t(100));
    assert_eq!(store.track(id).unwrap().len(), 1);
    for step in 1..=10_u8 {
        let second = 100 + i64::from(step) * 60;
        store.record_position(
            at(230_000_001, 60.0 + 0.01 * f64::from(step), 24.0, second),
            t(second),
        );
    }
    let track = store.track(id).unwrap();
    assert_eq!(track.len(), 8, "per-ship cap");
    assert!(track.windows(2).all(|w| w[0].epoch_sec < w[1].epoch_sec), "oldest first");
    assert_eq!(track.last().unwrap().epoch_sec, t(700).as_second());
    assert_eq!(store.stats().track_points, 8);
}

#[test]
fn a_ship_that_stays_put_keeps_a_retained_track_while_it_is_heard() {
    let store = VesselStore::new(limits());
    let id = mmsi(230_000_001);
    // Moored for an hour, reporting every three minutes from the same spot.
    for minute in (0..=60).step_by(3) {
        let second = minute * 60;
        store.record_position(at(230_000_001, 60.0, 24.0, second), t(second));
        let track = store.track(id).unwrap_or_else(|| panic!("no track at minute {minute}"));
        assert!(track.len() <= 3, "a stationary ship adds few points: {track:?}");
        let newest = track.last().unwrap().epoch_sec;
        assert!(t(second).as_second() - newest <= 15 * 60, "minute {minute}: {track:?}");
    }
    assert_eq!(store.in_area(&BBox::WORLD, 10).len(), 1);
}

#[test]
fn the_global_track_cap_drops_the_least_recently_extended_tracks_but_keeps_ships() {
    let store = VesselStore::new(VesselLimits { max_total_track_points: 5, ..limits() });
    for (n, second) in [(1, 0), (2, 60), (3, 120)] {
        store.record_position(at(230_000_000 + n, 60.0, 24.0 + f64::from(n), second), t(second));
        store.record_position(
            at(230_000_000 + n, 60.1, 24.0 + f64::from(n), second + 40),
            t(second + 40),
        );
    }
    // Six points were offered; ship 1's track (the oldest) was dropped to stay within 5.
    assert_eq!(store.stats(), StoreStats { vessels: 3, positioned: 3, track_points: 4 });
    assert!(store.track(mmsi(230_000_001)).is_none());
    assert_eq!(store.track(mmsi(230_000_003)).unwrap().len(), 2);
    assert_eq!(store.in_area(&BBox::WORLD, 10).len(), 3);
}

#[test]
fn most_recent_lists_newest_first() {
    let store = VesselStore::new(limits());
    for (n, second) in [(1, 0), (2, 600), (3, 1_200)] {
        store.record_position(at(230_000_000 + n, 60.0, 24.0 + f64::from(n), second), t(second));
    }
    assert_eq!(ids(&store.most_recent(2)), ["230000003", "230000002"]);
}

#[test]
fn area_queries_agree_with_a_linear_scan_after_random_updates() {
    let mut rng = fastrand::Rng::with_seed(7);
    let store = VesselStore::new(VesselLimits {
        max_vessels: 300,
        retention: Duration::from_hours(24),
        ..limits()
    });
    let mut latest: HashMap<u32, (f64, f64)> = HashMap::new();
    for step in 0..3_000_i64 {
        let n = 230_000_000 + rng.u32(0..400);
        let (lat, lon) = (rng.f64() * 170.0 - 85.0, rng.f64() * 360.0 - 180.0);
        store.record_position(at(n, lat, lon, step), t(step));
        latest.insert(n, (lat, lon));
    }
    let known: Vec<Vessel> = store.in_area(&BBox::WORLD, usize::MAX);
    assert_eq!(known.len(), 300, "capacity holds");
    for _ in 0..50 {
        let (w, e) = (rng.f64() * 360.0 - 180.0, rng.f64() * 360.0 - 180.0);
        let s = rng.f64() * 90.0 - 90.0;
        let b = bbox(w, s, e, s + rng.f64() * 90.0);
        let expected: Vec<&str> =
            known.iter().filter(|v| b.contains(&v.position)).map(|v| v.mmsi.as_str()).collect();
        assert_eq!(ids(&store.in_area(&b, usize::MAX)), expected, "{b:?}");
    }
    for vessel in &known {
        let n: u32 = vessel.mmsi.parse().unwrap();
        assert_eq!((vessel.position.lat, vessel.position.lon), latest[&n], "index holds latest");
    }
}
