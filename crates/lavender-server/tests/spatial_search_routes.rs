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

use lavender_core::{CoreConfig, LavenderCore};
use lavender_domain::{ApiError, ErrorCode, SpatialSearchResponse};
use lavender_server::config::ServerSettings;

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
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

#[tokio::test]
async fn http_spatial_search_endpoints() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Circle search
    let circle_body = json!({
        "filter": {
            "type": "circle",
            "center": { "lat": 60.1699, "lon": 24.9384 },
            "radiusM": 25000.0
        },
        "limit": 20
    });
    let (status, body) = post_json(&app, "/api/v1/search/spatial", circle_body).await;
    assert_eq!(status, StatusCode::OK);
    let resp: SpatialSearchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.total_matched, 0); // No live providers running in bare test
    assert!(!resp.truncated);

    // 2. BBox search
    let bbox_body = json!({
        "filter": {
            "type": "bbox",
            "west": 24.0,
            "south": 59.0,
            "east": 26.0,
            "north": 61.0
        }
    });
    let (status, body) = post_json(&app, "/api/v1/search/spatial", bbox_body).await;
    assert_eq!(status, StatusCode::OK);
    let resp: SpatialSearchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.total_matched, 0);

    // 3. Polygon search
    let poly_body = json!({
        "filter": {
            "type": "polygon",
            "points": [
                { "lat": 60.0, "lon": 24.0 },
                { "lat": 61.0, "lon": 24.0 },
                { "lat": 61.0, "lon": 26.0 },
                { "lat": 60.0, "lon": 26.0 }
            ]
        }
    });
    let (status, body) = post_json(&app, "/api/v1/search/spatial", poly_body).await;
    assert_eq!(status, StatusCode::OK);
    let resp: SpatialSearchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.total_matched, 0);

    // 4. Corridor search
    let corridor_body = json!({
        "filter": {
            "type": "corridor",
            "waypoints": [
                { "lat": 60.0, "lon": 24.0 },
                { "lat": 60.5, "lon": 25.0 }
            ],
            "bufferM": 10000.0
        }
    });
    let (status, body) = post_json(&app, "/api/v1/search/spatial", corridor_body).await;
    assert_eq!(status, StatusCode::OK);
    let resp: SpatialSearchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.total_matched, 0);

    // 5. Legacy alias `/api/search/spatial`
    let (legacy_status, _) = post_json(
        &app,
        "/api/search/spatial",
        json!({
            "filter": {
                "type": "circle",
                "center": { "lat": 0.0, "lon": 0.0 },
                "radiusM": 1000.0
            }
        }),
    )
    .await;
    assert_eq!(legacy_status, StatusCode::OK);

    // 6. Validation error: polygon with fewer than 3 points
    let bad_poly_body = json!({
        "filter": {
            "type": "polygon",
            "points": [
                { "lat": 60.0, "lon": 24.0 },
                { "lat": 61.0, "lon": 24.0 }
            ]
        }
    });
    let (bad_status, bad_body) = post_json(&app, "/api/v1/search/spatial", bad_poly_body).await;
    assert_eq!(bad_status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&bad_body).unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);

    // 7. Validation error: corridor with negative buffer
    let bad_corridor = json!({
        "filter": {
            "type": "corridor",
            "waypoints": [
                { "lat": 60.0, "lon": 24.0 },
                { "lat": 61.0, "lon": 25.0 }
            ],
            "bufferM": -50.0
        }
    });
    let (bad_corridor_status, _) = post_json(&app, "/api/v1/search/spatial", bad_corridor).await;
    assert_eq!(bad_corridor_status, StatusCode::BAD_REQUEST);
}
