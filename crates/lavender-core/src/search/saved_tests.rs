//! Unit tests for persistent saved searches: CRUD, matching, bounded capacity, alerting, and virtual feed snapshot.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreadable_literal)]

use std::sync::Arc;

use jiff::Timestamp;
use lavender_domain::{
    Aircraft, AlertKind, Coverage, Earthquake, FeedId, FeedItems, FeedSnapshot, Freshness,
    GeoPoint, SavedSearch, Vessel,
};

use super::*;
use crate::alerting::GeofenceAlertEngine;

fn ts() -> Timestamp {
    "2026-10-25T12:00:00Z".parse().unwrap()
}

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

fn sample_aircraft(hex: &str, callsign: Option<&str>, type_code: Option<&str>) -> Aircraft {
    Aircraft {
        hex: hex.to_string(),
        callsign: callsign.map(ToString::to_string),
        registration: None,
        type_code: type_code.map(ToString::to_string),
        category: None,
        position: point(52.5, 13.4),
        altitude_baro_ft: Some(30_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(450.0),
        track_deg: Some(90.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".into(),
    }
}

fn sample_vessel(mmsi: &str, name: Option<&str>, vtype: Option<&str>) -> Vessel {
    Vessel {
        mmsi: mmsi.to_string(),
        name: name.map(ToString::to_string),
        imo: None,
        vessel_type: vtype.map(ToString::to_string),
        destination: None,
        position: point(54.0, 10.0),
        speed_knots: Some(12.5),
        course_deg: Some(180.0),
        heading_deg: Some(180.0),
        last_position_utc: Some(ts().to_string()),
        last_position_epoch: None,
    }
}

fn sample_earthquake(id: &str, place: &str, mag: f64) -> Earthquake {
    Earthquake {
        id: id.to_string(),
        magnitude: mag,
        magnitude_type: Some("ml".into()),
        depth_km: 10.0,
        position: point(37.7, -122.4),
        place: Some(place.to_string()),
        time: ts(),
        updated: None,
        url: None,
        tsunami: false,
        alert: None,
        felt: None,
        significance: None,
        status: None,
    }
}

fn make_snapshot(feed: FeedId, items: FeedItems) -> FeedSnapshot {
    FeedSnapshot {
        feed,
        sources: vec!["test-source".into()],
        fetched_at: Some(ts()),
        generated_at: ts(),
        freshness: Freshness::Live,
        coverage: Coverage::Complete,
        source_count: u32::try_from(items.len()).unwrap_or(u32::MAX),
        pending_regions: 0,
        items,
        notices: Vec::new(),
    }
}

#[test]
fn crud_saved_search_lifecycle() {
    let engine = SavedSearchEngine::new(None, 10, 50);

    let search1 = SavedSearch {
        id: "search-1".into(),
        name: "Boeing 737s".into(),
        query: "B738".into(),
        feed_filter: Some(vec![FeedId::Aircraft]),
        created_at: ts(),
        notify_on_match: true,
    };

    // Create
    let created = engine.create_or_update(search1.clone()).unwrap();
    assert_eq!(created.id, "search-1");
    assert_eq!(engine.search_count(), 1);

    // Get
    let fetched = engine.get("search-1").unwrap();
    assert_eq!(fetched.name, "Boeing 737s");
    assert_eq!(fetched.query, "B738");

    // List
    let list = engine.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "search-1");

    // Update
    let mut updated = search1;
    updated.name = "Boeing 738 Fleet".into();
    engine.create_or_update(updated).unwrap();
    assert_eq!(engine.get("search-1").unwrap().name, "Boeing 738 Fleet");
    assert_eq!(engine.search_count(), 1);

    // Delete
    assert!(engine.delete("search-1"));
    assert!(!engine.delete("search-1"));
    assert_eq!(engine.search_count(), 0);
    assert!(engine.get("search-1").is_none());
}

#[test]
fn validation_and_capacity_limits() {
    let engine = SavedSearchEngine::new(None, 2, 10);

    // Empty ID
    let err = engine
        .create_or_update(SavedSearch {
            id: String::new(),
            name: "Test".into(),
            query: "Q".into(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap_err();
    assert!(matches!(err, CoreError::BadRequest(_)));

    // Empty name
    let err = engine
        .create_or_update(SavedSearch {
            id: "id1".into(),
            name: "   ".into(),
            query: "Q".into(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap_err();
    assert!(matches!(err, CoreError::BadRequest(_)));

    // Empty query
    let err = engine
        .create_or_update(SavedSearch {
            id: "id1".into(),
            name: "Test".into(),
            query: String::new(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap_err();
    assert!(matches!(err, CoreError::BadRequest(_)));

    // Capacity limit of 2 searches
    engine
        .create_or_update(SavedSearch {
            id: "s1".into(),
            name: "S1".into(),
            query: "Q1".into(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap();

    engine
        .create_or_update(SavedSearch {
            id: "s2".into(),
            name: "S2".into(),
            query: "Q2".into(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap();

    let err = engine
        .create_or_update(SavedSearch {
            id: "s3".into(),
            name: "S3".into(),
            query: "Q3".into(),
            feed_filter: None,
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap_err();
    assert!(matches!(err, CoreError::Limit(_)));
}

#[test]
fn feed_snapshot_ingestion_and_matching() {
    let alert_engine = Arc::new(GeofenceAlertEngine::new(10, 50, 100));
    let engine = SavedSearchEngine::new(Some(alert_engine.clone()), 10, 50);

    engine
        .create_or_update(SavedSearch {
            id: "search-ryr".into(),
            name: "Ryanair Flights".into(),
            query: "RYR".into(),
            feed_filter: Some(vec![FeedId::Aircraft]),
            created_at: ts(),
            notify_on_match: true,
        })
        .unwrap();

    engine
        .create_or_update(SavedSearch {
            id: "search-tanker".into(),
            name: "Tanker Vessels".into(),
            query: "Tanker".into(),
            feed_filter: Some(vec![FeedId::Vessels]),
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap();

    // Ingest aircraft snapshot
    let planes = vec![
        sample_aircraft("4ca123", Some("RYR12AB"), Some("B738")),
        sample_aircraft("3c6789", Some("DLH456"), Some("A320")),
        sample_aircraft("4ca999", Some("RYR99CD"), Some("B738")),
    ];
    let plane_snap = make_snapshot(FeedId::Aircraft, FeedItems::Aircraft(planes));
    let matches = engine.ingest_snapshot(&plane_snap);

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].search_id, "search-ryr");
    assert_eq!(matches[0].feed, FeedId::Aircraft);

    // Matches should be stored in recent matches
    let ryr_matches = engine.get_matches("search-ryr", None).unwrap();
    assert_eq!(ryr_matches.len(), 2);

    // Alert engine inbox should contain 2 events because notify_on_match was true
    let inbox = alert_engine.inbox(None);
    assert_eq!(inbox.len(), 2);
    assert_eq!(inbox[0].kind, AlertKind::SearchMatch);
    assert_eq!(inbox[0].rule_id, "search-ryr");

    // Re-ingesting same aircraft should not duplicate matches or alert events
    let matches2 = engine.ingest_snapshot(&plane_snap);
    assert_eq!(matches2.len(), 0);
    assert_eq!(alert_engine.inbox(None).len(), 2);

    // Ingest vessel snapshot
    let vessels = vec![
        sample_vessel("211000111", Some("NORDIC TANKER"), Some("Oil Tanker")),
        sample_vessel("211000222", Some("CARGO EXPRESS"), Some("General Cargo")),
    ];
    let vessel_snap = make_snapshot(FeedId::Vessels, FeedItems::Vessels(vessels));
    let v_matches = engine.ingest_snapshot(&vessel_snap);

    assert_eq!(v_matches.len(), 1);
    assert_eq!(v_matches[0].search_id, "search-tanker");
    assert_eq!(v_matches[0].entity_id, "211000111");

    // Tanker search had notify_on_match = false, so alert inbox remains at 2
    assert_eq!(alert_engine.inbox(None).len(), 2);
}

#[test]
fn virtual_feed_snapshot_generation() {
    let engine = SavedSearchEngine::new(None, 10, 50);

    engine
        .create_or_update(SavedSearch {
            id: "search-b738".into(),
            name: "738s Only".into(),
            query: "B738".into(),
            feed_filter: Some(vec![FeedId::Aircraft]),
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap();

    // Before snapshot ingestion: returns empty snapshot with correct feed
    let initial_feed = engine.get_feed_snapshot("search-b738").unwrap();
    assert_eq!(initial_feed.feed, FeedId::Aircraft);
    assert!(initial_feed.items.is_empty());
    assert_eq!(initial_feed.source_count, 0);

    // After snapshot ingestion: virtual snapshot contains only matching B738 items
    let planes = vec![
        sample_aircraft("4ca123", Some("RYR12AB"), Some("B738")),
        sample_aircraft("3c6789", Some("DLH456"), Some("A320")),
        sample_aircraft("4ca999", Some("RYR99CD"), Some("B738")),
        sample_aircraft("112233", Some("BAW11"), Some("B772")),
    ];
    let plane_snap = make_snapshot(FeedId::Aircraft, FeedItems::Aircraft(planes));
    engine.ingest_snapshot(&plane_snap);

    let virtual_feed = engine.get_feed_snapshot("search-b738").unwrap();
    assert_eq!(virtual_feed.feed, FeedId::Aircraft);
    assert_eq!(virtual_feed.source_count, 2);
    if let FeedItems::Aircraft(items) = virtual_feed.items {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].hex, "4ca123");
        assert_eq!(items[1].hex, "4ca999");
    } else {
        panic!("Expected Aircraft feed items");
    }
}

#[test]
fn match_tracking_bounded_capacity() {
    // Engine bounded to max 3 matches per search
    let engine = SavedSearchEngine::new(None, 5, 3);

    engine
        .create_or_update(SavedSearch {
            id: "quake-search".into(),
            name: "Quakes".into(),
            query: "California".into(),
            feed_filter: Some(vec![FeedId::Earthquakes]),
            created_at: ts(),
            notify_on_match: false,
        })
        .unwrap();

    let quakes = vec![
        sample_earthquake("q1", "Northern California", 3.2),
        sample_earthquake("q2", "Central California", 4.1),
        sample_earthquake("q3", "Southern California", 2.8),
        sample_earthquake("q4", "Offshore Northern California", 5.0),
    ];

    let snap = make_snapshot(FeedId::Earthquakes, FeedItems::Earthquakes(quakes));
    let matches = engine.ingest_snapshot(&snap);
    assert_eq!(matches.len(), 4);

    // Recent matches should be bounded by max_matches (3)
    let stored = engine.get_matches("quake-search", None).unwrap();
    assert_eq!(stored.len(), 3);
    // Most recent is at front
    assert_eq!(stored[0].entity_id, "q4");
    assert_eq!(stored[1].entity_id, "q3");
    assert_eq!(stored[2].entity_id, "q2");
}
