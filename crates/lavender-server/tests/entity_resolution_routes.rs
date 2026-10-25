#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::too_many_lines
)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_domain::{
    Aircraft, CanonicalEntity, Coverage, EntityIdentifier, EntityKind, FeedId, FeedItems,
    FeedSnapshot, Freshness, GeoPoint, SatelliteElements, Vessel,
};
use lavender_server::config::ServerSettings;

async fn get_response(app: &axum::Router, uri: &str) -> (StatusCode, Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::get(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

fn test_snapshot(feed: FeedId, items: FeedItems) -> FeedSnapshot {
    FeedSnapshot {
        feed,
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

#[tokio::test]
async fn test_entity_resolution_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();

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

    let vessel = Vessel {
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

    let settings = ServerSettings::default();
    let app = lavender_server::app(core.clone(), &settings);

    // 1. Before ingest: resolution returns 404
    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=icao24&value=4ca1b2").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err["code"], "notFound");

    // 2. Ingest feeds
    let _ =
        core.ingest_feed_snapshot(&test_snapshot(FeedId::Aircraft, FeedItems::Aircraft(vec![ac])));
    let _ = core
        .ingest_feed_snapshot(&test_snapshot(FeedId::Vessels, FeedItems::Vessels(vec![vessel])));
    let _ = core
        .ingest_feed_snapshot(&test_snapshot(FeedId::Satellites, FeedItems::Satellites(vec![sat])));

    // 3. Resolve Aircraft by icao24
    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=icao24&value=4ca1b2").await;
    assert_eq!(status, StatusCode::OK);
    let entity: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entity.id, "canonical:aircraft:4ca1b2");
    assert_eq!(entity.kind, EntityKind::Aircraft);
    assert_eq!(entity.primary_label, "RYR12AB");

    // 4. Resolve Aircraft by callsign
    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=callsign&value=RYR12AB").await;
    assert_eq!(status, StatusCode::OK);
    let entity_by_cs: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entity_by_cs.id, entity.id);

    // 5. GET /api/v1/entities/{id}
    let (status, bytes) = get_response(&app, &format!("/api/v1/entities/{}", entity.id)).await;
    assert_eq!(status, StatusCode::OK);
    let fetched: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fetched.id, entity.id);
    assert_eq!(fetched.kind, EntityKind::Aircraft);

    // 6. GET /api/v1/entities/{id}/aliases
    let (status, bytes) =
        get_response(&app, &format!("/api/v1/entities/{}/aliases", entity.id)).await;
    assert_eq!(status, StatusCode::OK);
    let aliases: Vec<EntityIdentifier> = serde_json::from_slice(&bytes).unwrap();
    assert!(aliases.iter().any(|a| a.scheme == "icao24" && a.value == "4ca1b2"));
    assert!(aliases.iter().any(|a| a.scheme == "callsign" && a.value == "RYR12AB"));
    assert!(aliases.iter().any(|a| a.scheme == "registration" && a.value == "EI-ABC"));

    // 7. Resolve Vessel by MMSI and IMO
    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=mmsi&value=230123456").await;
    assert_eq!(status, StatusCode::OK);
    let ship: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(ship.kind, EntityKind::Vessel);
    assert_eq!(ship.primary_label, "FINNSTAR");

    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=imo&value=9319440").await;
    assert_eq!(status, StatusCode::OK);
    let ship_by_imo: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(ship_by_imo.id, ship.id);

    // 8. Resolve Satellite by norad_cat_id
    let (status, bytes) =
        get_response(&app, "/api/v1/entities/resolve?scheme=norad_cat_id&value=25544").await;
    assert_eq!(status, StatusCode::OK);
    let satellite: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(satellite.kind, EntityKind::Satellite);
    assert_eq!(satellite.primary_label, "ISS (ZARYA)");

    // 9. Error cases
    // Missing scheme or value
    let (status, bytes) = get_response(&app, "/api/v1/entities/resolve?scheme=&value=123").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err["code"], "invalidRequest");

    let (status, bytes) = get_response(&app, "/api/v1/entities/resolve").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err["code"], "invalidRequest");

    // Not found entity ID
    let (status, bytes) = get_response(&app, "/api/v1/entities/nonexistent-id").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err["code"], "notFound");

    // Not found entity aliases
    let (status, bytes) = get_response(&app, "/api/v1/entities/nonexistent-id/aliases").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err["code"], "notFound");

    // 10. Outer route `/api/entities/...`
    let (status, bytes) =
        get_response(&app, "/api/entities/resolve?scheme=icao24&value=4ca1b2").await;
    assert_eq!(status, StatusCode::OK);
    let entity_outer: CanonicalEntity = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(entity_outer.id, entity.id);
}
