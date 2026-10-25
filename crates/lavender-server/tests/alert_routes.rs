#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_domain::{ApiError, ErrorCode, GeofenceRule};
use lavender_server::config::ServerSettings;

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

async fn post_json(app: &axum::Router, uri: &str, body: serde_json::Value) -> (StatusCode, Bytes) {
    let bytes = serde_json::to_vec(&body).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let resp_bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, resp_bytes)
}

async fn delete_request(app: &axum::Router, uri: &str) -> (StatusCode, Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::delete(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let resp_bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, resp_bytes)
}

fn build_test_app(core: LavenderCore) -> axum::Router {
    lavender_server::app(core, &ServerSettings::default())
}

#[tokio::test]
async fn alert_routes_disabled_by_default_returns_503() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let app = build_test_app(core);

    // POST /api/v1/alerts/rules
    let (status1, body1) = post_json(
        &app,
        "/api/v1/alerts/rules",
        json!({
            "id": "rule-1",
            "name": "Zone",
            "polygon": [{"lat": 10.0, "lon": 20.0}, {"lat": 11.0, "lon": 20.0}, {"lat": 11.0, "lon": 21.0}]
        }),
    )
    .await;
    assert_eq!(status1, StatusCode::SERVICE_UNAVAILABLE);
    let err1: ApiError = serde_json::from_slice(&body1).unwrap();
    assert_eq!(err1.code, ErrorCode::Unconfigured);

    // GET /api/v1/alerts/inbox
    let (status2, body2) = get_response(&app, "/api/v1/alerts/inbox").await;
    assert_eq!(status2, StatusCode::SERVICE_UNAVAILABLE);
    let err2: ApiError = serde_json::from_slice(&body2).unwrap();
    assert_eq!(err2.code, ErrorCode::Unconfigured);

    // DELETE /api/v1/alerts/rules/test
    let (status3, body3) = delete_request(&app, "/api/v1/alerts/rules/test").await;
    assert_eq!(status3, StatusCode::SERVICE_UNAVAILABLE);
    let err3: ApiError = serde_json::from_slice(&body3).unwrap();
    assert_eq!(err3.code, ErrorCode::Unconfigured);
}

#[tokio::test]
async fn alert_rules_crud_and_inbox_routes() {
    let mut config = CoreConfig::default();
    config.alerting.enabled = true;
    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // 1. Create a rule
    let payload = json!({
        "id": "baltic-guard",
        "name": "Baltic Air Defense Zone",
        "geofences": [
            {
                "id": "helsinki",
                "name": "Helsinki TMA",
                "polygon": [
                    {"lat": 60.0, "lon": 24.0},
                    {"lat": 61.0, "lon": 24.0},
                    {"lat": 61.0, "lon": 26.0},
                    {"lat": 60.0, "lon": 26.0}
                ]
            }
        ]
    });
    let (create_status, create_body) = post_json(&app, "/api/v1/alerts/rules", payload).await;
    assert_eq!(create_status, StatusCode::OK);
    let created: GeofenceRule = serde_json::from_slice(&create_body).unwrap();
    assert_eq!(created.id, "baltic-guard");
    assert_eq!(created.geofences.len(), 1);

    // 2. List rules
    let (list_status, list_body) = get_response(&app, "/api/v1/alerts/rules").await;
    assert_eq!(list_status, StatusCode::OK);
    let rules: Vec<GeofenceRule> = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, "baltic-guard");

    // 3. Get single rule
    let (get_status, get_body) = get_response(&app, "/api/v1/alerts/rules/baltic-guard").await;
    assert_eq!(get_status, StatusCode::OK);
    let fetched: GeofenceRule = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(fetched.name, "Baltic Air Defense Zone");

    // 4. Fetch inbox (empty initially)
    let (inbox_status, inbox_body) = get_response(&app, "/api/v1/alerts/inbox").await;
    assert_eq!(inbox_status, StatusCode::OK);
    let inbox: Vec<lavender_domain::AlertEvent> = serde_json::from_slice(&inbox_body).unwrap();
    assert!(inbox.is_empty());

    // 5. Delete rule
    let (del_status, del_body) = delete_request(&app, "/api/v1/alerts/rules/baltic-guard").await;
    assert_eq!(del_status, StatusCode::OK);
    let del_resp: serde_json::Value = serde_json::from_slice(&del_body).unwrap();
    assert_eq!(del_resp["deleted"], true);

    // 6. Delete again -> 404 Not Found
    let (del_again_status, del_again_body) =
        delete_request(&app, "/api/v1/alerts/rules/baltic-guard").await;
    assert_eq!(del_again_status, StatusCode::NOT_FOUND);
    let err: ApiError = serde_json::from_slice(&del_again_body).unwrap();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn alert_rules_validation_errors() {
    let mut config = CoreConfig::default();
    config.alerting.enabled = true;
    let core = LavenderCore::builder(config).build().unwrap();
    let app = build_test_app(core);

    // Empty ID
    let (status1, body1) = post_json(
        &app,
        "/api/v1/alerts/rules",
        json!({
            "id": "",
            "name": "Invalid Rule",
            "polygon": [{"lat": 10.0, "lon": 10.0}, {"lat": 11.0, "lon": 10.0}, {"lat": 11.0, "lon": 11.0}]
        }),
    )
    .await;
    assert_eq!(status1, StatusCode::BAD_REQUEST);
    let err1: ApiError = serde_json::from_slice(&body1).unwrap();
    assert_eq!(err1.code, ErrorCode::InvalidRequest);

    // Insufficient vertices (< 3)
    let (status2, body2) = post_json(
        &app,
        "/api/v1/alerts/rules",
        json!({
            "id": "too-few-vertices",
            "name": "Invalid Triangle",
            "polygon": [{"lat": 10.0, "lon": 10.0}, {"lat": 11.0, "lon": 10.0}]
        }),
    )
    .await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    let err2: ApiError = serde_json::from_slice(&body2).unwrap();
    assert_eq!(err2.code, ErrorCode::InvalidRequest);
}
