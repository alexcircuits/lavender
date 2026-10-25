#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::assert_is_empty
)]

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jiff::Timestamp;
use serde_json::json;
use tower::ServiceExt;

use lavender_core::alerting::GeofenceAlertEngine;
use lavender_core::{CoreConfig, LavenderCore};
use lavender_domain::{
    Aircraft, AlertEvent, AlertKind, ApiError, Coverage, ErrorCode, FeedId, FeedItems,
    FeedSnapshot, Freshness, GeoPoint, SavedSearch, SavedSearchMatch,
};
use lavender_server::config::ServerSettings;

fn ts() -> Timestamp {
    "2026-10-25T12:00:00Z".parse().unwrap()
}

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

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

fn sample_aircraft(hex: &str, callsign: Option<&str>, type_code: Option<&str>) -> Aircraft {
    Aircraft {
        hex: hex.to_string(),
        callsign: callsign.map(ToString::to_string),
        registration: None,
        type_code: type_code.map(ToString::to_string),
        category: None,
        position: point(52.5, 13.4),
        altitude_baro_ft: Some(30_000.0),
        altitude_geom_ft: None,
        on_ground: false,
        ground_speed_kt: Some(450.0),
        track_deg: Some(90.0),
        vertical_rate_fpm: None,
        squawk: None,
        emergency: None,
        is_military: false,
        seen_pos_s: 1.0,
        source: "adsb".into(),
    }
}

#[tokio::test]
async fn http_saved_search_routes_crud_and_validation() {
    let mut config = CoreConfig::default();
    config.alerting.enabled = true;
    let core = LavenderCore::builder(config).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Initial list is empty
    let (status, body) = get_response(&app, "/api/v1/searches/saved").await;
    assert_eq!(status, StatusCode::OK);
    let list: Vec<SavedSearch> = serde_json::from_slice(&body).unwrap();
    assert!(list.is_empty());

    // 2. Create saved search: POST /api/v1/searches/saved
    let create_payload = json!({
        "id": "search-patrol",
        "name": "Baltic Patrol",
        "query": "B738",
        "feedFilter": ["aircraft"],
        "createdAt": "2026-10-25T12:00:00Z",
        "notifyOnMatch": true
    });
    let (status, body) = post_json(&app, "/api/v1/searches/saved", create_payload).await;
    assert_eq!(status, StatusCode::CREATED);
    let created: SavedSearch = serde_json::from_slice(&body).unwrap();
    assert_eq!(created.id, "search-patrol");
    assert_eq!(created.name, "Baltic Patrol");
    assert_eq!(created.query, "B738");
    assert!(created.notify_on_match);

    // 3. Get saved search: GET /api/v1/searches/saved/{id}
    let (status, body) = get_response(&app, "/api/v1/searches/saved/search-patrol").await;
    assert_eq!(status, StatusCode::OK);
    let fetched: SavedSearch = serde_json::from_slice(&body).unwrap();
    assert_eq!(fetched.id, "search-patrol");

    // 4. Non-existent ID returns 404
    let (status, body) = get_response(&app, "/api/v1/searches/saved/non-existent").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, ErrorCode::NotFound);

    // 5. Validation error: empty query returns 400
    let invalid_payload = json!({
        "id": "search-invalid",
        "name": "Invalid",
        "query": "   ",
        "createdAt": "2026-10-25T12:00:00Z",
        "notifyOnMatch": false
    });
    let (status, body) = post_json(&app, "/api/v1/searches/saved", invalid_payload).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);

    // 6. Delete saved search: DELETE /api/v1/searches/saved/{id} -> 204
    let (status, _) = delete_request(&app, "/api/v1/searches/saved/search-patrol").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 7. Verify deletion -> 404
    let (status, _) = get_response(&app, "/api/v1/searches/saved/search-patrol").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 8. Delete unknown -> 404
    let (status, body) = delete_request(&app, "/api/v1/searches/saved/search-patrol").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn http_saved_search_feed_and_matches_with_alerts() {
    let alert_engine = Arc::new(GeofenceAlertEngine::new(10, 50, 100));
    let mut config = CoreConfig::default();
    config.alerting.enabled = true;
    let core = LavenderCore::builder(config).alert_engine(alert_engine.clone()).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core.clone(), &settings);

    // Create saved search for Ryanair Boeing 738 aircraft
    let create_payload = json!({
        "id": "search-ryr-b738",
        "name": "Ryanair B738",
        "query": "RYR",
        "feedFilter": ["aircraft"],
        "createdAt": "2026-10-25T12:00:00Z",
        "notifyOnMatch": true
    });
    let (status, _) = post_json(&app, "/api/v1/searches/saved", create_payload).await;
    assert_eq!(status, StatusCode::CREATED);

    // Feed before any data -> empty virtual FeedSnapshot
    let (status, body) = get_response(&app, "/api/v1/searches/saved/search-ryr-b738/feed").await;
    assert_eq!(status, StatusCode::OK);
    let initial_feed: FeedSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(initial_feed.feed, FeedId::Aircraft);
    assert!(initial_feed.items.is_empty());

    // Matches before any data -> empty list
    let (status, body) = get_response(&app, "/api/v1/searches/saved/search-ryr-b738/matches").await;
    assert_eq!(status, StatusCode::OK);
    let initial_matches: Vec<SavedSearchMatch> = serde_json::from_slice(&body).unwrap();
    assert!(initial_matches.is_empty());

    // Ingest snapshot into core
    let planes = vec![
        sample_aircraft("4ca123", Some("RYR12AB"), Some("B738")),
        sample_aircraft("3c6789", Some("DLH456"), Some("A320")),
        sample_aircraft("4ca456", Some("RYR99CD"), Some("B738")),
    ];
    let snapshot = FeedSnapshot {
        feed: FeedId::Aircraft,
        sources: vec!["adsb-test".into()],
        fetched_at: Some(ts()),
        generated_at: ts(),
        freshness: Freshness::Live,
        coverage: Coverage::Complete,
        source_count: 3,
        pending_regions: 0,
        items: FeedItems::Aircraft(planes),
        notices: Vec::new(),
    };
    let new_matches = core.ingest_saved_search_snapshot(&snapshot);
    assert_eq!(new_matches.len(), 2);

    // Verify virtual FeedSnapshot: GET /api/v1/searches/saved/{id}/feed
    let (status, body) = get_response(&app, "/api/v1/searches/saved/search-ryr-b738/feed").await;
    assert_eq!(status, StatusCode::OK);
    let feed: FeedSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(feed.feed, FeedId::Aircraft);
    assert_eq!(feed.source_count, 2);
    if let FeedItems::Aircraft(items) = feed.items {
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].hex, "4ca123");
        assert_eq!(items[1].hex, "4ca456");
    } else {
        panic!("Expected aircraft items in virtual feed");
    }

    // Verify recent matches: GET /api/v1/searches/saved/{id}/matches
    let (status, body) = get_response(&app, "/api/v1/searches/saved/search-ryr-b738/matches").await;
    assert_eq!(status, StatusCode::OK);
    let matches: Vec<SavedSearchMatch> = serde_json::from_slice(&body).unwrap();
    assert_eq!(matches.len(), 2);

    // Verify matches limit query param: GET /api/v1/searches/saved/{id}/matches?limit=1
    let (status, body) =
        get_response(&app, "/api/v1/searches/saved/search-ryr-b738/matches?limit=1").await;
    assert_eq!(status, StatusCode::OK);
    let matches_limit: Vec<SavedSearchMatch> = serde_json::from_slice(&body).unwrap();
    assert_eq!(matches_limit.len(), 1);

    // Verify alerts inbox received the match event: GET /api/v1/alerts/inbox
    let (status, body) = get_response(&app, "/api/v1/alerts/inbox").await;
    assert_eq!(status, StatusCode::OK);
    let alerts: Vec<AlertEvent> = serde_json::from_slice(&body).unwrap();
    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0].kind, AlertKind::SearchMatch);
    assert_eq!(alerts[0].rule_id, "search-ryr-b738");
}
