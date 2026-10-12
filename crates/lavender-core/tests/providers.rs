//! Provider contract tests: real providers, real fetch policy, mock upstreams serving
//! recorded or hand-written fixtures. No test touches a live provider.

// Test helpers outside #[test] fns are not covered by clippy.toml's test allowances.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, SystemTime};

use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::celestrak::CelestrakProvider;
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use lavender_core::{LavenderCore, MemorySecretStore, sources};
use lavender_domain::{
    ApiError, BBox, Coverage, ErrorCode, FeedId, FeedItems, FeedQuery, Freshness, HealthState,
    SatelliteTrackRequest, Timestamp,
};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USGS_FIXTURE: &str = include_str!("fixtures/usgs-2.5_day.geojson");

const OMM: &str = r#"[{"OBJECT_NAME":"TEST STATION","OBJECT_ID":"1998-067A","EPOCH":"2026-10-10T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0007,"INCLINATION":51.64,"RA_OF_ASC_NODE":247.46,"ARG_OF_PERICENTER":130.53,"MEAN_ANOMALY":325.03,"EPHEMERIS_TYPE":0,"CLASSIFICATION_TYPE":"U","NORAD_CAT_ID":25544,"ELEMENT_SET_NO":999,"REV_AT_EPOCH":44393,"BSTAR":0.0003,"MEAN_MOTION_DOT":0.00016,"MEAN_MOTION_DDOT":0}]"#;

fn config_for(server: &MockServer) -> CoreConfig {
    let base = Url::parse(&server.uri()).unwrap();
    let mut c = CoreConfig::default();
    c.fetch.allow_http_hosts = vec!["127.0.0.1".into()];
    c.fetch.retries = 0;
    c.hub.linger_s = 0;
    c.hub.first_result_timeout_s = 5;
    c.providers.usgs.base_url = base.join("usgs/").unwrap();
    c.providers.adsb_lol.base_url = base.join("adsb/").unwrap();
    c.providers.celestrak.base_url = base.join("celestrak/").unwrap();
    c.providers.celestrak.groups = vec!["stations".into()];
    c
}

#[tokio::test]
async fn usgs_recorded_feed_normalises_every_valid_event() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/usgs/2.5_day.geojson"))
        .respond_with(ResponseTemplate::new(200).set_body_string(USGS_FIXTURE))
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config_for(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    let raw: serde_json::Value = serde_json::from_str(USGS_FIXTURE).unwrap();
    let raw_count = raw["features"].as_array().unwrap().len();
    let FeedItems::Earthquakes(quakes) = &s.items else { panic!() };
    assert_eq!(quakes.len(), raw_count, "every recorded feature is valid");
    assert!(quakes.windows(2).all(|w| w[0].time >= w[1].time), "newest first");
    assert!(
        quakes
            .iter()
            .all(|q| q.magnitude >= 2.5
                && q.url.as_deref().is_none_or(|u| u.starts_with("https://")))
    );
    assert_eq!(s.freshness, Freshness::Live);
    assert_eq!(s.sources, vec!["usgs-earthquakes"]);
}

#[tokio::test]
async fn usgs_html_error_page_is_unavailable_not_a_crash() {
    let server = MockServer::start().await;
    Mock::given(path("/usgs/2.5_day.geojson"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"))
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config_for(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Earthquakes)).await.unwrap();
    match &s.freshness {
        Freshness::Unavailable { reason } => assert!(reason.contains("USGS"), "{reason}"),
        other => panic!("expected unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn adsb_lol_point_queries_and_military_fallback() {
    let server = MockServer::start().await;
    Mock::given(path_regex(r"^/adsb/v2/point/[-0-9.]+/[-0-9.]+/[0-9]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ac":[{"hex":"4ca1b2","flight":"RYR12AB","lat":61.0,"lon":24.0,"alt_baro":35000}]}"#,
        ))
        .mount(&server).await;
    Mock::given(path("/adsb/v2/mil"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ac":[{"hex":"ae0001","lat":38.9,"lon":-77.0,"alt_baro":20000}]}"#,
        ))
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config_for(&server)).build().unwrap();

    let view = BBox::new(22.0, 60.0, 25.0, 62.0).unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(view)).await.unwrap();
    let FeedItems::Aircraft(list) = &s.items else { panic!() };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].callsign.as_deref(), Some("RYR12AB"));
    assert_eq!(s.coverage, Coverage::Complete);

    let world =
        core.snapshot(FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::WORLD)).await.unwrap();
    let FeedItems::Aircraft(list) = &world.items else { panic!() };
    assert!(list.iter().all(|a| a.is_military));
    assert!(matches!(world.coverage, Coverage::Partial { .. }));
}

#[tokio::test]
async fn celestrak_caches_groups_on_disk_and_tracks_satellites() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .and(query_param("GROUP", "stations"))
        .and(query_param("FORMAT", "json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(OMM))
        .expect(1)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().unwrap();

    let core = LavenderCore::builder(config_for(&server)).cache_dir(cache.path()).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(s.items.len(), 1);
    let track = core
        .satellite_track(SatelliteTrackRequest {
            norad_id: 25544,
            from: None,
            minutes: Some(90),
            step_s: Some(60),
        })
        .await
        .unwrap();
    assert_eq!(track.points.len(), 91);
    assert!(track.points.iter().all(|p| p.position.alt_m.unwrap() > 300_000.0));
    assert!(
        core.satellite_track(SatelliteTrackRequest {
            norad_id: 1,
            from: None,
            minutes: None,
            step_s: None
        })
        .await
        .is_err()
    );
    assert!(
        core.satellite_track(SatelliteTrackRequest {
            norad_id: 25544,
            from: None,
            minutes: Some(1440),
            step_s: Some(10)
        })
        .await
        .is_err()
    );
    // Review LOW: `from` was unbounded; tracks far from the epoch are meaningless.
    let far = "2026-11-20T00:00:00Z".parse::<Timestamp>().unwrap();
    let err = core
        .satellite_track(SatelliteTrackRequest {
            norad_id: 25544,
            from: Some(far),
            minutes: Some(10),
            step_s: Some(60),
        })
        .await
        .unwrap_err();
    assert_eq!(ApiError::from(&err).code, ErrorCode::InvalidRequest, "{err}");
    let near = "2026-10-11T00:00:00Z".parse::<Timestamp>().unwrap();
    core.satellite_track(SatelliteTrackRequest {
        norad_id: 25544,
        from: Some(near),
        minutes: Some(10),
        step_s: Some(60),
    })
    .await
    .unwrap();
    core.shutdown();

    // A new core (a restart) reads the fresh disk cache instead of calling CelesTrak again
    // (the mock expects exactly one request).
    let core = LavenderCore::builder(config_for(&server)).cache_dir(cache.path()).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(s.items.len(), 1);
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn commercial_mode_refuses_unlicensed_feeds() {
    let server = MockServer::start().await;
    let mut config = config_for(&server);
    config.licence_mode = lavender_domain::LicenceMode::Commercial;
    let core = LavenderCore::builder(config).build().unwrap();
    let err = core.subscribe(FeedQuery::new(FeedId::Satellites)).unwrap_err();
    assert!(err.to_string().contains("licence"), "{err}");
    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Satellites).unwrap();
    assert_eq!(layer.availability, lavender_domain::Availability::DisabledByLicence);
}

/// Writes a cached CelesTrak group whose modification time is `age` ago.
fn write_cache(dir: &Path, group: &str, body: &str, age: Duration) {
    let path = dir.join("celestrak").join(format!("{group}.json"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() - age).unwrap();
}

fn seconds_ago(t: Timestamp) -> i64 {
    Timestamp::now().duration_since(t).as_secs()
}

#[tokio::test]
async fn celestrak_snapshot_reports_the_cache_age_not_now() {
    // Review finding 9: data served from cache was stamped `fetchedAt = now`.
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(200).set_body_string(OMM))
        .expect(0)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().unwrap();
    write_cache(cache.path(), "stations", OMM, Duration::from_hours(1));
    let core = LavenderCore::builder(config_for(&server)).cache_dir(cache.path()).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(s.items.len(), 1);
    let age = seconds_ago(s.fetched_at.unwrap());
    assert!((3_500..3_700).contains(&age), "fetchedAt is the cache's time: {age} s ago");
    assert_eq!(s.freshness, Freshness::Live, "an hour-old cache is within the 2 h cadence");
}

#[tokio::test]
async fn celestrak_failed_refresh_over_an_old_cache_is_degraded_not_live() {
    // Review finding 9: a failed refresh with any cache returned Ok, so health said
    // healthy and freshness said Live over five-hour-old elements.
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().unwrap();
    write_cache(cache.path(), "stations", OMM, Duration::from_hours(5));
    let mut config = config_for(&server);
    config.fetch.retries = 2; // CelesTrak must not use the fetcher's retries
    let core = LavenderCore::builder(config).cache_dir(cache.path()).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(s.items.len(), 1, "the old elements are still shown");
    match &s.freshness {
        Freshness::Degraded { reason, age_s } => {
            assert!(reason.contains("stations") && reason.contains("5 h"), "{reason}");
            assert!((17_900..18_100).contains(age_s), "{age_s}");
        }
        other => panic!("expected degraded, got {other:?}"),
    }
    assert!(seconds_ago(s.fetched_at.unwrap()) > 17_900);
    let health =
        core.status().sources.into_iter().find(|h| h.source_id == sources::CELESTRAK).unwrap();
    assert_eq!(health.state, HealthState::Degraded);
    assert_eq!(health.last_success, None, "serving a cache is not a successful refresh");
}

#[tokio::test]
async fn celestrak_partial_group_failure_is_degraded() {
    // Review finding 9: one failed group was masked by the groups that succeeded.
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .and(query_param("GROUP", "stations"))
        .respond_with(ResponseTemplate::new(200).set_body_string(OMM))
        .mount(&server)
        .await;
    Mock::given(path("/celestrak/gp.php"))
        .and(query_param("GROUP", "weather"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Invalid query: GROUP"))
        .mount(&server)
        .await;
    let mut config = config_for(&server);
    config.providers.celestrak.groups = vec!["stations".into(), "weather".into()];
    let core = LavenderCore::builder(config).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Satellites)).await.unwrap();
    assert_eq!(s.items.len(), 1);
    assert!(
        matches!(&s.freshness, Freshness::Degraded { reason, .. } if reason.contains("weather") && !reason.contains("stations,")),
        "{:?}",
        s.freshness
    );
}

#[tokio::test]
async fn celestrak_does_not_re_request_a_failed_group_before_its_retry_floor() {
    // Review finding 9: with no cache, failures were retried every few minutes (and three
    // times per attempt) against CelesTrak's one-download-per-two-hours policy.
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = config_for(&server);
    config.fetch.retries = 2;
    let provider = CelestrakProvider::new(config.providers.celestrak.clone());
    assert_eq!(provider.retry_floor(), Duration::from_secs(600));
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    let first = provider.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(first.to_string().contains("503"), "{first}");
    let second = provider.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(second.to_string().contains("next request allowed in"), "{second}");
}
