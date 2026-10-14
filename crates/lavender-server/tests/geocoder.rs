//! The HTTP route and async core use real Fetcher parsing against a saved upstream document.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::{CoreConfig, GeocoderConfig};
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn search(app: &axum::Router, query: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/search?{query}"))
                .header(header::HOST, "localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn http_places_are_explicit_cached_grouped_and_attributed() {
    let upstream = MockServer::start().await;
    Mock::given(path("/places/search"))
        .and(query_param("q", "London"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../lavender-core/tests/fixtures/nominatim-london.json"
        )))
        .expect(1)
        .mount(&upstream)
        .await;
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    config.geocoder = GeocoderConfig {
        enabled: true,
        base_url: Some(Url::parse(&format!("{}/places/", upstream.uri())).unwrap()),
        ..GeocoderConfig::default()
    };
    let core = LavenderCore::builder(config).providers(Vec::new()).build().unwrap();
    let app = lavender_server::app(core.clone(), &ServerSettings::default());
    let (status, body) = search(&app, "q=60.17,24.94&places=true").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["coordinates"].as_array().unwrap().len(), 1);
    let (status, body) = search(&app, "q=London&places=false").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["places"], serde_json::json!([]));
    for _ in 0..2 {
        let (status, body) = search(&app, "q=London&places=true").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["places"][0]["name"], "London");
        assert_eq!(body["places"][0]["sourceId"], "osm-geocoder-nominatim");
        assert!(
            body["notices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|notice| notice.as_str().unwrap().contains("OpenStreetMap"))
        );
    }
    let (status, body) = search(&app, "q=London&places=yes").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");
    core.shutdown();
}
