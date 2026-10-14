//! Synthetic NOAA schema fixtures exercised through real Fetcher/provider/core paths.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::cyclones::CyclonesProvider;
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use lavender_core::{LavenderCore, MemorySecretStore};
use lavender_domain::{
    BBox, Coverage, CycloneGeometryStatus, FeedId, FeedItems, FeedQuery, Freshness, SearchQuery,
    Timestamp,
};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STATUS: &str = include_str!("fixtures/cyclones-status.json");
const POINTS: &str = include_str!("fixtures/cyclones-points.json");
const TRACK: &str = include_str!("fixtures/cyclones-track.json");
const CONE: &str = include_str!("fixtures/cyclones-cone.json");

fn config(server: &MockServer) -> CoreConfig {
    let mut c = CoreConfig::default();
    c.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    c.fetch.retries = 2; // Provider must override automatic retries.
    c.hub.linger_s = 0;
    c.providers.cyclones.status_base_url = Url::parse(&format!("{}/nhc/", server.uri())).unwrap();
    c.providers.cyclones.gis_base_url = Url::parse(&format!("{}/gis/", server.uri())).unwrap();
    c
}

fn status_now() -> String {
    let mut document: serde_json::Value = serde_json::from_str(STATUS).unwrap();
    let now = Timestamp::from_second(Timestamp::now().as_second()).unwrap();
    document["activeStorms"][0]["lastUpdate"] = serde_json::json!(now.to_string());
    document["activeStorms"][0]["forecastAdvisory"]["issuance"] =
        serde_json::json!(Timestamp::from_second(now.as_second() - 3 * 3600).unwrap().to_string());
    serde_json::to_string(&document).unwrap()
}

async fn mount(server: &MockServer, status: ResponseTemplate, geometry: [&str; 3]) {
    Mock::given(method("GET"))
        .and(path("/nhc/CurrentStorms.json"))
        .respond_with(status)
        .expect(1)
        .mount(server)
        .await;
    for (i, body) in geometry.into_iter().enumerate() {
        Mock::given(method("GET"))
            .and(path(format!("/gis/{}/query", i + 5)))
            .and(query_param("outSR", "4326"))
            .and(query_param("f", "geojson"))
            .and(query_param("resultRecordCount", if i == 0 { "500" } else { "32" }))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn active_storm_survives_generic_core_filter_search_and_catalog() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(200).set_body_string(status_now()), [POINTS, TRACK, CONE])
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Cyclones).unwrap();
    assert_eq!(layer.cadence_s, 600);
    assert!(!layer.default_on);
    for _ in 0..2 {
        let s = core.snapshot(FeedQuery::new(FeedId::Cyclones)).await.unwrap();
        assert_eq!(s.freshness, Freshness::Live);
        assert!(matches!(s.coverage, Coverage::Partial { .. }));
        assert_eq!(s.sources, vec!["noaa-nhc-cphc"]);
        assert!(s.notices.iter().any(|n| n.contains("not the extent")));
        let FeedItems::Cyclones(list) = s.items else { panic!("cyclones") };
        assert_eq!(list[0].geometry_status, CycloneGeometryStatus::Current);
        assert_eq!(list[0].forecast_points.len(), 2);
        assert!(list[0].cone.is_some());
    }
    let s = core
        .snapshot(
            FeedQuery::new(FeedId::Cyclones).with_bbox(BBox::new(0.0, 0.0, 10.0, 10.0).unwrap()),
        )
        .await
        .unwrap();
    assert!(s.items.is_empty());
    assert_eq!(s.source_count, 1);
    let result = core
        .search(&SearchQuery { q: "alpha".to_owned(), near: None, limit: None, places: None })
        .await
        .unwrap();
    assert_eq!(result.entities[0].feed, FeedId::Cyclones);
    assert_eq!(result.entities[0].id, "al062026");
    core.shutdown();
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        4,
        "repeat snapshots share one bounded refresh"
    );
}

#[tokio::test]
async fn empty_season_is_healthy_and_never_requests_gis() {
    let server = MockServer::start().await;
    Mock::given(path("/nhc/CurrentStorms.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(include_str!("fixtures/cyclones-empty.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Cyclones)).await.unwrap();
    assert_eq!(s.freshness, Freshness::Live);
    assert!(s.items.is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    core.shutdown();
}

#[tokio::test]
async fn malformed_status_is_unavailable_and_cannot_trigger_gis() {
    let server = MockServer::start().await;
    Mock::given(path("/nhc/CurrentStorms.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(include_str!("fixtures/cyclones-malformed.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Cyclones)).await.unwrap();
    assert!(matches!(s.freshness, Freshness::Unavailable { .. }));
    assert!(s.items.is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    core.shutdown();
}

#[tokio::test]
async fn invalid_geometry_keeps_positions_and_honest_degraded_status() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_string(status_now()),
        [POINTS, TRACK, "<html>unavailable</html>"],
    )
    .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Cyclones)).await.unwrap();
    assert!(matches!(s.freshness, Freshness::Degraded { .. }));
    let FeedItems::Cyclones(storms) = s.items else { panic!() };
    assert_eq!(storms.len(), 1);
    assert_eq!(storms[0].geometry_status, CycloneGeometryStatus::Unavailable);
    assert_eq!(storms[0].track, None);
    assert_eq!(storms[0].intensity_kt, Some(80.0));
    core.shutdown();
}

#[tokio::test]
async fn earlier_advisory_geometry_is_pending_and_degraded() {
    let server = MockServer::start().await;
    let track = TRACK.replace("006", "005");
    mount(
        &server,
        ResponseTemplate::new(200).set_body_string(status_now()),
        [POINTS, &track, CONE],
    )
    .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Cyclones)).await.unwrap();
    assert!(matches!(s.freshness, Freshness::Degraded { .. }));
    let FeedItems::Cyclones(storms) = s.items else { panic!() };
    assert_eq!(storms[0].geometry_status, CycloneGeometryStatus::Pending);
    assert_eq!(storms[0].geometry_advisory_number, None);
    core.shutdown();
}

#[tokio::test]
async fn failed_refresh_is_not_retried_in_burst_even_with_fetch_retries_configured() {
    let server = MockServer::start().await;
    Mock::given(path("/nhc/CurrentStorms.json"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let c = config(&server);
    let provider = CyclonesProvider::new(c.providers.cyclones);
    let fetcher = Fetcher::new(c.fetch).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    assert_eq!(provider.retry_floor(), Duration::from_secs(600));
    for _ in 0..3 {
        assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    }
}

#[tokio::test]
async fn concurrent_fetches_share_bounded_refresh_and_keep_original_fetch_time() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200)
            .set_body_string(status_now())
            .set_delay(Duration::from_millis(50)),
        [POINTS, TRACK, CONE],
    )
    .await;
    let c = config(&server);
    let provider = Arc::new(CyclonesProvider::new(c.providers.cyclones));
    let fetcher = Fetcher::new(c.fetch).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    let (first, second) =
        tokio::join!(provider.fetch(&ctx, Region::Global), provider.fetch(&ctx, Region::Global));
    assert_eq!(first.unwrap(), second.unwrap());
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn cancellation_releases_refresh_and_does_not_consume_retry_budget() {
    let server = MockServer::start().await;
    Mock::given(path("/nhc/CurrentStorms.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(include_str!("fixtures/cyclones-empty.json"))
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&server)
        .await;
    let c = config(&server);
    let provider = CyclonesProvider::new(c.providers.cyclones);
    let fetcher = Fetcher::new(c.fetch).unwrap();
    let secrets = MemorySecretStore::default();
    let token = CancellationToken::new();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: token.clone(),
        cache_dir: None,
    };
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        token.cancel();
    };
    let (answer, ()) = tokio::join!(provider.fetch(&ctx, Region::Global), cancel);
    assert!(answer.is_err());
    let ctx = FetchContext { cancel: CancellationToken::new(), ..ctx };
    assert!(provider.fetch(&ctx, Region::Global).await.unwrap().items.is_empty());
}
