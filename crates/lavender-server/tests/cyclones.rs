//! The generic HTTP feed carries the complete cyclone contract without a new route.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn get(app: &axum::Router, url: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(url).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn http_cyclone_feed_preserves_card_fields_and_geometry() {
    let upstream = MockServer::start().await;
    let mut document: serde_json::Value = serde_json::from_str(include_str!(
        "../../lavender-core/tests/fixtures/cyclones-status.json"
    ))
    .unwrap();
    let now =
        lavender_domain::Timestamp::from_second(lavender_domain::Timestamp::now().as_second())
            .unwrap();
    document["activeStorms"][0]["lastUpdate"] = serde_json::json!(now.to_string());
    document["activeStorms"][0]["forecastAdvisory"]["issuance"] =
        serde_json::json!(now.to_string());
    Mock::given(path("/status/CurrentStorms.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(document))
        .expect(1)
        .mount(&upstream)
        .await;
    for (index, body) in [
        include_str!("../../lavender-core/tests/fixtures/cyclones-points.json"),
        include_str!("../../lavender-core/tests/fixtures/cyclones-track.json"),
        include_str!("../../lavender-core/tests/fixtures/cyclones-cone.json"),
    ]
    .into_iter()
    .enumerate()
    {
        Mock::given(path(format!("/gis/{}/query", index + 5)))
            .and(query_param("f", "geojson"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&upstream)
            .await;
    }
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.hub.linger_s = 0;
    config.providers.cyclones.status_base_url =
        Url::parse(&format!("{}/status/", upstream.uri())).unwrap();
    config.providers.cyclones.gis_base_url =
        Url::parse(&format!("{}/gis/", upstream.uri())).unwrap();
    let core = LavenderCore::builder(config).build().unwrap();
    let app = lavender_server::app(core.clone(), &ServerSettings::default());
    let (status, body) = get(&app, "/api/v1/feeds/cyclones").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["feed"], "cyclones");
    assert_eq!(body["items"]["kind"], "cyclones");
    assert_eq!(body["freshness"]["state"], "live");
    assert_eq!(body["coverage"]["kind"], "partial");
    let storm = &body["items"]["items"][0];
    assert_eq!(storm["id"], "al062026");
    assert_eq!(storm["name"], "TEST ALPHA");
    assert_eq!(storm["classification"], "HU");
    assert_eq!(storm["basin"], "AL");
    assert_eq!(storm["intensityKt"].as_f64(), Some(80.0));
    assert_eq!(storm["pressureMb"].as_f64(), Some(970.0));
    assert_eq!(storm["movement"]["directionDegrees"].as_f64(), Some(300.0));
    assert_eq!(storm["movement"]["speedKt"].as_f64(), Some(12.0));
    assert_eq!(storm["geometryStatus"], "current");
    assert_eq!(storm["geometryAdvisoryNumber"], "6");
    assert_eq!(storm["track"]["type"], "MultiLineString");
    assert_eq!(storm["cone"]["type"], "MultiPolygon");
    assert_eq!(storm["forecastPoints"][1]["tauHours"].as_f64(), Some(24.0));
    assert!(storm["positionAt"].is_string() && storm["issuedAt"].is_string());
    assert!(storm["advisoryUrl"].as_str().unwrap().starts_with("https://www.nhc.noaa.gov/"));
    assert!(storm["outlookUrl"].as_str().unwrap().contains("basin=atlc"));
    let (_, catalog) = get(&app, "/api/v1/catalog").await;
    let layer =
        catalog["layers"].as_array().unwrap().iter().find(|l| l["feed"] == "cyclones").unwrap();
    assert_eq!(layer["cadenceS"], 600);
    assert_eq!(layer["sourceIds"], serde_json::json!(["noaa-nhc-cphc"]));
    core.shutdown();
}
