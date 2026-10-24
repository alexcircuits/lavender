#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::assert_is_empty)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_domain::BoundaryResponse;
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
async fn http_boundaries_viewport_and_class_filter() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Query world boundaries
    let (status, body) =
        get_json(&app, "/api/v1/boundaries?west=-180&south=-85&east=180&north=85&limit=20").await;
    assert_eq!(status, StatusCode::OK);
    let resp: BoundaryResponse = serde_json::from_value(body.clone()).unwrap();
    assert!(!resp.features.is_empty());
    assert!(resp.attribution.contains("Natural Earth"));

    // 2. Query only disputed lines
    let (status, body) = get_json(
        &app,
        "/api/v1/boundaries?west=-180&south=-85&east=180&north=85&classes=disputedLine&limit=50",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: BoundaryResponse = serde_json::from_value(body.clone()).unwrap();
    assert!(!resp.features.is_empty());
    for f in &resp.features {
        assert_eq!(f.class, lavender_domain::BoundaryClass::DisputedLine);
    }

    // 3. Query only breakaway areas
    let (status, body) = get_json(
        &app,
        "/api/v1/boundaries?west=-180&south=-85&east=180&north=85&classes=breakawayArea&limit=50",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let resp: BoundaryResponse = serde_json::from_value(body.clone()).unwrap();
    assert!(!resp.features.is_empty());
    for f in &resp.features {
        assert_eq!(f.class, lavender_domain::BoundaryClass::BreakawayArea);
    }

    // 4. Query single boundary feature by ID
    let first_id = &resp.features[0].id;
    let (status, body) = get_json(&app, &format!("/api/v1/boundaries/{first_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], *first_id);

    // 5. Query 404 on missing boundary
    let (status, _) = get_json(&app, "/api/v1/boundaries/unknown-9999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 6. Test alias route `/api/boundaries`
    let (status, body) =
        get_json(&app, "/api/boundaries?west=-10&south=35&east=40&north=60&limit=5").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["total"].as_u64().unwrap_or(0) > 0);
}
