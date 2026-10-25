#![allow(clippy::unwrap_used)]

use lavender_domain::{
    Aircraft, Coverage, EntityIdentifier, EntityKind, FeedId, FeedItems, FeedSnapshot, Freshness,
    GeoPoint, SatelliteElements, Vessel,
};

use super::*;
use crate::config::ResolutionConfig;

fn test_snapshot(items: FeedItems) -> FeedSnapshot {
    FeedSnapshot {
        feed: FeedId::Aircraft,
        sources: vec!["test-source".into()],
        fetched_at: Some("2026-10-25T12:00:00Z".parse().unwrap()),
        generated_at: "2026-10-25T12:00:00Z".parse().unwrap(),
        freshness: Freshness::Live,
        coverage: Coverage::Complete,
        source_count: u32::try_from(items.len()).unwrap_or(0),
        pending_regions: 0,
        items,
        notices: vec![],
    }
}

#[test]
fn aircraft_snapshot_ingest_and_resolution() {
    let engine = ResolutionEngine::new(ResolutionConfig::default());

    let point = GeoPoint::new(51.5, -0.1).unwrap();
    let ac = Aircraft {
        hex: "4ca1b2".into(),
        callsign: Some("RYR12AB".into()),
        registration: Some("EI-ABC".into()),
        type_code: Some("B738".into()),
        category: Some("A3".into()),
        position: point,
        altitude_baro_ft: Some(30000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(420.0),
        track_deg: Some(180.0),
        vertical_rate_fpm: None,
        squawk: Some("1200".into()),
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb-lol".into(),
    };

    let snapshot = test_snapshot(FeedItems::Aircraft(vec![ac]));
    let count = engine.ingest_feed_snapshot(&snapshot);
    assert_eq!(count, 1);

    // Resolve by icao24
    let entity = engine.resolve("icao24", "4ca1b2").unwrap();
    assert_eq!(entity.id, "canonical:aircraft:4ca1b2");
    assert_eq!(entity.kind, EntityKind::Aircraft);
    assert_eq!(entity.primary_label, "RYR12AB");

    // Resolve case-insensitively
    let by_hex_upper = engine.resolve("ICAO24", "4CA1B2").unwrap();
    assert_eq!(by_hex_upper.id, entity.id);

    // Resolve by callsign
    let by_cs = engine.resolve("callsign", "ryr12ab").unwrap();
    assert_eq!(by_cs.id, entity.id);

    // Resolve by registration
    let by_reg = engine.resolve("registration", "ei-abc").unwrap();
    assert_eq!(by_reg.id, entity.id);

    // Get aliases
    let aliases = engine.get_entity_aliases(&entity.id).unwrap();
    assert_eq!(aliases.len(), 3);

    // Get links
    let links = engine.get_entity_links(&entity.id);
    assert_eq!(links.len(), 3);
}

#[test]
fn vessel_and_satellite_ingest_and_resolution() {
    let engine = ResolutionEngine::new(ResolutionConfig::default());

    let point = GeoPoint::new(60.17, 24.94).unwrap();
    let v = Vessel {
        mmsi: "230123456".into(),
        name: Some("FINNSTAR".into()),
        imo: Some("9319440".into()),
        vessel_type: Some("Passenger".into()),
        destination: Some("HELSINKI".into()),
        position: point,
        speed_knots: Some(18.5),
        course_deg: Some(90.0),
        heading_deg: Some(92.0),
        last_position_utc: None,
        last_position_epoch: Some(1_729_000_000),
    };

    let sat = SatelliteElements {
        norad_id: 25544,
        name: "ISS (ZARYA)".into(),
        intl_designator: Some("1998-067A".into()),
        group: "stations".into(),
        epoch: "2026-10-25T12:00:00Z".parse().unwrap(),
        mean_motion: 15.5,
        eccentricity: 0.0007,
        inclination_deg: 51.6,
        raan_deg: 247.4,
        arg_of_pericenter_deg: 130.5,
        mean_anomaly_deg: 325.0,
        bstar: 0.0003,
        mean_motion_dot: 0.0001,
        mean_motion_ddot: 0.0,
        rev_at_epoch: Some(1),
        classification: Some("U".into()),
    };

    let snap_vessel = test_snapshot(FeedItems::Vessels(vec![v]));
    let snap_sat = test_snapshot(FeedItems::Satellites(vec![sat]));

    assert_eq!(engine.ingest_feed_snapshot(&snap_vessel), 1);
    assert_eq!(engine.ingest_feed_snapshot(&snap_sat), 1);

    // Vessel lookup by MMSI, IMO, and Name
    let ship = engine.resolve("mmsi", "230123456").unwrap();
    assert_eq!(ship.primary_label, "FINNSTAR");
    assert_eq!(ship.kind, EntityKind::Vessel);

    let ship_by_imo = engine.resolve("imo", "9319440").unwrap();
    assert_eq!(ship_by_imo.id, ship.id);

    let ship_by_name = engine.resolve("name", "finnstar").unwrap();
    assert_eq!(ship_by_name.id, ship.id);

    // Satellite lookup by NORAD cat id, intl designator, and name
    let iss = engine.resolve("norad_cat_id", "25544").unwrap();
    assert_eq!(iss.kind, EntityKind::Satellite);
    assert_eq!(iss.primary_label, "ISS (ZARYA)");

    let iss_by_intl = engine.resolve("intl_designator", "1998-067A").unwrap();
    assert_eq!(iss_by_intl.id, iss.id);

    let iss_by_name = engine.resolve("name", "iss (zarya)").unwrap();
    assert_eq!(iss_by_name.id, iss.id);
}

#[test]
fn manual_alias_linking_and_splitting() {
    let engine = ResolutionEngine::new(ResolutionConfig::default());

    let point = GeoPoint::new(51.5, -0.1).unwrap();
    let ac = Aircraft {
        hex: "112233".into(),
        callsign: Some("ALPHA1".into()),
        registration: None,
        type_code: None,
        category: None,
        position: point,
        altitude_baro_ft: None,
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "test".into(),
    };

    let _ = engine.ingest_feed_snapshot(&test_snapshot(FeedItems::Aircraft(vec![ac])));
    let entity = engine.resolve("icao24", "112233").unwrap();

    // Link a custom alias
    let custom_id = EntityIdentifier::new("tail_number", "N11223");
    let updated = engine.link_alias(&entity.id, &custom_id).unwrap();
    assert!(updated.identifiers.iter().any(|i| i.scheme == "tail_number" && i.value == "N11223"));

    // Can resolve by custom alias
    let resolved = engine.resolve("tail_number", "N11223").unwrap();
    assert_eq!(resolved.id, entity.id);

    // Split alias off
    let split_entity = engine.split_alias(&entity.id, &custom_id).unwrap();
    assert_ne!(split_entity.id, entity.id);
    assert_eq!(split_entity.identifiers, vec![custom_id.clone()]);

    // Now tail_number resolves to the new entity!
    let resolved_split = engine.resolve("tail_number", "N11223").unwrap();
    assert_eq!(resolved_split.id, split_entity.id);

    // And the original entity no longer has tail_number
    let remaining_aliases = engine.get_entity_aliases(&entity.id).unwrap();
    assert!(!remaining_aliases.iter().any(|i| i.scheme == "tail_number"));

    // Splitting when only 1 identifier remains returns BadRequest
    let split_fail = engine.split_alias(&split_entity.id, &custom_id);
    assert!(split_fail.is_err());
}

#[test]
fn manual_merging_entities() {
    let engine = ResolutionEngine::new(ResolutionConfig::default());

    let point = GeoPoint::new(51.5, -0.1).unwrap();
    let ac1 = Aircraft {
        hex: "aaaaaa".into(),
        callsign: Some("CALL1".into()),
        registration: None,
        type_code: Some("C172".into()),
        category: None,
        position: point,
        altitude_baro_ft: None,
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "test".into(),
    };
    let ac2 = Aircraft {
        hex: "bbbbbb".into(),
        callsign: Some("CALL2".into()),
        registration: None,
        type_code: None,
        category: None,
        position: point,
        altitude_baro_ft: None,
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "test".into(),
    };

    let _ = engine.ingest_feed_snapshot(&test_snapshot(FeedItems::Aircraft(vec![ac1, ac2])));

    let ent1 = engine.resolve("icao24", "aaaaaa").unwrap();
    let ent2 = engine.resolve("icao24", "bbbbbb").unwrap();

    let merged = engine.merge_entities(&ent1.id, &ent2.id).unwrap();
    assert_eq!(merged.id, ent1.id);

    // Old source entity is no longer individually present under its ID
    assert!(engine.get_entity(&ent2.id).is_none());

    // Both aliases now resolve to merged entity!
    assert_eq!(engine.resolve("icao24", "aaaaaa").unwrap().id, ent1.id);
    assert_eq!(engine.resolve("icao24", "bbbbbb").unwrap().id, ent1.id);
    assert_eq!(engine.resolve("callsign", "CALL2").unwrap().id, ent1.id);
}

#[test]
fn bounded_lru_eviction() {
    let config = ResolutionConfig { max_entities: 3, max_links: 10 };
    let engine = ResolutionEngine::new(config);

    let make_ac = |hex: &str| Aircraft {
        hex: hex.into(),
        callsign: Some(format!("CALL_{hex}")),
        registration: None,
        type_code: None,
        category: None,
        position: GeoPoint::new(0.0, 0.0).unwrap(),
        altitude_baro_ft: None,
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: None,
        track_deg: None,
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "test".into(),
    };

    // Insert 3 entities (e1, e2, e3)
    let _ = engine.ingest_feed_snapshot(&test_snapshot(FeedItems::Aircraft(vec![
        make_ac("111111"),
        make_ac("222222"),
        make_ac("333333"),
    ])));
    assert_eq!(engine.entity_count(), 3);

    // Touch e1 so e2 becomes the least recently used
    let _ = engine.resolve("icao24", "111111");

    // Insert e4 -> should evict e2
    let _ =
        engine.ingest_feed_snapshot(&test_snapshot(FeedItems::Aircraft(vec![make_ac("444444")])));
    assert_eq!(engine.entity_count(), 3);

    // e1, e3, e4 should be present; e2 should be evicted
    assert!(engine.resolve("icao24", "111111").is_some());
    assert!(engine.resolve("icao24", "333333").is_some());
    assert!(engine.resolve("icao24", "444444").is_some());
    assert!(engine.resolve("icao24", "222222").is_none());
    assert!(engine.resolve("callsign", "CALL_222222").is_none());
}
