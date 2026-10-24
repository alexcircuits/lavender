#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
async fn http_aircraft_enrichment_and_legacy_routes() {
    let server = MockServer::start().await;
    let base = Url::parse(&server.uri()).unwrap();

    // Mock adsbdb callsign
    Mock::given(method("GET"))
        .and(path("/callsign/DAL456"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{
                "response": {
                    "flightroute": {
                        "airline": { "name": "Delta Air Lines" },
                        "origin": { "iata_code": "ATL", "municipality": "Atlanta" },
                        "destination": { "iata_code": "AUS", "municipality": "Austin" }
                    }
                }
            }"#,
        ))
        .mount(&server)
        .await;

    // Mock adsbdb type
    Mock::given(method("GET"))
        .and(path("/aircraft/a15c54"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{
                "response": {
                    "aircraft": {
                        "icao_type": "E75L",
                        "manufacturer": "Embraer",
                        "type": "ERJ-175",
                        "registration": "N123SY"
                    }
                }
            }"#,
        ))
        .mount(&server)
        .await;

    // Mock adsb.lol point and mil for /api/flights
    Mock::given(method("GET"))
        .and(path_regex(r"^/v2/point/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"now": 1760000000, "ac": [
                {"hex":"a15c54","flight":"DAL456","lat":33.6407,"lon":-84.4277,"alt_baro":31000,"seen_pos":1.0}
            ]}"#,
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/mil"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"now": 1760000000, "ac": [
                {"hex":"ae0001","flight":"RCH101","lat":38.9072,"lon":-77.0369,"alt_baro":25000,"dbFlags":1,"seen_pos":1.0}
            ]}"#,
        ))
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    config.hub.linger_s = 0;
    config.providers.adsbdb.enabled = true;
    config.providers.adsbdb.base_url = base.clone();
    config.providers.adsb_lol.base_url = base;

    let core = LavenderCore::builder(config).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // 1. GET /api/v1/aircraft/{hex}/type
    let (status, body) = get_json(&app, "/api/v1/aircraft/a15c54/type").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["found"], true);
    assert_eq!(body["typeCode"], "E75L");
    assert_eq!(body["typeName"], "Embraer ERJ-175");
    assert_eq!(body["registration"], "N123SY");

    // 2. Legacy alias: GET /api/adsbdb/type/{hex}
    let (status, body) = get_json(&app, "/api/adsbdb/type/a15c54").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["found"], true);
    assert_eq!(body["typeCode"], "E75L");

    // 3. GET /api/v1/aircraft/route/{callsign}
    let (status, body) = get_json(&app, "/api/v1/aircraft/route/DAL456").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["found"], true);
    assert_eq!(body["airline"], "Delta Air Lines");
    assert_eq!(body["origin"]["code"], "ATL");
    assert_eq!(body["destination"]["code"], "AUS");

    // 4. Legacy alias: GET /api/adsbdb/route/{callsign}
    let (status, body) = get_json(&app, "/api/adsbdb/route/DAL456").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["found"], true);
    assert_eq!(body["airline"], "Delta Air Lines");

    // 5. Validation failure: bad hex
    let (status, body) = get_json(&app, "/api/v1/aircraft/not-a-hex/type").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");

    // 6. Validation failure: bad callsign
    let (status, body) = get_json(&app, "/api/v1/aircraft/route/X").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalidRequest");

    // 7. Aircraft track missing: 404
    let (status, body) = get_json(&app, "/api/v1/aircraft/a15c54/track").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "notFound");

    // 8. Legacy track missing: 404
    let (status, _) = get_json(&app, "/api/flights/track?icao24=a15c54").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 9. Legacy flights snapshot
    let (status, body) = get_json(&app, "/api/flights").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["time"].is_number());
    assert!(body["states"].is_array());
}
