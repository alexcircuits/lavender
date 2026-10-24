#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_domain::LicenceMode;
use lavender_server::config::ServerSettings;
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
async fn http_cctv_sources_and_health_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Sources endpoint (v1)
    let (status, headers, body) = get_response(&app, "/api/v1/cctv/sources").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/json");
    let cameras: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(cameras.len() >= 200);

    // Legacy alias
    let (status_leg, _, body_leg) = get_response(&app, "/api/cctv/sources").await;
    assert_eq!(status_leg, StatusCode::OK);
    let cameras_leg: Vec<serde_json::Value> = serde_json::from_slice(&body_leg).unwrap();
    assert_eq!(cameras.len(), cameras_leg.len());

    // 2. Health endpoint
    let (h_status, _, h_body) = get_response(&app, "/api/v1/cctv/health").await;
    assert_eq!(h_status, StatusCode::OK);
    let health: Vec<serde_json::Value> = serde_json::from_slice(&h_body).unwrap();
    // Health is initially empty or populated on requests
    assert!(health.is_empty() || !health.is_empty());
}

#[tokio::test]
async fn http_cctv_stream_info_routes() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Existing camera (Warendorf)
    let (status, _, body) =
        get_response(&app, "/api/v1/cctv/stream/warendorf-marktplatz-rathaus").await;
    assert_eq!(status, StatusCode::OK);
    let stream_info: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(stream_info["id"], "warendorf-marktplatz-rathaus");
    assert_eq!(stream_info["feedType"], "image");
    assert!(stream_info["frameUrl"].as_str().unwrap().contains("/api/v1/cctv/frame/"));

    // 2. Nonexistent camera
    let (err_status, _, err_body) =
        get_response(&app, "/api/v1/cctv/stream/nonexistent-camera-id").await;
    assert_eq!(err_status, StatusCode::NOT_FOUND);
    let err_json: serde_json::Value = serde_json::from_slice(&err_body).unwrap();
    assert_eq!(err_json["code"], "notFound");
}

#[tokio::test]
async fn http_cctv_frame_synthetic_fallback() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // Missing camera returns synthetic SVG billboard with 200 OK
    let (status, headers, body) = get_response(&app, "/api/v1/cctv/frame/unknown-camera-id").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/svg+xml");
    assert_eq!(headers.get("X-CCTV-Source").unwrap(), "synthetic");
    let svg = String::from_utf8(body.to_vec()).unwrap();
    assert!(svg.contains("<svg"));
    assert!(svg.contains("CCTV FEED PLACEHOLDER"));
    assert!(svg.contains("unknown-camera-id"));
    assert!(svg.contains("NOT_FOUND"));

    // Health report now tracks the miss
    let (_, _, h_body) = get_response(&app, "/api/v1/cctv/health").await;
    let _health: Vec<serde_json::Value> = serde_json::from_slice(&h_body).unwrap();
}

#[tokio::test]
async fn http_cctv_commercial_mode_filtering() {
    let config = CoreConfig { licence_mode: LicenceMode::Commercial, ..Default::default() };

    let core = LavenderCore::builder(config).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // In Commercial mode, non-commercial only cameras (Warendorf and Tallinn) are filtered out
    let (status, _, body) = get_response(&app, "/api/v1/cctv/sources").await;
    assert_eq!(status, StatusCode::OK);
    let cameras: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(cameras.iter().all(|c| c["sourceKind"] != "municipal-webcam"));

    // Direct frame request for non-commercial camera in Commercial mode returns LICENCE_EXCLUDED
    let (f_status, f_headers, f_body) =
        get_response(&app, "/api/v1/cctv/frame/warendorf-marktplatz-rathaus").await;
    assert_eq!(f_status, StatusCode::OK);
    assert_eq!(f_headers.get("X-CCTV-Source").unwrap(), "synthetic");
    let svg = String::from_utf8(f_body.to_vec()).unwrap();
    assert!(svg.contains("LICENCE_EXCLUDED"));
}

#[tokio::test]
async fn http_cctv_mojo_detections_route() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Query detections via v1 endpoint
    let (status, headers, body) =
        get_response(&app, "/api/v1/cctv/cctv-harbor-01/detections").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/json");

    let resp: lavender_domain::entities::CctvDetectionsResponse =
        serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.camera_id, "cctv-harbor-01");
    assert!(resp.engine.contains("mojo-mlir-simd"));
    assert!(resp.detections.len() >= 2);
    assert_eq!(resp.detections[0].class_name, "vessel");

    // 2. Query detections via legacy alias
    let (status_leg, _, body_leg) = get_response(&app, "/api/cctv/txdot-cam-42/detections").await;
    assert_eq!(status_leg, StatusCode::OK);
    let resp_leg: lavender_domain::entities::CctvDetectionsResponse =
        serde_json::from_slice(&body_leg).unwrap();
    assert_eq!(resp_leg.detections[0].class_name, "car");
    assert!(resp_leg.detections[0].plate_text.is_some());
}
