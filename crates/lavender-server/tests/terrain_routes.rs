#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreadable_literal)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::{CoreConfig, TerrainConfig};
use lavender_domain::{ApiError, TerrainHeightsResponse, TerrainPointResult};
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn post_json(
    app: &axum::Router,
    uri: &str,
    body_val: &serde_json::Value,
) -> (StatusCode, Bytes) {
    let body_bytes = serde_json::to_vec(body_val).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

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

fn build_test_app(core: LavenderCore) -> axum::Router {
    lavender_server::app(core, &ServerSettings::default())
}

#[tokio::test]
async fn http_terrain_heights_order_duplicates_and_cache() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/heights.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                { "id": "p-30", "ellipsoid": 300.0, "elevation": 270.0, "geoid": 30.0 },
                { "id": "p-20", "ellipsoid": 200.0, "elevation": 180.0, "geoid": 20.0 },
                { "id": "p-10", "ellipsoid": 100.0, "elevation": 90.0, "geoid": 10.0 }
            ]
        })))
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.terrain = TerrainConfig {
        enabled: true,
        upstream_url: format!("{}/heights.json", server.uri()),
        max_points: 100,
        chunk_size: 64,
        cache_capacity: 1000,
        cache_ttl_days: 30,
    };

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // Request with duplicates and specific order: [30, 20, 10, 30, 20]
    let request_body = serde_json::json!({
        "points": [
            [30.0, 3.0],
            [20.0, 2.0],
            [10.0, 1.0],
            [30.0, 3.0],
            [20.0, 2.0]
        ]
    });

    let (status, bytes) = post_json(&app, "/api/v1/terrain/heights", &request_body).await;
    assert_eq!(status, StatusCode::OK);

    let resp: TerrainHeightsResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp.results.len(), 5);
    assert_eq!(resp.results[0].ellipsoid, 300.0);
    assert_eq!(resp.results[1].ellipsoid, 200.0);
    assert_eq!(resp.results[2].ellipsoid, 100.0);
    assert_eq!(resp.results[3].ellipsoid, 300.0);
    assert_eq!(resp.results[4].ellipsoid, 200.0);

    // Legacy route check
    let (status_leg, bytes_leg) = post_json(&app, "/api/terrain/heights", &request_body).await;
    assert_eq!(status_leg, StatusCode::OK);
    let resp_leg: TerrainHeightsResponse = serde_json::from_slice(&bytes_leg).unwrap();
    assert_eq!(resp_leg.results.len(), 5);
}

#[tokio::test]
async fn http_terrain_heights_validation_errors() {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.terrain.max_points = 5;

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // 1. Empty points
    let (status, bytes) =
        post_json(&app, "/api/v1/terrain/heights", &serde_json::json!({ "points": [] })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&bytes).unwrap();
    assert!(err.message.contains("empty points array"));

    // 2. Too many points (> max_points of 5)
    let too_many = serde_json::json!({
        "points": [
            [0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0], [5.0, 5.0]
        ]
    });
    let (status, bytes) = post_json(&app, "/api/v1/terrain/heights", &too_many).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&bytes).unwrap();
    assert!(err.message.contains("too many points"));

    // 3. Out-of-range longitude (> 180)
    let bad_lon = serde_json::json!({
        "points": [[185.0, 45.0]]
    });
    let (status, bytes) = post_json(&app, "/api/v1/terrain/heights", &bad_lon).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&bytes).unwrap();
    assert!(err.message.contains("longitude"));

    // 4. Out-of-range latitude (> 90)
    let bad_lat = serde_json::json!({
        "points": [[0.0, 95.0]]
    });
    let (status, bytes) = post_json(&app, "/api/v1/terrain/heights", &bad_lat).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&bytes).unwrap();
    assert!(err.message.contains("latitude"));
}

#[tokio::test]
async fn http_terrain_heights_cache_bounded() {
    let server = MockServer::start().await;

    // Upstream answers any point with a dummy height
    Mock::given(method("GET"))
        .and(path("/heights.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                { "ellipsoid": 50.0, "elevation": 40.0, "geoid": 10.0 }
            ]
        })))
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.terrain = TerrainConfig {
        enabled: true,
        upstream_url: format!("{}/heights.json", server.uri()),
        max_points: 100,
        chunk_size: 1,
        cache_capacity: 4, // Cap at 4 entries
        cache_ttl_days: 30,
    };

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core.clone());

    // Query 8 distinct coordinates sequentially
    for i in 0..8 {
        let (status, _) = post_json(
            &app,
            "/api/v1/terrain/heights",
            &serde_json::json!({ "points": [[f64::from(i), f64::from(i)]] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Cache must stay within the bound of 4!
    assert!(core.terrain_engine().cache().len() <= 4);
    assert_eq!(core.terrain_engine().cache().len(), 4);
}

#[tokio::test]
async fn http_terrain_heights_query_param_compat() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/heights.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                { "ellipsoid": 111.0, "elevation": 100.0, "geoid": 11.0 },
                { "ellipsoid": 222.0, "elevation": 200.0, "geoid": 22.0 }
            ]
        })))
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.terrain.upstream_url = format!("{}/heights.json", server.uri());

    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // GET with semicolon-separated points query param
    let (status, bytes) =
        get_response(&app, "/api/terrain/heights?points=-97.7431,30.2672;139.6917,35.6895").await;
    assert_eq!(status, StatusCode::OK);

    let resp: TerrainHeightsResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp.results.len(), 2);
    assert_eq!(resp.results[0].ellipsoid, 111.0);
    assert_eq!(resp.results[1].ellipsoid, 222.0);

    // Missing query param returns 400
    let (status_err, _) = get_response(&app, "/api/terrain/heights").await;
    assert_eq!(status_err, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_terrain_heights_stale_fallback_and_502_when_missing() {
    let server = MockServer::start().await;

    // Upstream fails with 503
    Mock::given(method("GET"))
        .and(path("/heights.json"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into(), "localhost".into()];
    config.terrain.upstream_url = format!("{}/heights.json", server.uri());

    let core = LavenderCore::builder(config).build().unwrap();

    // Insert a stale entry for [15.0, 25.0]
    let key = lavender_core::providers::terrain::canonical_key([15.0, 25.0]);
    core.terrain_engine().cache().insert(
        key,
        TerrainPointResult {
            id: Some("stale-p".into()),
            ellipsoid: 42.0,
            elevation: None,
            geoid: None,
        },
        1_000, // very old -> stale
    );

    let app = build_test_app(core);

    // 1. Point present in stale cache -> serves stale 200
    let (status, bytes) = post_json(
        &app,
        "/api/v1/terrain/heights",
        &serde_json::json!({ "points": [[15.0, 25.0]] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: TerrainHeightsResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp.results[0].ellipsoid, 42.0);

    // 2. Point NOT in cache and upstream failing -> 502 Bad Gateway
    let (status_fail, bytes_fail) = post_json(
        &app,
        "/api/v1/terrain/heights",
        &serde_json::json!({ "points": [[50.0, 60.0]] }),
    )
    .await;
    assert_eq!(status_fail, StatusCode::BAD_GATEWAY);
    let err: ApiError = serde_json::from_slice(&bytes_fail).unwrap();
    assert!(err.message.contains("terrain heights fetch failed"));
}

#[tokio::test]
async fn http_terrain_heights_cross_site_request_refused() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let app = build_test_app(core);

    // Cross-site request with foreign Origin header
    let response = app
        .oneshot(
            Request::post("/api/v1/terrain/heights")
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "https://malicious.example.com")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"points":[[10.0, 20.0]]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
