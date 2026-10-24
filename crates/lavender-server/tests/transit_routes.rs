#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreadable_literal)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_core::providers::transit::proto::{
    FeedEntity, FeedHeader, FeedMessage, Position, TripDescriptor, VehicleDescriptor,
    VehiclePosition,
};
use lavender_domain::{ApiError, GeoPoint, TransitFeed, TransitTrail, TransitVehicle};
use lavender_server::config::ServerSettings;
use prost::Message;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn get_response(app: &axum::Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::get(uri).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes)
}

fn encode_gtfs_rt_sample() -> Vec<u8> {
    let now_s = u64::try_from(jiff::Timestamp::now().as_second()).unwrap_or(0);
    let message = FeedMessage {
        header: Some(FeedHeader {
            gtfs_realtime_version: Some("2.0".to_string()),
            incrementality: Some(0),
            timestamp: Some(now_s),
        }),
        entity: vec![
            FeedEntity {
                id: "ent-1".to_string(),
                is_deleted: Some(false),
                vehicle: Some(VehiclePosition {
                    trip: Some(TripDescriptor {
                        trip_id: Some("trip-101".to_string()),
                        route_id: Some("1".to_string()),
                        direction_id: Some(0),
                        start_time: None,
                        start_date: None,
                        schedule_relationship: None,
                    }),
                    position: Some(Position {
                        latitude: 42.355,
                        longitude: -71.065,
                        bearing: Some(90.0),
                        odometer: None,
                        speed: Some(10.0),
                    }),
                    current_stop_sequence: Some(4),
                    current_status: Some(2), // IN_TRANSIT_TO
                    timestamp: Some(now_s),
                    congestion_level: None,
                    stop_id: Some("stop-a".to_string()),
                    vehicle: Some(VehicleDescriptor {
                        id: Some("bus-101".to_string()),
                        label: Some("101".to_string()),
                        license_plate: None,
                    }),
                    occupancy_status: Some(1), // MANY_SEATS_AVAILABLE
                }),
            },
            FeedEntity {
                id: "ent-2".to_string(),
                is_deleted: Some(false),
                vehicle: Some(VehiclePosition {
                    trip: Some(TripDescriptor {
                        trip_id: Some("trip-202".to_string()),
                        route_id: Some("2".to_string()),
                        direction_id: Some(1),
                        start_time: None,
                        start_date: None,
                        schedule_relationship: None,
                    }),
                    position: Some(Position {
                        latitude: 40.7128,
                        longitude: -74.0060,
                        bearing: Some(270.0),
                        odometer: None,
                        speed: Some(15.0),
                    }),
                    current_stop_sequence: Some(2),
                    current_status: Some(1), // STOPPED_AT
                    timestamp: Some(now_s),
                    congestion_level: None,
                    stop_id: Some("stop-b".to_string()),
                    vehicle: Some(VehicleDescriptor {
                        id: Some("bus-202".to_string()),
                        label: Some("202".to_string()),
                        license_plate: None,
                    }),
                    occupancy_status: Some(2), // FEW_SEATS_AVAILABLE
                }),
            },
        ],
    };

    let mut buf = Vec::new();
    message.encode(&mut buf).unwrap();
    buf
}

#[tokio::test]
async fn http_transit_feeds_catalog() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) = get_response(&app, "/api/v1/transit/feeds").await;
    assert_eq!(status, StatusCode::OK);
    let catalog: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let feeds = catalog["feeds"].as_array().unwrap();
    assert!(feeds.len() >= 7);
    assert!(feeds.iter().any(|f| f["id"] == "mbta"));
    assert!(feeds.iter().any(|f| f["id"] == "capmetro-austin"));
    assert!(feeds.iter().any(|f| f["id"] == "hsl-helsinki"));
    assert!(feeds.iter().any(|f| f["id"] == "entur-norway"));

    // Legacy route alias check
    let (legacy_status, _legacy_headers, legacy_body) =
        get_response(&app, "/api/transit/feeds").await;
    assert_eq!(legacy_status, StatusCode::OK);
    let legacy_catalog: serde_json::Value = serde_json::from_slice(&legacy_body).unwrap();
    assert_eq!(legacy_catalog["feeds"].as_array().unwrap().len(), feeds.len());
}

#[tokio::test]
async fn http_transit_decode_recorded_feed_and_bbox() {
    let mock_server = MockServer::start().await;
    let pb_bytes = encode_gtfs_rt_sample();

    Mock::given(method("GET"))
        .and(path("/realtime/vehicles.pb"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(pb_bytes)
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    let test_feed = TransitFeed {
        id: "test-agency".to_string(),
        name: "Test Agency".to_string(),
        operator: "Test Operator".to_string(),
        region: "Test Region".to_string(),
        center: GeoPoint::new(42.3601, -71.0589).unwrap(),
        load_radius_km: 70.0,
        url: format!("{}/realtime/vehicles.pb", mock_server.uri()),
        license: "Open Data".to_string(),
        attribution: "Test Agency".to_string(),
        default_enabled: true,
        default_mode: "bus".to_string(),
    };
    config.transit.feeds = vec![test_feed];
    config.fetch.allow_http_hosts.push("127.0.0.1".into());

    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Fetch all vehicles without bbox
    let (status, _headers, body) = get_response(&app, "/api/v1/transit/test-agency/vehicles").await;
    assert_eq!(status, StatusCode::OK);
    let vehicles: Vec<TransitVehicle> = serde_json::from_slice(&body).unwrap();
    assert_eq!(vehicles.len(), 2);
    assert!(vehicles.iter().any(|v| v.vehicle_id == "bus-101"));
    assert!(vehicles.iter().any(|v| v.vehicle_id == "bus-202"));

    // 2. Filter by bbox around Boston (holds bus-101 at 42.355, -71.065, excludes bus-202 at NYC)
    let (status_bbox, _headers, body_bbox) =
        get_response(&app, "/api/v1/transit/test-agency/vehicles?bbox=-72.0,42.0,-70.0,43.0").await;
    assert_eq!(status_bbox, StatusCode::OK);
    let filtered: Vec<TransitVehicle> = serde_json::from_slice(&body_bbox).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].vehicle_id, "bus-101");

    // 3. Query vehicle trail after ingestion
    let (status_trail, _headers, body_trail) =
        get_response(&app, "/api/v1/transit/test-agency/bus-101/trail").await;
    assert_eq!(status_trail, StatusCode::OK);
    let trail: TransitTrail = serde_json::from_slice(&body_trail).unwrap();
    assert_eq!(trail.version, 1);
    assert_eq!(trail.feed_id, "test-agency");
    assert_eq!(trail.vehicle_id, "bus-101");
    assert_eq!(trail.fixes.len(), 1);
    assert_eq!(trail.epochs.len(), 1);
    assert_eq!(trail.epochs[0].route, "1");

    // Legacy trail route check
    let (legacy_trail_status, _headers, legacy_trail_body) =
        get_response(&app, "/api/transit/trail/test-agency/bus-101").await;
    assert_eq!(legacy_trail_status, StatusCode::OK);
    let legacy_trail: TransitTrail = serde_json::from_slice(&legacy_trail_body).unwrap();
    assert_eq!(legacy_trail.vehicle_id, "bus-101");
}

#[tokio::test]
async fn http_transit_unknown_feed_returns_404() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) = get_response(&app, "/api/v1/transit/ghost-feed/vehicles").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert!(err.message.contains("ghost-feed"));
}

#[tokio::test]
async fn http_transit_invalid_identifier_returns_400() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // Path traversal / invalid identifier attempts
    let (status, _headers, body) =
        get_response(&app, "/api/v1/transit/mbta/..%2F..%2Fetc/trail").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert!(err.message.contains("invalid transit vehicle identifier"));
}

#[tokio::test]
async fn http_transit_redirect_following() {
    let mock_server = MockServer::start().await;
    let pb_bytes = encode_gtfs_rt_sample();

    // 1. Initial URL redirects to /files/feed.pb on the SAME server
    Mock::given(method("GET"))
        .and(path("/start.pb"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/files/feed.pb"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/files/feed.pb"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(pb_bytes)
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    let test_feed = TransitFeed {
        id: "redirect-feed".to_string(),
        name: "Redirect Feed".to_string(),
        operator: "Agency".to_string(),
        region: "Region".to_string(),
        center: GeoPoint::new(42.0, -71.0).unwrap(),
        load_radius_km: 50.0,
        url: format!("{}/start.pb", mock_server.uri()),
        license: "Open".to_string(),
        attribution: "Agency".to_string(),
        default_enabled: true,
        default_mode: "bus".to_string(),
    };
    config.transit.feeds = vec![test_feed];
    config.fetch.allow_http_hosts.push("127.0.0.1".into());

    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) =
        get_response(&app, "/api/v1/transit/redirect-feed/vehicles").await;
    assert_eq!(status, StatusCode::OK);
    let vehicles: Vec<TransitVehicle> = serde_json::from_slice(&body).unwrap();
    assert_eq!(vehicles.len(), 2);
}
