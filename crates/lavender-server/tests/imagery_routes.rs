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
async fn http_imagery_routes_validation() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let settings = ServerSettings { request_timeout_s: 60, ..ServerSettings::default() };
    let app = lavender_server::app(core, &settings);

    // 1. Invalid bbox format
    let (status, body) = get_json(&app, "/api/v1/imagery/recent?bbox=invalid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");

    // 2. Dateline crossing
    let (status, body) = get_json(&app, "/api/v1/imagery/recent?bbox=179,0,-179,1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");
    assert!(body["message"].as_str().unwrap().contains("dateline"));

    // 3. Oversized box
    let (status, body) = get_json(&app, "/api/v1/imagery/recent?bbox=0,0,0.1,10").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");
    assert!(body["message"].as_str().unwrap().contains("limit 1,000 km"));
}

#[tokio::test]
async fn http_imagery_routes_success_and_descriptors() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let settings = ServerSettings { request_timeout_s: 60, ..ServerSettings::default() };
    let app = lavender_server::app(core, &settings);

    // 1. Search recent imagery on a valid bbox (Austin, TX)
    let (status, body) =
        get_json(&app, "/api/v1/imagery/recent?bbox=-97.8,30.2,-97.7,30.3&days=5").await;
    assert_eq!(status, StatusCode::OK);

    // Attribution
    assert_eq!(body["attribution"]["credit"], "NASA EOSDIS / GIBS / LP DAAC");
    assert_eq!(body["attribution"]["licence"], "Public Domain / NASA Open Data");
    assert!(
        body["attribution"]["statement"]
            .as_str()
            .unwrap()
            .contains("NASA Worldview Snapshots and GIBS")
    );

    // Tile templates
    assert!(
        body["tileTemplates"]["S30"]
            .as_str()
            .unwrap()
            .contains("HLS_S30_Nadir_BRDF_Adjusted_Reflectance")
    );
    assert!(
        body["tileTemplates"]["L30"]
            .as_str()
            .unwrap()
            .contains("HLS_L30_Nadir_BRDF_Adjusted_Reflectance")
    );
    assert!(
        body["tileTemplates"]["VIIRS"]
            .as_str()
            .unwrap()
            .contains("VIIRS_NOAA21_CorrectedReflectance_TrueColor")
    );

    // Candidates list contains the 5 synthesized VIIRS overview days
    let candidates = body["candidates"].as_array().unwrap();
    assert!(candidates.len() >= 5);
    assert!(candidates.iter().any(|c| c["product"] == "VIIRS" && c["coverage"] == "full"));

    // Ranked latest candidate
    assert!(!body["rankedLatest"]["candidate"].is_null());

    // 2. Legacy alias route
    let (status_alias, body_alias) =
        get_json(&app, "/api/imagery/recent?bbox=-97.8,30.2,-97.7,30.3&days=3").await;
    assert_eq!(status_alias, StatusCode::OK);
    assert_eq!(body_alias["attribution"]["credit"], "NASA EOSDIS / GIBS / LP DAAC");
}
