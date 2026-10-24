#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::assert_is_empty)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;

async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(uri).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn http_regions_at_point_in_polygon() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // London: lon -0.1276, lat 51.5074
    let (status, body) = get_json(&app, "/api/v1/regions/at?lat=51.5074&lon=-0.1276").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["name"] == "Great Britain" || body["name"] == "British Isles",
        "Expected Great Britain or British Isles, got {}",
        body["name"]
    );
    assert_eq!(body["isoA2"], "GB");

    // Tokyo: lon 139.6917, lat 35.6895
    let (status, body) = get_json(&app, "/api/v1/regions/at?lat=35.6895&lon=139.6917").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["name"] == "Honshü" || body["name"] == "Japan",
        "Expected Honshu or Japan, got {}",
        body["name"]
    );

    // New York: lon -73.9, lat 40.72
    let (status, body) = get_json(&app, "/api/v1/regions/at?lat=40.72&lon=-73.9").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["name"] == "Long I." || body["name"] == "North America",
        "Expected Long I. or North America, got {}",
        body["name"]
    );

    // Invalid parameters -> 400
    let (status, body) = get_json(&app, "/api/v1/regions/at?lat=120&lon=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");

    let (status, body) = get_json(&app, "/api/v1/regions/at?lat=abc&lon=0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");
}

#[tokio::test]
async fn http_regions_search() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    let (status, body) = get_json(&app, "/api/v1/regions/search?q=Great%20Britain").await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(!arr.is_empty());
    assert_eq!(arr[0]["name"], "Great Britain");
    assert_eq!(arr[0]["isoA2"], "GB");

    let (status, body) = get_json(&app, "/api/v1/regions/search?q=Japan").await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr.iter().any(|r| r["name"] == "Japan"));

    let (status, body) = get_json(&app, "/api/v1/regions/search?q=UnknownPlaceXYZ").await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.is_empty());
}

#[tokio::test]
async fn http_infrastructure_viewport_and_thinning() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // Zoom < 6: only major facilities
    let (status, body) = get_json(
        &app,
        "/api/v1/infrastructure?west=-180&south=-90&east=180&north=90&zoom=4&limit=200",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let low_zoom = body.as_array().unwrap();
    assert!(!low_zoom.is_empty());

    // Zoom >= 10: more facilities returned
    let (status, body) = get_json(
        &app,
        "/api/v1/infrastructure?west=-180&south=-90&east=180&north=90&zoom=10&limit=200",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let high_zoom = body.as_array().unwrap();
    assert!(!high_zoom.is_empty());

    // Limit capping
    let (status, body) = get_json(
        &app,
        "/api/v1/infrastructure?west=-180&south=-90&east=180&north=90&zoom=10&limit=5",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 5);

    // Invalid bbox (south > north) -> 400
    let (status, body) =
        get_json(&app, "/api/v1/infrastructure?west=0&south=50&east=10&north=40&zoom=10").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");
}
