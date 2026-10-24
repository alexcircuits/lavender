#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::assert_is_empty)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;

#[tokio::test]
async fn http_secrets_status_and_rejection() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. GET /api/v1/secrets/status reports managedBy: operator and status boolean map without values
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/v1/secrets/status")
                .header(header::HOST, "localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["managedBy"], "operator");
    assert!(json["status"].is_object());
    assert_eq!(json["status"]["firms"], false);

    // 2. POST /api/v1/secrets is rejected with 403 Forbidden
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/secrets")
                .header(header::HOST, "localhost:7410")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"key":"firms","value":"my-secret-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
