#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
async fn http_weather_manifest_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Radar manifest
    let (status, body) = get_json(&app, "/api/v1/weather/manifest?product=radar").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["product"], "radar");
    assert_eq!(body["tilingScheme"], "geographic");
    assert_eq!(body["tileSize"], 256);
    assert_eq!(body["source"], "NOAA nowCOAST");

    // 2. Lightning manifest
    let (status, body) = get_json(&app, "/api/v1/weather/manifest?product=lightning").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["product"], "lightning");

    // 3. Legacy path
    let (status, body) = get_json(&app, "/api/weather/manifest?product=clouds").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["product"], "clouds");
}
