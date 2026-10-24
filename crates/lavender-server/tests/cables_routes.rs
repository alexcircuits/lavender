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
async fn http_cables_viewport_and_detail_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // Query Atlantic cables
    let (status, body) =
        get_json(&app, "/api/v1/infrastructure/cables?west=-60&south=10&east=-10&north=60").await;
    assert_eq!(status, StatusCode::OK);
    let cables = body.as_array().expect("expected array of cables");
    assert!(!cables.is_empty(), "expected some cables in Atlantic");

    // Query single cable
    let (status, body) = get_json(&app, "/api/v1/infrastructure/cables/kunoa-north").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "kunoa-north");
    assert_eq!(body["name"], "Kunoa North");

    // Non-existent cable -> 404
    let (status, _) =
        get_json(&app, "/api/v1/infrastructure/cables/non-existent-cable-id-12345").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Query landings
    let (status, body) = get_json(
        &app,
        "/api/v1/infrastructure/landings?west=-180&south=-90&east=180&north=90&zoom=10",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let landings = body.as_array().expect("expected array of landings");
    assert!(!landings.is_empty(), "expected some landing points");
}
