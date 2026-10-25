#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jiff::Timestamp;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::air_quality::EcccAirQualityProvider;
use lavender_domain::{AirQualityBand, AirQualitySnapshot, Freshness};
use lavender_server::config::ServerSettings;
use serde_json::json;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn get(app: &axum::Router, url: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(url)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

#[tokio::test]
async fn http_air_quality_route_returns_snapshot() {
    let upstream = MockServer::start().await;

    let stations_payload = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "id": "60104",
                "geometry": {
                    "type": "Point",
                    "coordinates": [-79.38, 43.65]
                },
                "properties": {
                    "location_id": "60104",
                    "location_name_en": "Toronto Downtown",
                    "eccc_administrative-zone": "ont"
                }
            },
            {
                "type": "Feature",
                "id": "80100",
                "geometry": {
                    "type": "Point",
                    "coordinates": [-123.12, 49.28]
                },
                "properties": {
                    "location_id": "80100",
                    "location_name_en": "Vancouver Kitsilano",
                    "eccc_administrative-zone": "pyr"
                }
            }
        ]
    });

    let now = Timestamp::now();
    let obs_time_str = now.to_string();

    let obs_payload = json!({
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "properties": {
                    "location_id": "60104",
                    "aqhi": 3.8,
                    "observation_datetime": obs_time_str,
                    "special_notes_en": null
                }
            },
            {
                "type": "Feature",
                "properties": {
                    "location_id": "80100",
                    "aqhi": 11.2,
                    "observation_datetime": obs_time_str,
                    "special_notes_en": "Wildfire smoke"
                }
            }
        ]
    });

    Mock::given(path("/collections/aqhi-stations/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(stations_payload))
        .mount(&upstream)
        .await;

    Mock::given(path("/collections/aqhi-observations-realtime/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(obs_payload))
        .mount(&upstream)
        .await;

    let stations_url =
        Url::parse(&format!("{}/collections/aqhi-stations/items", upstream.uri())).unwrap();
    let obs_url =
        Url::parse(&format!("{}/collections/aqhi-observations-realtime/items", upstream.uri()))
            .unwrap();

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts.push("127.0.0.1".to_owned());
    let fetcher = Arc::new(Fetcher::new(config.fetch.clone()).unwrap());
    let aq_provider = Arc::new(EcccAirQualityProvider::with_urls(fetcher, stations_url, obs_url));

    let core = LavenderCore::builder(config).air_quality(aq_provider).build().unwrap();

    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. GET /api/v1/air-quality
    let (status, body) = get(&app, "/api/v1/air-quality").await;
    assert_eq!(status, StatusCode::OK);

    let snapshot: AirQualitySnapshot = serde_json::from_value(body.clone()).unwrap();
    assert_eq!(snapshot.freshness, Freshness::Live);
    assert_eq!(snapshot.readings.len(), 2);

    // Sorted worst air quality first
    assert_eq!(snapshot.readings[0].station_id, "80100");
    assert_eq!(snapshot.readings[0].value, 11);
    assert_eq!(snapshot.readings[0].band, AirQualityBand::VeryHigh);
    assert_eq!(snapshot.readings[0].label, "10+");
    assert_eq!(snapshot.readings[0].risk, "Very High health risk");
    assert_eq!(snapshot.readings[0].color, "#c1121f");
    assert_eq!(snapshot.readings[0].note, Some("Wildfire smoke".to_owned()));

    assert_eq!(snapshot.readings[1].station_id, "60104");
    assert_eq!(snapshot.readings[1].value, 4);
    assert_eq!(snapshot.readings[1].band, AirQualityBand::Moderate);
    assert_eq!(snapshot.readings[1].label, "4");
    assert_eq!(snapshot.readings[1].risk, "Moderate health risk");
    assert_eq!(snapshot.readings[1].color, "#ffd43b");
    assert_eq!(snapshot.readings[1].note, None);

    // 2. GET /api/air-quality (legacy alias)
    let (status_alias, body_alias) = get(&app, "/api/air-quality").await;
    assert_eq!(status_alias, StatusCode::OK);
    let snapshot_alias: AirQualitySnapshot = serde_json::from_value(body_alias).unwrap();
    assert_eq!(snapshot_alias.readings.len(), 2);
}
