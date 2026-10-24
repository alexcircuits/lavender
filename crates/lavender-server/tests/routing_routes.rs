#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreadable_literal)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::{CoreConfig, RoutingConfig};
use lavender_server::config::ServerSettings;
use serde_json::json;
use tower::ServiceExt;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
async fn http_routing_validation_errors() {
    let core = LavenderCore::builder(CoreConfig::default()).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. Missing coords
    let (status, _, body) = get_response(&app, "/api/v1/route?profile=car").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "invalidRequest");
    assert_eq!(err["message"], "need 2-12 coordinates");

    // 2. Single coordinate
    let (status, _, body) = get_response(&app, "/api/v1/route?coords=13.4,52.5").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "invalidRequest");
    assert_eq!(err["message"], "need 2-12 coordinates");

    // 3. Invalid profile
    let (status, _, body) =
        get_response(&app, "/api/v1/route?profile=rocket&coords=13.4,52.5;13.5,52.6").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "invalidRequest");
    assert_eq!(err["message"], "invalid profile");

    // 4. Invalid coordinate value
    let (status, _, body) = get_response(&app, "/api/v1/route?coords=foo,bar;13.5,52.6").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "invalidRequest");
    assert_eq!(err["message"], "invalid coordinate");

    // 5. Leg too long (> 600 km)
    let (status, _, body) = get_response(&app, "/api/v1/route?coords=0.12,51.5;12.49,41.9").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "invalidRequest");
    assert_eq!(err["message"], "route leg too long");
}

#[tokio::test]
async fn http_routing_success_and_steps() {
    let mock_server = MockServer::start().await;

    let osrm_fixture = json!({
        "code": "Ok",
        "routes": [{
            "distance": 850.4,
            "duration": 120.2,
            "geometry": {
                "coordinates": [
                    [13.38886, 52.517033],
                    [13.38950, 52.517500],
                    [13.39000, 52.518000]
                ],
                "type": "LineString"
            },
            "legs": [{
                "steps": [
                    {
                        "maneuver": {
                            "type": "depart",
                            "location": [13.38886, 52.517033]
                        },
                        "name": "Friedrichstraße",
                        "ref": "B96",
                        "distance": 350.0,
                        "duration": 50.0
                    },
                    {
                        "maneuver": {
                            "type": "arrive",
                            "modifier": "right",
                            "location": [13.39000, 52.518000]
                        },
                        "name": "Unter den Linden",
                        "distance": 500.0,
                        "duration": 70.0
                    }
                ]
            }]
        }]
    });

    Mock::given(method("GET"))
        .and(path_regex(r"^/route/v1/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(osrm_fixture))
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.routing = RoutingConfig {
        car_endpoint: Some(mock_server.uri()),
        bike_endpoint: Some(mock_server.uri()),
        foot_endpoint: Some(mock_server.uri()),
        min_request_interval_ms: 50,
        ..RoutingConfig::default()
    };

    let core = LavenderCore::builder(config).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. GET /api/v1/route with steps=1
    let uri = "/api/v1/route?profile=car&coords=13.38886,52.517033;13.39000,52.518000&steps=1";
    let (status, headers, body) = get_response(&app, uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "application/json");

    let route: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(route["ok"], true);
    assert_eq!(route["profile"], "car");
    assert_eq!(route["distanceM"], 850);
    assert_eq!(route["durationS"], 120);
    assert_eq!(route["geometry"].as_array().unwrap().len(), 3);

    let steps = route["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["type"], "depart");
    assert_eq!(steps[0]["instruction"], "Head out on Friedrichstraße (B96)");
    assert_eq!(steps[1]["type"], "arrive");
    assert_eq!(steps[1]["instruction"], "Arrive at your destination, on the right");

    // 2. Legacy alias /api/route without steps
    let uri_legacy = "/api/route?profile=car&coords=13.38886,52.517033;13.39000,52.518000";
    let (status_leg, _, body_leg) = get_response(&app, uri_legacy).await;
    assert_eq!(status_leg, StatusCode::OK);
    let route_leg: serde_json::Value = serde_json::from_slice(&body_leg).unwrap();
    assert_eq!(route_leg["ok"], true);
    assert!(route_leg.get("steps").is_none());
}

#[tokio::test]
async fn http_routing_not_found_on_no_route() {
    let mock_server = MockServer::start().await;

    let no_route_fixture = json!({
        "code": "NoRoute",
        "message": "Impossible route between points"
    });

    Mock::given(method("GET"))
        .and(path_regex(r"^/route/v1/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(no_route_fixture))
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.routing = RoutingConfig {
        foot_endpoint: Some(mock_server.uri()),
        min_request_interval_ms: 50,
        ..RoutingConfig::default()
    };

    let core = LavenderCore::builder(config).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    let (status, _, body) = get_response(
        &app,
        "/api/v1/route?profile=foot&coords=13.38886,52.517033;13.39000,52.518000",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "notFound");
    assert_eq!(err["message"], "no route found");
}

#[tokio::test]
async fn http_routing_upstream_429_forwarding() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/route/v1/.*"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "15")
                .set_body_string("Too Many Requests"),
        )
        .mount(&mock_server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.routing = RoutingConfig {
        foot_endpoint: Some(mock_server.uri()),
        min_request_interval_ms: 50,
        ..RoutingConfig::default()
    };

    let core = LavenderCore::builder(config).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    let (status, headers, body) = get_response(
        &app,
        "/api/v1/route?profile=foot&coords=13.38886,52.517033;13.39000,52.518000",
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), "15");
    let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(err["code"], "rateLimited");
    assert_eq!(err["message"], "routing service is rate limited");
    assert_eq!(err["retryable"], true);
}
