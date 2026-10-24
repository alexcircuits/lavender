//! Comprehensive aircraft sources contract tests (ENG-948):
//! - Recorded adsb.lol and OpenSky responses normalise into Lavender's `Aircraft` format.
//! - Merging civil and military lists removes duplicates by `hex` and marks them military.
//! - Two overlapping views in the same region cause one upstream request.
//! - Bounded track buffers per aircraft and globally.
//! - Evaluation licensing restrictions for OpenSky and adsbdb.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use lavender_core::config::CoreConfig;
use lavender_core::providers::adsb_lol;
use lavender_core::providers::opensky;
use lavender_core::{LavenderCore, sources};
use lavender_domain::{BBox, FeedId, FeedItems, FeedQuery, LicenceMode};
use url::Url;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OPENSKY_FIXTURE: &str = include_str!("fixtures/opensky-recorded.json");
const ADSBLOL_FIXTURE: &str = include_str!("fixtures/adsb-lol-recorded.json");

#[test]
fn contract_test_opensky_recorded_response() {
    let (aircraft, dropped, doc_time) =
        opensky::parse_opensky_states(OPENSKY_FIXTURE.as_bytes()).unwrap();

    assert_eq!(dropped, 0);
    assert_eq!(aircraft.len(), 3);
    assert!(doc_time.is_some());

    let ual = aircraft.iter().find(|a| a.hex == "a1b2c3").unwrap();
    assert_eq!(ual.callsign.as_deref(), Some("UAL123"));
    assert_eq!(ual.category.as_deref(), Some("A3"));
    assert!(!ual.on_ground);
    assert_eq!(ual.source, sources::OPENSKY);
    assert!((ual.position.lat - 30.2672).abs() < 1e-4);
    assert!((ual.position.lon - (-97.7431)).abs() < 1e-4);
    assert!(ual.altitude_baro_ft.is_some());
    assert!(ual.altitude_geom_ft.is_some());
    assert!(ual.ground_speed_kt.is_some());

    let ryr = aircraft.iter().find(|a| a.hex == "4ca1b2").unwrap();
    assert_eq!(ryr.callsign.as_deref(), Some("RYR456"));
    assert_eq!(ryr.category.as_deref(), Some("A1"));
    assert!(ryr.on_ground);
    assert!(ryr.altitude_baro_ft.is_none());
    assert_eq!(ryr.squawk.as_deref(), Some("7000"));

    let rch = aircraft.iter().find(|a| a.hex == "ae0001").unwrap();
    assert_eq!(rch.callsign.as_deref(), Some("RCH101"));
    assert_eq!(rch.category.as_deref(), Some("A5"));
    assert!(!rch.on_ground);
}

#[test]
fn contract_test_adsb_lol_recorded_response() {
    let (aircraft, dropped) = adsb_lol::parse(ADSBLOL_FIXTURE.as_bytes(), false).unwrap();

    assert_eq!(dropped, 0);
    assert_eq!(aircraft.len(), 2);

    let ryr = aircraft.iter().find(|a| a.hex == "4ca1b2").unwrap();
    assert_eq!(ryr.callsign.as_deref(), Some("RYR12AB"));
    assert_eq!(ryr.registration.as_deref(), Some("EI-ABC"));
    assert_eq!(ryr.type_code.as_deref(), Some("B738"));
    assert_eq!(ryr.category.as_deref(), Some("A3"));
    assert!(!ryr.is_military);
    assert_eq!(ryr.source, sources::ADSB_LOL);

    let rch = aircraft.iter().find(|a| a.hex == "ae0001").unwrap();
    assert_eq!(rch.callsign.as_deref(), Some("RCH101"));
    assert_eq!(rch.registration.as_deref(), Some("08-8191"));
    assert_eq!(rch.type_code.as_deref(), Some("C17"));
    assert!(rch.is_military);
}

#[tokio::test]
async fn merging_civil_and_military_removes_duplicate_hex_and_marks_military() {
    let server = MockServer::start().await;
    let base = Url::parse(&server.uri()).unwrap();

    // adsb.lol regional point returns a contact without military flag
    Mock::given(method("GET"))
        .and(path_regex(r"^/v2/point/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"now": 1760000000, "ac": [
                {"hex":"ae0001","flight":"RCH101","lat":38.9072,"lon":-77.0369,"alt_baro":25000,"dbFlags":0,"seen_pos":5.0},
                {"hex":"a1b2c3","flight":"CIV123","lat":38.9000,"lon":-77.0000,"alt_baro":10000,"seen_pos":2.0}
            ]}"#,
        ))
        .mount(&server)
        .await;

    // adsb.lol military endpoint returns ae0001 with military feed flag
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
    config.providers.adsb_lol.base_url = base;

    let core = LavenderCore::builder(config).build().unwrap();

    // Query world to get global military
    let world_snapshot =
        core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::WORLD)).await.unwrap();
    let FeedItems::Aircraft(mil_list) = &world_snapshot.items else { panic!() };
    assert_eq!(mil_list.len(), 1);
    assert_eq!(mil_list[0].hex, "ae0001");
    assert!(mil_list[0].is_military);

    // Query regional cell
    let bbox = BBox::new(-78.0, 38.0, -76.0, 40.0).unwrap();
    let regional_snapshot =
        core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(bbox)).await.unwrap();
    let FeedItems::Aircraft(reg_list) = &regional_snapshot.items else { panic!() };
    assert_eq!(reg_list.len(), 2);
    let ae0001 = reg_list.iter().find(|a| a.hex == "ae0001").unwrap();
    assert_eq!(ae0001.callsign.as_deref(), Some("RCH101"));
}

#[tokio::test]
async fn two_overlapping_views_in_same_region_cause_one_upstream_request() {
    let server = MockServer::start().await;
    let base = Url::parse(&server.uri()).unwrap();

    // Expect exactly one request for the cell
    Mock::given(method("GET"))
        .and(path_regex(r"^/v2/point/.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"now": 1760000000, "ac": [
                {"hex":"4ca1b2","flight":"RYR12AB","lat":53.3498,"lon":-6.2603,"alt_baro":35000,"seen_pos":1.0}
            ]}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    config.hub.linger_s = 30; // slots linger so second snapshot reuses it
    config.providers.adsb_lol.base_url = base;

    let core = LavenderCore::builder(config).build().unwrap();

    // View 1
    let view1 = BBox::new(-7.0, 52.0, -5.0, 54.0).unwrap();
    let s1 = core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(view1)).await.unwrap();
    assert_eq!(s1.items.len(), 1);

    // Overlapping View 2 in the same cell
    let view2 = BBox::new(-6.8, 52.5, -5.5, 53.8).unwrap();
    let s2 = core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(view2)).await.unwrap();
    assert_eq!(s2.items.len(), 1);
}

#[tokio::test]
async fn evaluation_licence_policy_rejects_opensky_in_commercial_mode_without_agreement() {
    let mut config = CoreConfig { licence_mode: LicenceMode::Commercial, ..Default::default() };
    config.providers.opensky.enabled = true;
    config.providers.adsbdb.enabled = false;
    config.licence_acknowledged = vec![]; // no agreement

    let errors = config.validate().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("OpenSky is licensed for non-commercial/evaluation use only")),
        "commercial validation should reject OpenSky without agreement: {errors:?}"
    );

    // Now clear with agreement
    config.licence_acknowledged = vec![sources::OPENSKY.to_owned()];
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn evaluation_licence_policy_rejects_adsbdb_in_commercial_mode_without_agreement() {
    let mut config = CoreConfig { licence_mode: LicenceMode::Commercial, ..Default::default() };
    config.providers.adsbdb.enabled = true;
    config.licence_acknowledged = vec![]; // no agreement

    let errors = config.validate().unwrap_err();
    assert!(
        errors.iter().any(|e| e.contains("adsbdb route enrichment requires permission")),
        "commercial validation should reject adsbdb without agreement: {errors:?}"
    );

    // Now clear with agreement
    config.licence_acknowledged = vec![sources::ADS_B_DB.to_owned()];
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn aircraft_enrichment_route_and_type_lookups_with_negative_caching() {
    let server = MockServer::start().await;
    let base = Url::parse(&server.uri()).unwrap();

    // Route for BAW123
    Mock::given(method("GET"))
        .and(path("/callsign/BAW123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{
                "response": {
                    "flightroute": {
                        "airline": { "name": "British Airways" },
                        "origin": { "iata_code": "LHR", "municipality": "London" },
                        "destination": { "iata_code": "JFK", "municipality": "New York" }
                    }
                }
            }"#,
        ))
        .expect(1) // cached on second call
        .mount(&server)
        .await;

    // Type for 4ca1b2
    Mock::given(method("GET"))
        .and(path("/aircraft/4ca1b2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{
                "response": {
                    "aircraft": {
                        "icao_type": "B738",
                        "manufacturer": "Boeing",
                        "type": "737-800",
                        "registration": "EI-ABC"
                    }
                }
            }"#,
        ))
        .expect(1) // cached on second call
        .mount(&server)
        .await;

    // 404 for unknown hex
    Mock::given(method("GET"))
        .and(path("/aircraft/ffffff"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1) // negative cached
        .mount(&server)
        .await;

    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    config.providers.adsbdb.enabled = true;
    config.providers.adsbdb.base_url = base;

    let core = LavenderCore::builder(config).build().unwrap();

    // First call: fetches
    let route1 = core.aircraft_enrichment_route("BAW123").await.unwrap();
    assert!(route1.found);
    assert_eq!(route1.airline.as_deref(), Some("British Airways"));
    assert_eq!(route1.origin.unwrap().code, "LHR");

    // Second call: served from cache (wiremock expect(1) guarantees no second network request)
    let route2 = core.aircraft_enrichment_route("baw123").await.unwrap();
    assert!(route2.found);

    // Type lookup
    let type1 = core.aircraft_enrichment_type("4ca1b2").await.unwrap();
    assert!(type1.found);
    assert_eq!(type1.type_code.as_deref(), Some("B738"));
    assert_eq!(type1.type_name.as_deref(), Some("Boeing 737-800"));

    let type2 = core.aircraft_enrichment_type("4CA1B2").await.unwrap();
    assert!(type2.found);

    // Negative cache lookup
    let unknown1 = core.aircraft_enrichment_type("ffffff").await.unwrap();
    assert!(!unknown1.found);

    let unknown2 = core.aircraft_enrichment_type("ffffff").await.unwrap();
    assert!(!unknown2.found);
}
