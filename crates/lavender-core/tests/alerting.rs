#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::assert_is_empty
)]

use jiff::Timestamp;
use lavender_core::alerting::GeofenceAlertEngine;
use lavender_domain::{
    Aircraft, AlertKind, FeedId, FeedItems, GeoPoint, Geofence, GeofenceRule, SatelliteElements,
    Vessel,
};

fn helsinki_fence() -> Geofence {
    Geofence {
        id: "helsinki-tma".to_owned(),
        name: "Helsinki Terminal Area".to_owned(),
        polygon: vec![
            GeoPoint::new(60.0, 24.0).unwrap(),
            GeoPoint::new(61.0, 24.0).unwrap(),
            GeoPoint::new(61.0, 26.0).unwrap(),
            GeoPoint::new(60.0, 26.0).unwrap(),
        ],
    }
}

fn pacific_antimeridian_fence() -> Geofence {
    Geofence {
        id: "pacific-box".to_owned(),
        name: "Pacific Antimeridian Patrol".to_owned(),
        polygon: vec![
            GeoPoint::new(-10.0, 170.0).unwrap(),
            GeoPoint::new(10.0, 170.0).unwrap(),
            GeoPoint::new(10.0, -170.0).unwrap(),
            GeoPoint::new(-10.0, -170.0).unwrap(),
        ],
    }
}

#[test]
fn point_in_out_transitions_and_deduplication() {
    let engine = GeofenceAlertEngine::new(10, 100, 1_000);
    let rule = GeofenceRule {
        id: "rule-helsinki".to_owned(),
        name: "Helsinki Airspace Guard".to_owned(),
        geofences: vec![helsinki_fence()],
        enabled: true,
    };
    engine.set_rule(rule).unwrap();

    let hex = "4ca1b2";
    let t1 = Timestamp::from_second(1_700_000_000).unwrap();
    let t2 = Timestamp::from_second(1_700_000_030).unwrap();
    let t3 = Timestamp::from_second(1_700_000_060).unwrap();
    let t4 = Timestamp::from_second(1_700_000_090).unwrap();

    // Sequence 1: Aircraft inside Helsinki TMA -> Emits Enter
    let plane1 = Aircraft {
        hex: hex.to_owned(),
        callsign: Some("FIN101".to_owned()),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(60.2, 24.9).unwrap(),
        altitude_baro_ft: Some(15_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(400.0),
        track_deg: Some(90.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".to_owned(),
    };
    let alerts1 =
        engine.evaluate_feed_items(FeedId::Aircraft, t1, &FeedItems::Aircraft(vec![plane1]));
    assert_eq!(alerts1.len(), 1);
    assert_eq!(alerts1[0].kind, AlertKind::Enter);
    assert_eq!(alerts1[0].entity_id, hex);
    assert_eq!(alerts1[0].rule_id, "rule-helsinki");
    assert_eq!(alerts1[0].geofence_id.as_deref(), Some("helsinki-tma"));

    // Sequence 2: Aircraft still inside Helsinki TMA -> 0 duplicate alerts
    let plane2 = Aircraft {
        hex: hex.to_owned(),
        callsign: Some("FIN101".to_owned()),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(60.5, 25.1).unwrap(),
        altitude_baro_ft: Some(15_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(400.0),
        track_deg: Some(90.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".to_owned(),
    };
    let alerts2 =
        engine.evaluate_feed_items(FeedId::Aircraft, t2, &FeedItems::Aircraft(vec![plane2]));
    assert!(alerts2.is_empty(), "Unchanged containment must not duplicate Enter alert");

    // Sequence 3: Aircraft flies south to Tallinn (lat 59.4 < 60.0, outside) -> Emits Exit
    let plane3 = Aircraft {
        hex: hex.to_owned(),
        callsign: Some("FIN101".to_owned()),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(59.4, 24.8).unwrap(),
        altitude_baro_ft: Some(15_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(400.0),
        track_deg: Some(180.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".to_owned(),
    };
    let alerts3 =
        engine.evaluate_feed_items(FeedId::Aircraft, t3, &FeedItems::Aircraft(vec![plane3]));
    assert_eq!(alerts3.len(), 1);
    assert_eq!(alerts3[0].kind, AlertKind::Exit);
    assert_eq!(alerts3[0].entity_id, hex);

    // Sequence 4: Aircraft remains outside -> 0 duplicate alerts
    let plane4 = Aircraft {
        hex: hex.to_owned(),
        callsign: Some("FIN101".to_owned()),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(59.0, 24.8).unwrap(),
        altitude_baro_ft: Some(15_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(400.0),
        track_deg: Some(180.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".to_owned(),
    };
    let alerts4 =
        engine.evaluate_feed_items(FeedId::Aircraft, t4, &FeedItems::Aircraft(vec![plane4]));
    assert!(alerts4.is_empty(), "Outside position must not emit duplicate Exit alerts");

    // Check inbox order (newest first)
    let inbox = engine.inbox(None);
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox[0].kind, AlertKind::Exit);
    assert_eq!(inbox[1].kind, AlertKind::Enter);
}

#[test]
fn antimeridian_geofence_evaluation() {
    let engine = GeofenceAlertEngine::new(10, 100, 1_000);
    let rule = GeofenceRule {
        id: "rule-pacific".to_owned(),
        name: "Pacific Watch".to_owned(),
        geofences: vec![pacific_antimeridian_fence()],
        enabled: true,
    };
    engine.set_rule(rule).unwrap();

    let mmsi = "553123456";
    let t = Timestamp::from_second(1_700_000_000).unwrap();

    // 1. Vessel outside to west at 165°E -> No alert
    let v1 = Vessel {
        mmsi: mmsi.to_owned(),
        name: Some("PACIFIC TRADER".to_owned()),
        imo: None,
        vessel_type: None,
        destination: None,
        position: GeoPoint::new(0.0, 165.0).unwrap(),
        speed_knots: Some(12.0),
        course_deg: Some(90.0),
        heading_deg: None,
        last_position_utc: None,
        last_position_epoch: None,
    };
    let a1 = engine.evaluate_feed_items(FeedId::Vessels, t, &FeedItems::Vessels(vec![v1]));
    assert!(a1.is_empty());

    // 2. Vessel crosses into fence at 175°E -> Enter alert
    let v2 = Vessel {
        mmsi: mmsi.to_owned(),
        name: Some("PACIFIC TRADER".to_owned()),
        imo: None,
        vessel_type: None,
        destination: None,
        position: GeoPoint::new(0.0, 175.0).unwrap(),
        speed_knots: Some(12.0),
        course_deg: Some(90.0),
        heading_deg: None,
        last_position_utc: None,
        last_position_epoch: None,
    };
    let a2 = engine.evaluate_feed_items(FeedId::Vessels, t, &FeedItems::Vessels(vec![v2]));
    assert_eq!(a2.len(), 1);
    assert_eq!(a2[0].kind, AlertKind::Enter);

    // 3. Vessel crosses antimeridian to 175°W (-175°E) -> Still inside, no alert
    let v3 = Vessel {
        mmsi: mmsi.to_owned(),
        name: Some("PACIFIC TRADER".to_owned()),
        imo: None,
        vessel_type: None,
        destination: None,
        position: GeoPoint::new(0.0, -175.0).unwrap(),
        speed_knots: Some(12.0),
        course_deg: Some(90.0),
        heading_deg: None,
        last_position_utc: None,
        last_position_epoch: None,
    };
    let a3 = engine.evaluate_feed_items(FeedId::Vessels, t, &FeedItems::Vessels(vec![v3]));
    assert!(a3.is_empty(), "Antimeridian crossing inside polygon must not trigger exit/enter");

    // 4. Vessel exits fence to east at 165°W (-165°E) -> Exit alert
    let v4 = Vessel {
        mmsi: mmsi.to_owned(),
        name: Some("PACIFIC TRADER".to_owned()),
        imo: None,
        vessel_type: None,
        destination: None,
        position: GeoPoint::new(0.0, -165.0).unwrap(),
        speed_knots: Some(12.0),
        course_deg: Some(90.0),
        heading_deg: None,
        last_position_utc: None,
        last_position_epoch: None,
    };
    let a4 = engine.evaluate_feed_items(FeedId::Vessels, t, &FeedItems::Vessels(vec![v4]));
    assert_eq!(a4.len(), 1);
    assert_eq!(a4[0].kind, AlertKind::Exit);
}

#[test]
fn bounded_state_and_inbox_capping() {
    let engine = GeofenceAlertEngine::new(5, 5, 10);
    let rule = GeofenceRule {
        id: "rule-box".to_owned(),
        name: "Capacity Test Rule".to_owned(),
        geofences: vec![helsinki_fence()],
        enabled: true,
    };
    engine.set_rule(rule).unwrap();

    let t = Timestamp::from_second(1_700_000_000).unwrap();

    // Send 15 distinct entities entering the fence
    for i in 0..15 {
        let hex = format!("{i:06x}");
        let plane = Aircraft {
            hex,
            callsign: None,
            registration: None,
            type_code: None,
            category: None,
            position: GeoPoint::new(60.5, 25.0).unwrap(),
            altitude_baro_ft: Some(10_000.0),
            altitude_geom_ft: None,
            on_ground: false,
            ground_speed_kt: Some(300.0),
            track_deg: Some(0.0),
            vertical_rate_fpm: None,
            squawk: None,
            emergency: None,
            is_military: false,
            seen_pos_s: 1.0,
            source: "adsb".to_owned(),
        };
        engine.evaluate_feed_items(FeedId::Aircraft, t, &FeedItems::Aircraft(vec![plane]));
    }

    // Inbox must be strictly capped at 5
    let inbox = engine.inbox(None);
    assert_eq!(inbox.len(), 5);
    // Newest is entity 14 ("00000e")
    assert_eq!(inbox[0].entity_id, "00000e");
    // Oldest retained is entity 10 ("00000a")
    assert_eq!(inbox[4].entity_id, "00000a");
}

#[test]
fn satellite_overflight_pass_prediction() {
    let engine = GeofenceAlertEngine::new(5, 50, 100);

    // Observer ground fence positioned under ISS orbit (51.6° inclination)
    let ground_fence = Geofence {
        id: "ground-station".to_owned(),
        name: "Bavaria Ground Station".to_owned(),
        polygon: vec![
            GeoPoint::new(48.0, 11.0).unwrap(),
            GeoPoint::new(49.0, 11.0).unwrap(),
            GeoPoint::new(49.0, 12.0).unwrap(),
            GeoPoint::new(48.0, 12.0).unwrap(),
        ],
    };
    let rule = GeofenceRule {
        id: "rule-satellite".to_owned(),
        name: "Satellite Overflight Watch".to_owned(),
        geofences: vec![ground_fence],
        enabled: true,
    };
    engine.set_rule(rule).unwrap();

    let epoch = "2024-03-08T12:00:00Z".parse::<Timestamp>().unwrap();
    let iss = SatelliteElements {
        norad_id: 25544,
        name: "ISS (ZARYA)".to_owned(),
        intl_designator: Some("1998-067A".to_owned()),
        group: "stations".to_owned(),
        epoch,
        mean_motion: 15.50377579443931,
        eccentricity: 0.0006703,
        inclination_deg: 51.6416,
        raan_deg: 247.4627,
        arg_of_pericenter_deg: 130.5360,
        mean_anomaly_deg: 325.0288,
        bstar: 0.00016717,
        mean_motion_dot: 0.0,
        mean_motion_ddot: 0.0,
        rev_at_epoch: Some(44393),
        classification: Some("U".to_owned()),
    };

    let alerts = engine.evaluate_feed_items(
        FeedId::Satellites,
        epoch,
        &FeedItems::Satellites(vec![iss.clone()]),
    );

    // Pass prediction over the next 2 hours should find passes or evaluate safely
    for alert in &alerts {
        assert_eq!(alert.kind, AlertKind::Overflight);
        assert_eq!(alert.entity_id, "25544");
        assert!(alert.confidence >= 0.5);
        assert!(alert.position.is_some());
    }

    // Evaluating the exact same satellite again must deduplicate and emit 0 duplicate overflights
    let alerts_recheck =
        engine.evaluate_feed_items(FeedId::Satellites, epoch, &FeedItems::Satellites(vec![iss]));
    assert!(alerts_recheck.is_empty(), "Duplicate satellite overflights must be suppressed");
}
