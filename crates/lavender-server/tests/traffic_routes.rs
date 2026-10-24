#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use lavender_core::LavenderCore;
use lavender_core::config::{CoreConfig, TrafficConfig};
use lavender_core::secrets::{MemorySecretStore, SecretKey};
use lavender_domain::{ApiError, ErrorCode, TrafficStatus, TrafficTileResponse};
use lavender_server::config::ServerSettings;

const TOMTOM_FIXTURE_PBF: &[u8] =
    include_bytes!("../../../legacy/src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf");
const OFM_FIXTURE_PBF: &[u8] =
    include_bytes!("../../../legacy/src/data/fixtures/ofm-austin-12-935-1686.pbf");

async fn get_response(app: &axum::Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Bytes) {
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
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes)
}

fn build_test_app(core: LavenderCore) -> axum::Router {
    lavender_server::app(core, &ServerSettings::default())
}

#[tokio::test]
async fn http_traffic_status_keyless_and_keyed() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 1. Keyless mode
    let mut config = CoreConfig::default();
    config.traffic.cache_dir = temp_dir.path().to_path_buf();
    config.traffic.daily_budget = 6000;

    let core_keyless = LavenderCore::builder(config.clone()).build().unwrap();
    let app_keyless = build_test_app(core_keyless);

    let (status, _, body) = get_response(&app_keyless, "/api/v1/traffic/status").await;
    assert_eq!(status, StatusCode::OK);
    let st: TrafficStatus = serde_json::from_slice(&body).unwrap();
    assert!(!st.has_key);
    assert_eq!(st.daily_count, 0);
    assert_eq!(st.budget, 6000);
    assert!(!st.date.is_empty());

    // Also check legacy alias /api/tomtom/status
    let (status_alias, _, body_alias) = get_response(&app_keyless, "/api/tomtom/status").await;
    assert_eq!(status_alias, StatusCode::OK);
    let st_alias: TrafficStatus = serde_json::from_slice(&body_alias).unwrap();
    assert_eq!(st, st_alias);

    // 2. Keyed mode
    let secrets =
        Arc::new(MemorySecretStore::with([(SecretKey::TomTomApiKey, "test-secret-tomtom-key")]));

    let core_keyed = LavenderCore::builder(config).secrets(secrets).build().unwrap();
    let app_keyed = build_test_app(core_keyed);

    let (status2, _, body2) = get_response(&app_keyed, "/api/v1/traffic/status").await;
    assert_eq!(status2, StatusCode::OK);
    let st2: TrafficStatus = serde_json::from_slice(&body2).unwrap();
    assert!(st2.has_key);
    assert_eq!(st2.budget, 6000);
}

#[tokio::test]
async fn http_traffic_missing_key_returns_503() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = CoreConfig::default();
    config.traffic.cache_dir = temp_dir.path().to_path_buf();

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // Flow PBF endpoint
    let (status1, _, body1) = get_response(&app, "/api/tomtom/flow/12/935/1686.pbf").await;
    assert_eq!(status1, StatusCode::SERVICE_UNAVAILABLE);
    let err1: ApiError = serde_json::from_slice(&body1).unwrap();
    assert_eq!(err1.code, ErrorCode::Unconfigured);

    // Matched roads tile endpoint
    let (status2, _, body2) = get_response(&app, "/api/v1/tiles/traffic/12/935/1686").await;
    assert_eq!(status2, StatusCode::SERVICE_UNAVAILABLE);
    let err2: ApiError = serde_json::from_slice(&body2).unwrap();
    assert_eq!(err2.code, ErrorCode::Unconfigured);
}

#[tokio::test]
async fn http_traffic_validation_invalid_tile_coordinates() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = CoreConfig::default();
    config.traffic.cache_dir = temp_dir.path().to_path_buf();
    let secrets = Arc::new(MemorySecretStore::with([(SecretKey::TomTomApiKey, "valid-key")]));

    let core = LavenderCore::builder(config).secrets(secrets).build().unwrap();
    let app = build_test_app(core);

    // Zoom out of range (< 8 or > 16)
    let (status, _, body) = get_response(&app, "/api/v1/tiles/traffic/5/935/1686").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);

    // Non-numeric row
    let (status2, _, body2) = get_response(&app, "/api/v1/tiles/traffic/12/935/abc").await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    let err2: ApiError = serde_json::from_slice(&body2).unwrap();
    assert_eq!(err2.code, ErrorCode::InvalidRequest);
}

#[tokio::test]
async fn http_traffic_cross_site_request_refused() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = CoreConfig::default();
    config.traffic.cache_dir = temp_dir.path().to_path_buf();

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    let cross_origin_req = Request::get("/api/tomtom/status")
        .header(header::HOST, "localhost:7410")
        .header(header::ORIGIN, "https://malicious-site.example.com")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(cross_origin_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn http_traffic_budget_survives_restarts_and_governs() {
    let mock_upstream = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/traffic/12/935/1686.pbf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(TOMTOM_FIXTURE_PBF.to_vec())
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_upstream)
        .await;

    Mock::given(method("GET"))
        .and(path("/ofm/12/935/1686.pbf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(OFM_FIXTURE_PBF.to_vec())
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_upstream)
        .await;

    let temp_dir = tempfile::tempdir().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();

    let secrets = Arc::new(MemorySecretStore::with([(SecretKey::TomTomApiKey, "test-key")]));

    // Phase 1: Set budget limit to 2 requests
    {
        let mut config = CoreConfig::default();
        config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
        config.traffic = TrafficConfig {
            enabled: true,
            daily_budget: 2,
            cache_dir: cache_dir.clone(),
            max_disk_bytes: 50 * 1024 * 1024,
            tomtom_base_url: format!("{}/traffic", mock_upstream.uri()),
            openfreemap_tile_url: format!("{}/ofm", mock_upstream.uri()),
        };

        let core = LavenderCore::builder(config).secrets(secrets.clone()).build().unwrap();
        let app = build_test_app(core);

        // First request: uses 1 budget slot
        let (st1, headers1, body1) = get_response(&app, "/api/tomtom/flow/12/935/1686.pbf").await;
        assert_eq!(st1, StatusCode::OK);
        assert_eq!(headers1.get("x-tomtom-cache").unwrap(), "MISS");
        assert_eq!(body1.len(), TOMTOM_FIXTURE_PBF.len());

        // Check status: dailyCount should be 1
        let (_, _, status_body) = get_response(&app, "/api/v1/traffic/status").await;
        let st: TrafficStatus = serde_json::from_slice(&status_body).unwrap();
        assert_eq!(st.daily_count, 1);
    }

    // Phase 2: Restart! Re-create core and app with the same disk directory
    {
        let mut config = CoreConfig::default();
        config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
        config.traffic = TrafficConfig {
            enabled: true,
            daily_budget: 2,
            cache_dir: cache_dir.clone(),
            max_disk_bytes: 50 * 1024 * 1024,
            tomtom_base_url: format!("{}/traffic", mock_upstream.uri()),
            openfreemap_tile_url: format!("{}/ofm", mock_upstream.uri()),
        };

        let core = LavenderCore::builder(config).secrets(secrets.clone()).build().unwrap();
        let app = build_test_app(core);

        // Status after restart: dailyCount MUST be 1 (survived restart)
        let (_, _, status_body) = get_response(&app, "/api/v1/traffic/status").await;
        let st: TrafficStatus = serde_json::from_slice(&status_body).unwrap();
        assert_eq!(st.daily_count, 1, "budget count must survive restarts");

        // Fresh request from disk cache: counts as HIT (does not increase budget count)
        let (st_hit, headers_hit, _) = get_response(&app, "/api/tomtom/flow/12/935/1686.pbf").await;
        assert_eq!(st_hit, StatusCode::OK);
        assert_eq!(headers_hit.get("x-tomtom-cache").unwrap(), "HIT");

        // Status still 1 because cache hits are free
        let (_, _, status_body2) = get_response(&app, "/api/v1/traffic/status").await;
        let st2: TrafficStatus = serde_json::from_slice(&status_body2).unwrap();
        assert_eq!(st2.daily_count, 1);
    }
}

#[tokio::test]
async fn http_traffic_tile_matching_with_fixtures() {
    let mock_upstream = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/traffic/12/935/1686.pbf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(TOMTOM_FIXTURE_PBF.to_vec())
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_upstream)
        .await;

    Mock::given(method("GET"))
        .and(path("/ofm/12/935/1686.pbf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(OFM_FIXTURE_PBF.to_vec())
                .insert_header("content-type", "application/x-protobuf"),
        )
        .mount(&mock_upstream)
        .await;

    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.traffic = TrafficConfig {
        enabled: true,
        daily_budget: 6000,
        cache_dir: temp_dir.path().to_path_buf(),
        max_disk_bytes: 50 * 1024 * 1024,
        tomtom_base_url: format!("{}/traffic", mock_upstream.uri()),
        openfreemap_tile_url: format!("{}/ofm", mock_upstream.uri()),
    };

    let secrets = Arc::new(MemorySecretStore::with([(SecretKey::TomTomApiKey, "test-key")]));

    let core = LavenderCore::builder(config).secrets(secrets).build().unwrap();
    let app = build_test_app(core);

    let (status, _, body) = get_response(&app, "/api/v1/tiles/traffic/12/935/1686").await;
    assert_eq!(status, StatusCode::OK);

    let tile: TrafficTileResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(tile.z, 12);
    assert_eq!(tile.x, 935);
    assert_eq!(tile.y, 1686);
    assert!(!tile.roads.is_empty(), "matched roads must not be empty on recorded Austin fixture");

    for road in &tile.roads {
        assert!(road.coords.len() >= 2);
        assert!(road.traffic_level >= 0.0 && road.traffic_level <= 1.0);
    }
}
