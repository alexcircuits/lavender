//! Recorded WFIGS and InciWeb documents served by Wiremock, exercised through the real
//! Fetcher, provider, hub and core. Provenance: tests/fixtures/perimeters-provenance.md.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::perimeters::{
    COVERAGE_NOTICE, LINKS_UNAVAILABLE_NOTICE, MAX_RESPONSE_BYTES, MAX_RESPONSE_VERTICES,
    PerimetersProvider, TRUNCATED_NOTICE,
};
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use lavender_core::{CoreError, MemorySecretStore};
use lavender_domain::{
    Availability, BBox, Coverage, FeedId, FeedItems, FeedQuery, FeedSnapshot, FirePerimeter,
    Freshness, LicenceMode, SearchQuery,
};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const WFIGS: &str = include_str!("fixtures/perimeters-wfigs.geojson");
const INDEX: &str = include_str!("fixtures/perimeters-inciweb-index.json");
const PAGES: [(&str, &str); 3] = [
    ("mtbdf-sand-creek", include_str!("fixtures/perimeters-inciweb-mtbdf-sand-creek.html")),
    (
        "cogmf-gold-mountain-fire",
        include_str!("fixtures/perimeters-inciweb-cogmf-gold-mountain-fire.html"),
    ),
    (
        "wanes-sinlahekin-fire",
        include_str!("fixtures/perimeters-inciweb-wanes-sinlahekin-fire.html"),
    ),
];

fn config(server: &MockServer) -> CoreConfig {
    let mut c = CoreConfig::default();
    c.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    c.fetch.retries = 2; // The provider must override automatic retries.
    c.hub.linger_s = 0;
    c.providers.perimeters.wfigs_base_url =
        Url::parse(&format!("{}/wfigs/FeatureServer/0/", server.uri())).unwrap();
    c.providers.perimeters.inciweb_base_url =
        Url::parse(&format!("{}/inciweb/", server.uri())).unwrap();
    c
}

async fn mount_wfigs(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/wfigs/FeatureServer/0/query"))
        .and(query_param("f", "geojson"))
        .and(query_param("outSR", "4326"))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount_inciweb(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/inciweb/api/map_data"))
        .respond_with(ResponseTemplate::new(200).set_body_string(INDEX))
        .mount(server)
        .await;
    for (slug, html) in PAGES {
        Mock::given(method("GET"))
            .and(path(format!("/inciweb/incident-information/{slug}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(html))
            .mount(server)
            .await;
    }
}

fn perimeters(snapshot: FeedSnapshot) -> Vec<FirePerimeter> {
    let FeedItems::Perimeters(items) = snapshot.items else { panic!("perimeters") };
    items
}

async fn requests_to(server: &MockServer, prefix: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().starts_with(prefix))
        .count()
}

#[tokio::test]
async fn recorded_season_arrives_simplified_linked_searchable_and_credited() {
    let server = MockServer::start().await;
    mount_wfigs(&server, ResponseTemplate::new(200).set_body_string(WFIGS)).await;
    mount_inciweb(&server).await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();

    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Perimeters).unwrap();
    assert_eq!((layer.cadence_s, layer.default_on), (300, false));
    assert_eq!(layer.source_ids, ["nifc-wfigs", "inciweb"]);
    assert_eq!(layer.availability, Availability::Available);
    let catalog = core.catalog();
    let wfigs = catalog.sources.iter().find(|s| s.id == "nifc-wfigs").unwrap();
    assert!(wfigs.attribution.contains("National Interagency Fire Center"));
    assert!(!wfigs.requires_key);

    let world = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(world.freshness, Freshness::Live);
    assert_eq!(world.sources, ["nifc-wfigs"]);
    assert_eq!(world.source_count, 7);
    assert!(world.notices.iter().any(|n| n == COVERAGE_NOTICE));
    assert!(
        matches!(&world.coverage, Coverage::Partial { reason } if reason == COVERAGE_NOTICE),
        "a US-only source never covers the whole view: {:?}",
        world.coverage
    );
    assert!(!world.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE));
    assert!(serde_json::to_vec(&world.items).unwrap().len() <= MAX_RESPONSE_BYTES);
    let items = perimeters(world);
    assert_eq!(items.len(), 7);
    assert!(items.iter().all(|p| p.detail.level == 3), "the whole world gets coarse outlines");
    let linked: Vec<(&str, &str)> =
        items.iter().filter_map(|p| Some((p.id.as_str(), p.inciweb_url.as_deref()?))).collect();
    assert_eq!(linked.len(), 3, "{linked:?}");
    let sand = items.iter().find(|p| p.id == "2026-MTBDF-266319").unwrap();
    let expected = format!("{}/inciweb/incident-information/mtbdf-sand-creek", server.uri());
    assert_eq!(sand.inciweb_url.as_deref(), Some(expected.as_str()));

    let close = BBox::new(-113.4, 45.5, -113.2, 45.6).unwrap();
    let detail = core.snapshot(FeedQuery::new(FeedId::Perimeters).with_bbox(close)).await.unwrap();
    let detail = perimeters(detail);
    assert_eq!(detail.len(), 1);
    assert_eq!(detail[0].detail.level, 0, "a close view gets the published outline");
    assert!(detail[0].inciweb_url.is_some());

    let hits = core
        .search(&SearchQuery { q: "sand creek".to_owned(), near: None, limit: None, places: None })
        .await
        .unwrap();
    let hit = hits.entities.iter().find(|h| h.feed == FeedId::Perimeters).unwrap();
    assert_eq!(hit.id, "2026-MTBDF-266319");
    core.shutdown();

    // One WFIGS page; one index; one page per matched incident (Rowe Creek's page is not
    // recorded and answers 404, so Camel Hump stays unlinked).
    assert_eq!(requests_to(&server, "/wfigs/").await, 1, "views share one refresh");
    assert_eq!(requests_to(&server, "/inciweb/api/").await, 1);
    assert_eq!(requests_to(&server, "/inciweb/incident-information/").await, 4);
}

#[tokio::test]
async fn inciweb_outage_only_loses_the_links() {
    let server = MockServer::start().await;
    mount_wfigs(&server, ResponseTemplate::new(200).set_body_string(WFIGS)).await;
    Mock::given(path("/inciweb/api/map_data"))
        .respond_with(ResponseTemplate::new(403).set_body_string("<html>403 Forbidden</html>"))
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(s.freshness, Freshness::Live);
    assert!(s.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE));
    let items = perimeters(s);
    assert_eq!(items.len(), 7);
    assert!(items.iter().all(|p| p.inciweb_url.is_none()));
    core.shutdown();
    assert_eq!(requests_to(&server, "/inciweb/incident-information/").await, 0);
}

#[tokio::test]
async fn changed_or_oversized_incident_pages_only_lose_the_links() {
    let server = MockServer::start().await;
    mount_wfigs(&server, ResponseTemplate::new(200).set_body_string(WFIGS)).await;
    Mock::given(path("/inciweb/api/map_data"))
        .respond_with(ResponseTemplate::new(200).set_body_string(INDEX))
        .mount(&server)
        .await;
    Mock::given(path("/inciweb/incident-information/mtbdf-sand-creek"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(3 * 1024 * 1024)))
        .mount(&server)
        .await;
    Mock::given(path("/inciweb/incident-information/cogmf-gold-mountain-fire"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>redesigned</html>"))
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(s.freshness, Freshness::Live);
    assert!(perimeters(s).iter().all(|p| p.inciweb_url.is_none()));
    core.shutdown();
}

#[tokio::test]
async fn unusable_wfigs_documents_are_unavailable_and_never_reach_inciweb() {
    for (status, body, reason) in [
        (200, "<html>maintenance</html>", "unexpected shape"),
        (200, include_str!("fixtures/perimeters-wfigs-error.json"), "error 400"),
        (200, r#"{"error":{"code":429,"message":"Too many requests"}}"#, "error 429"),
        (429, "", "429"),
    ] {
        let server = MockServer::start().await;
        let response =
            ResponseTemplate::new(status).set_body_string(body).insert_header("Retry-After", "1");
        mount_wfigs(&server, response).await;
        mount_inciweb(&server).await;
        let core = LavenderCore::builder(config(&server)).build().unwrap();
        let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
        let Freshness::Unavailable { reason: actual } = &s.freshness else {
            panic!("{status} {body}: {:?}", s.freshness)
        };
        assert!(actual.contains(reason), "{actual}");
        assert!(!actual.contains("Too many requests"), "upstream text is not echoed");
        assert!(s.items.is_empty());
        core.shutdown();
        assert_eq!(requests_to(&server, "/wfigs/").await, 1, "no burst retries ({status})");
        assert_eq!(requests_to(&server, "/inciweb/").await, 0);
    }
}

#[tokio::test]
async fn an_empty_season_is_live_and_empty() {
    let server = MockServer::start().await;
    mount_wfigs(
        &server,
        ResponseTemplate::new(200)
            .set_body_string(include_str!("fixtures/perimeters-wfigs-empty.geojson")),
    )
    .await;
    mount_inciweb(&server).await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!((s.freshness, s.source_count), (Freshness::Live, 0));
    core.shutdown();
    assert_eq!(requests_to(&server, "/inciweb/incident-information/").await, 0);
}

#[tokio::test]
async fn malformed_shapes_are_skipped_with_a_notice() {
    let server = MockServer::start().await;
    mount_wfigs(
        &server,
        ResponseTemplate::new(200)
            .set_body_string(include_str!("fixtures/perimeters-wfigs-malformed.geojson")),
    )
    .await;
    let mut c = config(&server);
    c.providers.perimeters.inciweb_enabled = false;
    let core = LavenderCore::builder(c).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(s.freshness, Freshness::Live);
    assert_eq!(s.source_count, 3);
    assert!(
        s.notices.iter().any(|n| n.contains("10 perimeter record(s) skipped")),
        "{:?}",
        s.notices
    );
    assert!(!s.notices.iter().any(|n| n == LINKS_UNAVAILABLE_NOTICE), "InciWeb is off");
    core.shutdown();
    assert_eq!(requests_to(&server, "/inciweb/").await, 0);
}

#[tokio::test]
async fn a_view_cut_short_keeps_the_largest_fires_and_says_so() {
    let server = MockServer::start().await;
    mount_wfigs(&server, ResponseTemplate::new(200).set_body_string(WFIGS)).await;
    let mut c = config(&server);
    c.providers.perimeters.inciweb_enabled = false;
    c.hub.max_items = 2;
    let core = LavenderCore::builder(c).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(s.source_count, 7);
    assert!(s.notices.iter().any(|n| n == TRUNCATED_NOTICE), "{:?}", s.notices);
    let ids: Vec<String> = perimeters(s).into_iter().map(|p| p.id).collect();
    assert_eq!(ids, ["2026-WANES-001791", "2026-COGMF-000099"]);
    core.shutdown();
}

/// A WFIGS page of `fires` synthetic star-shaped fires of `corners` corners each, fire
/// `i` being `1000 + i` acres. Their spikes are about 6 km deep, so even 2.5 km
/// simplification keeps most corners.
fn spiky_season(fires: u32, corners: u32) -> String {
    let features: Vec<String> = (0..fires)
        .map(|i| {
            let (lon, lat) = (-120.0 + f64::from(i % 12), 35.0 + f64::from(i / 12));
            let ring: Vec<String> = (0..=corners)
                .map(|c| {
                    let angle = f64::from(c % corners) / f64::from(corners) * std::f64::consts::TAU;
                    let radius = if c % 2 == 0 { 0.36 } else { 0.30 };
                    format!("[{:.5},{:.5}]", lon + radius * angle.cos(), lat + radius * angle.sin())
                })
                .collect();
            format!(
                r#"{{"type":"Feature","properties":{{"attr_UniqueFireIdentifier":"2099-SPIKE-{i:06}","poly_IncidentName":"Spike {i}","attr_IncidentSize":{}}},"geometry":{{"type":"Polygon","coordinates":[[{}]]}}}}"#,
                1000 + i,
                ring.join(",")
            )
        })
        .collect();
    format!(r#"{{"type":"FeatureCollection","features":[{}]}}"#, features.join(","))
}

#[tokio::test]
async fn a_view_over_its_size_limits_serves_the_largest_fires_that_fit_and_says_so() {
    // 120 fires of 1200 corners that no level can simplify: about 3 MB of outlines even
    // at 2.5 km detail, over the 2 MiB a snapshot may carry.
    let server = MockServer::start().await;
    mount_wfigs(&server, ResponseTemplate::new(200).set_body_string(spiky_season(120, 1_200)))
        .await;
    let mut c = config(&server);
    c.providers.perimeters.inciweb_enabled = false;
    let core = LavenderCore::builder(c).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    core.shutdown();
    assert_eq!((&s.freshness, s.source_count), (&Freshness::Live, 120));
    assert!(s.notices.iter().any(|n| n == TRUNCATED_NOTICE), "{:?}", s.notices);
    let bytes = serde_json::to_vec(&s.items).unwrap().len();
    assert!(bytes <= MAX_RESPONSE_BYTES, "{bytes} bytes");
    let items = perimeters(s);
    assert!((50..120).contains(&items.len()), "{} fires", items.len());
    assert!(items.iter().all(|p| p.detail.level == 3), "the coarsest outlines");
    let positions: usize =
        items.iter().flat_map(|p| p.polygons.iter().flatten()).map(Vec::len).sum();
    assert!(positions <= MAX_RESPONSE_VERTICES);
    let largest: Vec<String> =
        (0..120).rev().take(items.len()).map(|i| format!("2099-SPIKE-{i:06}")).collect();
    assert_eq!(items.iter().map(|p| p.id.clone()).collect::<Vec<_>>(), largest);
}

#[tokio::test]
async fn transfer_limit_pages_are_followed_by_offset() {
    let mut document: serde_json::Value = serde_json::from_str(WFIGS).unwrap();
    let features = document["features"].as_array().unwrap().clone();
    document["features"] = serde_json::json!(features[..4]);
    document["properties"] = serde_json::json!({ "exceededTransferLimit": true });
    let first = document.to_string();
    document["features"] = serde_json::json!(features[4..]);
    document["properties"] = serde_json::json!({});
    let second = document.to_string();

    let server = MockServer::start().await;
    for (offset, body) in [("0", first), ("4", second)] {
        Mock::given(path("/wfigs/FeatureServer/0/query"))
            .and(query_param("resultOffset", offset))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    let mut c = config(&server);
    c.providers.perimeters.inciweb_enabled = false;
    let core = LavenderCore::builder(c).build().unwrap();
    let s = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap();
    assert_eq!(s.source_count, 7);
    core.shutdown();
}

#[tokio::test]
async fn disabled_provider_makes_no_requests_and_commercial_mode_allows_the_layer() {
    let server = MockServer::start().await;
    let mut c = config(&server);
    c.providers.perimeters.enabled = false;
    let core = LavenderCore::builder(c).build().unwrap();
    let err = core.snapshot(FeedQuery::new(FeedId::Perimeters)).await.unwrap_err();
    assert!(matches!(err, CoreError::Unconfigured { .. }), "{err}");
    core.shutdown();
    assert!(server.received_requests().await.unwrap().is_empty());

    let mut c = config(&server);
    c.licence_mode = LicenceMode::Commercial;
    let core = LavenderCore::builder(c).build().unwrap();
    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Perimeters).unwrap();
    assert_eq!(layer.availability, Availability::Available);
    core.shutdown();
}

#[tokio::test]
async fn cancellation_releases_the_refresh_without_spending_the_cadence() {
    let server = MockServer::start().await;
    mount_wfigs(
        &server,
        ResponseTemplate::new(200).set_body_string(WFIGS).set_delay(Duration::from_millis(150)),
    )
    .await;
    let c = config(&server);
    let provider = PerimetersProvider::new(lavender_core::config::PerimetersConfig {
        inciweb_enabled: false,
        ..c.providers.perimeters
    });
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
    assert_eq!(provider.fetch(&ctx, Region::Global).await.unwrap().items.len(), 28);
    assert_eq!(provider.retry_floor(), Duration::from_secs(300));
}
