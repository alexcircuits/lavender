#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreadable_literal)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_domain::{ApiError, BikeshareStation, BikeshareSystem, GeoPoint};
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GBFS_V2_DISCOVERY: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v2/gbfs.json");
const GBFS_V2_INFO: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v2/station_information.json");
const GBFS_V2_STATUS: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v2/station_status.json");

const GBFS_V3_DISCOVERY: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v3/gbfs.json");
const GBFS_V3_INFO: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v3/station_information.json");
const GBFS_V3_STATUS: &str =
    include_str!("../../lavender-core/tests/fixtures/gbfs_v3/station_status.json");

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

#[tokio::test]
async fn http_bikeshare_systems_catalog() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) = get_response(&app, "/api/v1/bikeshare/systems").await;
    assert_eq!(status, StatusCode::OK);
    let systems: Vec<BikeshareSystem> = serde_json::from_slice(&body).unwrap();
    assert!(systems.len() >= 30);
    assert!(systems.iter().any(|s| s.id == "nyc-citibike"));
    assert!(systems.iter().any(|s| s.id == "chicago-divvy"));
    assert!(systems.iter().any(|s| s.id == "austin-capmetro"));
}

#[tokio::test]
async fn http_bikeshare_v2_recorded_data_and_bbox() {
    let mock_server = MockServer::start().await;

    // Upstream mocks
    Mock::given(method("GET"))
        .and(path("/v2/gbfs.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    GBFS_V2_DISCOVERY.replace(
                        "https://gbfs.example.com/2.3",
                        &format!("{}/v2", mock_server.uri()),
                    ),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/station_information.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(GBFS_V2_INFO)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/station_status.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(GBFS_V2_STATUS)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts.push("127.0.0.1".into());
    // Configure system to point to mock server
    config.bikeshare.systems = vec![BikeshareSystem {
        id: "test-v2-system".into(),
        name: "Test V2 System".into(),
        city: "New York, NY".into(),
        center: GeoPoint::new(40.76, -73.98).unwrap(),
        load_radius_km: 100.0,
        provider: "Citi Bike".into(),
        gbfs_url: format!("{}/v2/gbfs.json", mock_server.uri()),
    }];

    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Full system stations
    let (status, _headers, body) = get_response(&app, "/api/v1/bikeshare/test-v2-system").await;
    assert_eq!(status, StatusCode::OK);
    let stations: Vec<BikeshareStation> = serde_json::from_slice(&body).unwrap();
    assert_eq!(stations.len(), 2);

    let st1 = stations.iter().find(|s| s.station_id == "v2-st-1").unwrap();
    assert_eq!(st1.id, "test-v2-system:v2-st-1");
    assert_eq!(st1.name, "Columbus Circle");
    assert_eq!(st1.bikes_available, Some(18));
    assert_eq!(st1.docks_available, Some(17));
    assert_eq!(st1.capacity, Some(35));
    assert!(st1.is_installed);
    assert!(st1.is_renting);
    assert!(st1.is_returning);
    assert_eq!(st1.last_reported, Some(1698000030));

    let st2 = stations.iter().find(|s| s.station_id == "v2-st-2").unwrap();
    assert_eq!(st2.name, "Times Square");
    assert_eq!(st2.bikes_available, Some(0));
    assert_eq!(st2.docks_available, Some(50));
    assert!(!st2.is_renting);

    // 2. Filtered by bounding box covering only Columbus Circle (lat 40.768, lon -73.981)
    let bbox_query = "/api/v1/bikeshare/test-v2-system?bbox=-73.982,40.765,-73.980,40.770";
    let (status, _headers, body) = get_response(&app, bbox_query).await;
    assert_eq!(status, StatusCode::OK);
    let filtered: Vec<BikeshareStation> = serde_json::from_slice(&body).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].station_id, "v2-st-1");
}

#[tokio::test]
async fn http_bikeshare_v3_recorded_data() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v3/gbfs.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    GBFS_V3_DISCOVERY.replace(
                        "https://gbfs.example.com/3.0",
                        &format!("{}/v3", mock_server.uri()),
                    ),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v3/station_information.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(GBFS_V3_INFO)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v3/station_status.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(GBFS_V3_STATUS)
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts.push("127.0.0.1".into());
    config.bikeshare.systems = vec![BikeshareSystem {
        id: "test-v3-system".into(),
        name: "Test V3 System".into(),
        city: "Paris, FR".into(),
        center: GeoPoint::new(48.86, 2.29).unwrap(),
        load_radius_km: 80.0,
        provider: "Vélib'".into(),
        gbfs_url: format!("{}/v3/gbfs.json", mock_server.uri()),
    }];

    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) = get_response(&app, "/api/v1/bikeshare/test-v3-system").await;
    assert_eq!(status, StatusCode::OK);
    let stations: Vec<BikeshareStation> = serde_json::from_slice(&body).unwrap();
    assert_eq!(stations.len(), 2);

    let eiffel = stations.iter().find(|s| s.station_id == "v3-st-101").unwrap();
    assert_eq!(eiffel.name, "Eiffel Tower");
    assert_eq!(eiffel.bikes_available, Some(22));
    assert_eq!(eiffel.docks_available, Some(18));
    assert_eq!(eiffel.capacity, Some(40));
    assert!(eiffel.last_reported.is_some());
}

#[tokio::test]
async fn http_bikeshare_unknown_system_returns_404() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    let (status, _headers, body) =
        get_response(&app, "/api/v1/bikeshare/non-existent-system-id").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, lavender_domain::ErrorCode::NotFound);
    assert!(err.message.contains("unknown bike-share system"));
}

#[tokio::test]
async fn http_bikeshare_rejects_url_parameters() {
    let config = CoreConfig::default();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Test legacy route /api/gbfs?url=https://attacker.example.com/feed.json
    let (status, _headers, body) =
        get_response(&app, "/api/gbfs?url=https://attacker.example.com/feed.json").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, lavender_domain::ErrorCode::InvalidRequest);
    assert!(err.message.contains("URLs are not accepted"));

    // 2. Test legacy route /api/gbfs/<encoded-url>
    let (status, _headers, body) =
        get_response(&app, "/api/gbfs/https%3A%2F%2Fattacker.example.com%2Ffeed.json").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, lavender_domain::ErrorCode::InvalidRequest);

    // 3. Test modern route /api/v1/bikeshare/<url> injection
    let (status, _headers, body) =
        get_response(&app, "/api/v1/bikeshare/https%3A%2F%2Fattacker.example.com").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, lavender_domain::ErrorCode::InvalidRequest);
}
