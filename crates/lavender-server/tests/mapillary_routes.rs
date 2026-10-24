#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_core::secrets::{MemorySecretStore, SecretKey};
use lavender_server::config::ServerSettings;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

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
async fn http_mapillary_status_unconfigured() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    let (status, headers, body) = get_response(&app, "/api/v1/mapillary/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["configured"], false);

    // Also check legacy alias
    let (status_leg, _, body_leg) = get_response(&app, "/api/mapillary/status").await;
    assert_eq!(status_leg, StatusCode::OK);
    let json_leg: serde_json::Value = serde_json::from_slice(&body_leg).unwrap();
    assert_eq!(json_leg["configured"], false);
}

#[tokio::test]
async fn http_mapillary_status_configured() {
    let secrets = Arc::new(MemorySecretStore::with([(
        SecretKey::MapillaryClientToken,
        "MLY|secret-token-12345",
    )]));
    let core = LavenderCore::builder(CoreConfig::default())
        .secrets(secrets)
        .providers(Vec::new())
        .build()
        .unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    let (status, _, body) = get_response(&app, "/api/v1/mapillary/status").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["configured"], true);

    // CRITICAL: Ensure the secret token value is NEVER in the body or anywhere in response
    let raw_resp = String::from_utf8_lossy(&body);
    assert!(!raw_resp.contains("secret-token-12345"));
    assert!(!raw_resp.contains("MLY|"));
}

#[tokio::test]
async fn http_mapillary_tile_validation_and_no_key() {
    // 1. Without token: 503 Service Unavailable with keyRequired
    let core_no_key =
        LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app_no_key = lavender_server::app(core_no_key, &ServerSettings::default());

    let (status, _, body) = get_response(&app_no_key, "/api/v1/tiles/mapillary/14/1/2").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "no_key");
    assert_eq!(json["keyRequired"], true);

    // Legacy alias also returns 503
    let (status_leg, _, body_leg) =
        get_response(&app_no_key, "/api/mapillary/tiles/coverage/14/1/2").await;
    assert_eq!(status_leg, StatusCode::SERVICE_UNAVAILABLE);
    let json_leg: serde_json::Value = serde_json::from_slice(&body_leg).unwrap();
    assert_eq!(json_leg["error"], "no_key");

    // 2. With token: bad zooms or out-of-range coordinates return 400 Bad Request
    let secrets =
        Arc::new(MemorySecretStore::with([(SecretKey::MapillaryClientToken, "MLY|test-token")]));
    let core_with_key = LavenderCore::builder(CoreConfig::default())
        .secrets(secrets)
        .providers(Vec::new())
        .build()
        .unwrap();
    let app_with_key = lavender_server::app(core_with_key, &ServerSettings::default());

    // Zoom 6 (disallowed: must be 0–5 or 11–14)
    let (status_z6, _, body_z6) =
        get_response(&app_with_key, "/api/v1/tiles/mapillary/6/0/0").await;
    assert_eq!(status_z6, StatusCode::BAD_REQUEST);
    let json_z6: serde_json::Value = serde_json::from_slice(&body_z6).unwrap();
    assert!(json_z6["error"].as_str().unwrap().contains("0–5 or 11–14"));

    // Zoom 10 (disallowed)
    let (status_z10, _, _body_z10) =
        get_response(&app_with_key, "/api/v1/tiles/mapillary/10/0/0").await;
    assert_eq!(status_z10, StatusCode::BAD_REQUEST);

    // Zoom 1 with coordinate x=2 (out of range: max for z=1 is 1)
    let (status_oor, _, body_oor) =
        get_response(&app_with_key, "/api/v1/tiles/mapillary/1/2/0").await;
    assert_eq!(status_oor, StatusCode::BAD_REQUEST);
    let json_oor: serde_json::Value = serde_json::from_slice(&body_oor).unwrap();
    assert!(json_oor["error"].as_str().unwrap().contains("out of range"));
}

#[tokio::test]
async fn http_mapillary_disk_cache_and_sweep() {
    let dir = tempdir().unwrap();
    let tile_dir = dir.path().join("mapillary/tiles/coverage/14");
    std::fs::create_dir_all(&tile_dir).unwrap();

    // Create a mock cached tile on disk
    let mock_pbf = vec![26, 6, 10, 4, b't', b'e', b's', b't'];
    let tile_file = tile_dir.join("100-200.pbf");
    std::fs::write(&tile_file, &mock_pbf).unwrap();

    let secrets = Arc::new(MemorySecretStore::with([(
        SecretKey::MapillaryClientToken,
        "MLY|secret-held-token",
    )]));

    let core = LavenderCore::builder(CoreConfig::default())
        .secrets(secrets)
        .cache_dir(dir.path())
        .providers(Vec::new())
        .build()
        .unwrap();

    let app = lavender_server::app(core, &ServerSettings::default());

    // Fetch the tile: should hit disk cache
    let (status, headers, body) = get_response(&app, "/api/v1/tiles/mapillary/14/100/200").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/x-protobuf");
    assert_eq!(headers.get("x-gev-cache").unwrap(), "disk");
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "public, max-age=3600");
    assert_eq!(body.as_ref(), mock_pbf.as_slice());

    // Fetch again: should hit memory LRU cache
    let (status2, headers2, body2) = get_response(&app, "/api/v1/tiles/mapillary/14/100/200").await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(headers2.get("x-gev-cache").unwrap(), "memory");
    assert_eq!(body2.as_ref(), mock_pbf.as_slice());

    // CRITICAL ACCEPTANCE ITEM: Verify secret token NEVER appears anywhere in headers or body
    for (name, val) in &headers {
        assert!(!name.as_str().contains("secret-held-token"));
        assert!(!val.to_str().unwrap_or("").contains("secret-held-token"));
    }
    let body_str = String::from_utf8_lossy(&body);
    assert!(!body_str.contains("secret-held-token"));
}
