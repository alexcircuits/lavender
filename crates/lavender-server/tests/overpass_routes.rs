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

async fn post_form(
    app: &axum::Router,
    uri: &str,
    body_str: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body_str.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn http_overpass_and_military_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Status probe returns configured: false when unset
    let (status, body) = get_json(&app, "/api/v1/overpass/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["configured"], false);

    let (status, body) = get_json(&app, "/api/overpass/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["configured"], false);

    // 2. Unbounded selector query rejected by sanitiser with 400 Bad Request
    let (status, body) =
        post_form(&app, "/api/v1/overpass", "data=[out:json];node[\"name\"=\"Central Park\"];out;")
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("unbounded"));

    // 3. Valid bounded query with unconfigured upstreams returns 503 OVERPASS_NOT_CONFIGURED
    let (status, body) =
        post_form(&app, "/api/v1/overpass", "data=[out:json];node(around:500,30.27,-97.74);out;")
            .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "OVERPASS_NOT_CONFIGURED");

    // 4. Military installations query
    let (status, body) = get_json(
        &app,
        "/api/v1/military-installations?west=-98.0&south=30.0&east=-97.0&north=31.0",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["elements"].is_array());
}
