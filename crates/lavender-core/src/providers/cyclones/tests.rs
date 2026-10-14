use super::*;
use lavender_domain::{CycloneConeGeometry, CycloneTrackGeometry};

const STATUS: &[u8] = include_bytes!("../../../tests/fixtures/cyclones-status.json");
const POINTS: &[u8] = include_bytes!("../../../tests/fixtures/cyclones-points.json");
const TRACK: &[u8] = include_bytes!("../../../tests/fixtures/cyclones-track.json");
const CONE: &[u8] = include_bytes!("../../../tests/fixtures/cyclones-cone.json");

fn now() -> Timestamp {
    "2026-10-14T00:00:00Z".parse().unwrap()
}
fn active() -> Vec<Cyclone> {
    parse_status(STATUS, now()).unwrap()
}

#[test]
fn preserves_legacy_advisory_fields_and_geometry_topology() {
    let mut storms = active();
    let storm = &storms[0];
    assert_eq!(storm.name, "TEST ALPHA");
    assert_eq!(storm.advisory_number, "6");
    assert_eq!(storm.basin, "AL");
    assert_eq!(storm.classification, "HU");
    assert_eq!(storm.intensity_kt, Some(80.0));
    assert_eq!(storm.pressure_mb, Some(970.0));
    assert_eq!(storm.movement.direction_degrees, Some(300.0));
    assert_eq!(storm.movement.speed_kt, Some(12.0));
    assert_ne!(storm.position_at, storm.issued_at);
    assert_eq!(
        storm.advisory_url.as_deref(),
        Some("https://www.nhc.noaa.gov/text/MIATCMAT1.shtml")
    );
    attach_geometry(&mut storms, [POINTS, TRACK, CONE]).unwrap();
    let storm = &storms[0];
    assert_eq!(storm.geometry_status, CycloneGeometryStatus::Current);
    assert_eq!(storm.geometry_advisory_number.as_deref(), Some("6"));
    assert_eq!(storm.forecast_points.len(), 2);
    assert_eq!(storm.forecast_points[1].tau_hours, 24.0);
    let Some(CycloneTrackGeometry::MultiLineString(parts)) = &storm.track else {
        panic!("split track")
    };
    assert_eq!(parts.len(), 3);
    let Some(CycloneConeGeometry::MultiPolygon(polygons)) = &storm.cone else {
        panic!("split cone")
    };
    assert_eq!(polygons.len(), 3);
    assert_eq!(polygons[0].len(), 2, "exterior and hole retained");
}

#[test]
fn empty_season_is_valid_but_corrupt_status_is_not_empty() {
    assert_eq!(
        parse_status(include_bytes!("../../../tests/fixtures/cyclones-empty.json"), now()).unwrap(),
        Vec::<Cyclone>::new()
    );
    assert!(
        parse_status(include_bytes!("../../../tests/fixtures/cyclones-malformed.json"), now())
            .is_err()
    );
    assert!(parse_status(b"<html>outage</html>", now()).is_err());
}

#[test]
fn wrong_or_future_advisories_positions_and_text_are_rejected() {
    for (field, value) in [
        ("lastUpdate", serde_json::json!("2026-10-14T00:06:00Z")),
        ("lastUpdate", serde_json::json!("2026-10-13T11:59:59Z")),
        ("lastUpdate", serde_json::json!("2026-02-30T00:00:00Z")),
        ("latitudeNumeric", serde_json::json!(91)),
        ("longitudeNumeric", serde_json::json!("-60")),
        ("name", serde_json::json!("<script>")),
    ] {
        let mut value_doc: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
        value_doc["activeStorms"][0][field] = value;
        assert!(parse_status(&serde_json::to_vec(&value_doc).unwrap(), now()).is_err(), "{field}");
    }
    let mut doc: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
    let storm = doc["activeStorms"][0].clone();
    doc["activeStorms"].as_array_mut().unwrap().push(storm);
    assert!(parse_status(&serde_json::to_vec(&doc).unwrap(), now()).is_err());
}

#[test]
fn optional_numbers_and_non_official_links_are_dropped() {
    let mut doc: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
    let row = &mut doc["activeStorms"][0];
    row["intensity"] = serde_json::json!("unknown");
    row["pressure"] = serde_json::json!(500);
    row["forecastAdvisory"]["url"] =
        serde_json::json!("https://www.nhc.noaa.gov/text/MIATCMAT1.shtml?private=secret");
    let storms = parse_status(&serde_json::to_vec(&doc).unwrap(), now()).unwrap();
    assert_eq!(storms[0].intensity_kt, None);
    assert_eq!(storms[0].pressure_mb, None);
    assert_eq!(storms[0].advisory_url, None);
}

#[test]
fn older_geometry_is_never_relabelled_as_current() {
    let mut storms = active();
    let body = String::from_utf8(TRACK.to_vec()).unwrap().replace("006", "005");
    attach_geometry(&mut storms, [POINTS, body.as_bytes(), CONE]).unwrap();
    assert_eq!(storms[0].geometry_status, CycloneGeometryStatus::Pending);
    assert_eq!(storms[0].track, None);
    assert_eq!(storms[0].cone, None);
    assert_eq!(storms[0].forecast_points, Vec::<lavender_domain::CycloneForecastPoint>::new());
}

#[test]
fn malformed_mismatched_duplicate_geometry_rejects_atomically() {
    let mut cone: serde_json::Value = serde_json::from_slice(CONE).unwrap();
    cone["features"][0]["geometry"]["coordinates"][0][0][4] = serde_json::json!([-61, 19]);
    let mut points: serde_json::Value = serde_json::from_slice(POINTS).unwrap();
    points["features"][1]["properties"]["tau"] = serde_json::json!(0);
    let mut track: serde_json::Value = serde_json::from_slice(TRACK).unwrap();
    track["features"][0]["properties"]["advisnum"] = serde_json::json!("5");
    for bodies in [
        [POINTS.to_vec(), TRACK.to_vec(), serde_json::to_vec(&cone).unwrap()],
        [serde_json::to_vec(&points).unwrap(), TRACK.to_vec(), CONE.to_vec()],
        [POINTS.to_vec(), serde_json::to_vec(&track).unwrap(), CONE.to_vec()],
    ] {
        let mut storms = active();
        let before = storms.clone();
        assert!(attach_geometry(&mut storms, [&bodies[0], &bodies[1], &bodies[2]]).is_err());
        assert_eq!(storms, before);
    }
}

#[test]
fn transfer_and_coordinate_budgets_are_enforced() {
    let mut points: serde_json::Value = serde_json::from_slice(POINTS).unwrap();
    points["exceededTransferLimit"] = serde_json::json!(true);
    assert!(
        attach_geometry(&mut active(), [&serde_json::to_vec(&points).unwrap(), TRACK, CONE])
            .is_err()
    );
    let mut track: serde_json::Value = serde_json::from_slice(TRACK).unwrap();
    track["features"][0]["geometry"] =
        serde_json::json!({"type":"MultiLineString","coordinates":vec![vec![[0.0,0.0];10_000];3]});
    assert!(
        attach_geometry(&mut active(), [POINTS, &serde_json::to_vec(&track).unwrap(), CONE])
            .is_err()
    );
}

#[test]
fn parsers_reject_oversized_documents_and_wrong_typed_transfer_flags() {
    let mut status: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
    status["unused"] = serde_json::json!("x".repeat(128 * 1024));
    assert!(parse_status(&serde_json::to_vec(&status).unwrap(), now()).is_err());
    let mut points: serde_json::Value = serde_json::from_slice(POINTS).unwrap();
    points["unused"] = serde_json::json!("x".repeat(512 * 1024));
    assert!(
        attach_geometry(&mut active(), [&serde_json::to_vec(&points).unwrap(), TRACK, CONE])
            .is_err()
    );
    points.as_object_mut().unwrap().remove("unused");
    points["exceededTransferLimit"] = serde_json::json!("true");
    assert!(
        attach_geometry(&mut active(), [&serde_json::to_vec(&points).unwrap(), TRACK, CONE])
            .is_err()
    );
}

#[tokio::test]
async fn failure_keeps_cached_timestamp_and_reason_until_next_success() {
    use crate::fetch::Fetcher;
    use crate::secrets::MemorySecretStore;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(path("/CurrentStorms.json"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let config = CyclonesConfig {
        status_base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
        ..CyclonesConfig::default()
    };
    let provider = CyclonesProvider::new(config);
    let fetch_config = crate::config::FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".to_owned()],
        ..crate::config::FetchConfig::default()
    };
    let fetcher = Fetcher::new(fetch_config).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    let fetched_at = Timestamp::from_second(Timestamp::now().as_second() - 700).unwrap();
    provider.state.lock().await.cache =
        Some(Cached { storms: Vec::new(), fetched_at, degraded: None });
    let first = provider.fetch(&ctx, Region::Global).await.unwrap();
    let second = provider.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(first.as_of, Some(fetched_at));
    assert_eq!(first.degraded.as_deref(), Some("Cached cyclone advisory; upstream unavailable."));
    assert_eq!(second, first, "cooldown cannot erase the failure reason or refresh timestamp");
}

#[test]
fn expired_advisories_and_cached_empty_seasons_are_not_usable() {
    let old = Timestamp::from_second(Timestamp::now().as_second() - 43_201).unwrap();
    assert!(!CyclonesProvider::usable(&Cached {
        storms: Vec::new(),
        fetched_at: old,
        degraded: None
    }));
    let mut storms = active();
    storms[0].issued_at = old;
    assert!(!CyclonesProvider::usable(&Cached {
        storms,
        fetched_at: Timestamp::now(),
        degraded: None
    }));
}

#[tokio::test]
async fn dropping_an_inflight_future_rolls_back_its_attempt_budget() {
    use crate::fetch::Fetcher;
    use crate::secrets::MemorySecretStore;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(path("/CurrentStorms.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"activeStorms\":[]}")
                .set_delay(Duration::from_millis(150)),
        )
        .mount(&server)
        .await;
    let provider = Arc::new(CyclonesProvider::new(CyclonesConfig {
        status_base_url: Url::parse(&format!("{}/", server.uri())).unwrap(),
        ..CyclonesConfig::default()
    }));
    let fetcher = Arc::new(
        Fetcher::new(crate::config::FetchConfig {
            allow_http_hosts: vec!["127.0.0.1".to_owned()],
            ..crate::config::FetchConfig::default()
        })
        .unwrap(),
    );
    let worker_provider = provider.clone();
    let worker_fetcher = fetcher.clone();
    let task = tokio::spawn(async move {
        let secrets = MemorySecretStore::default();
        let ctx = FetchContext {
            fetcher: &worker_fetcher,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            cache_dir: None,
        };
        worker_provider.fetch(&ctx, Region::Global).await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(provider.state.lock().await.attempted_at, None);
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    assert!(provider.fetch(&ctx, Region::Global).await.unwrap().items.is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
