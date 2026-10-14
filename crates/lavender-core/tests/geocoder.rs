//! Place contracts use saved documentation examples and synthetic failure cases.
//! No test contacts a live geocoder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lavender_core::LavenderCore;
use lavender_core::config::{CoreConfig, GeocoderConfig, GeocoderFormat};
use lavender_core::fetch::Fetcher;
use lavender_core::search::geocoder::{ConfiguredGeocoder, Geocoder, GeocoderError};
use lavender_domain::{GeoPoint, LicenceMode, PlaceHit, SearchQuery};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const NOMINATIM: &str = include_str!("fixtures/nominatim-london.json");
const PHOTON: &str = include_str!("fixtures/photon-berlin.json");

fn config(server: &MockServer, format: GeocoderFormat) -> CoreConfig {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.fetch.retries = 2; // Geocoder deliberately overrides the fetch retry count.
    config.geocoder = GeocoderConfig {
        enabled: true,
        format,
        base_url: Some(Url::parse(&format!("{}/places/", server.uri())).unwrap()),
        min_request_interval_ms: 1000,
        attribution: Some("Lavender test place service".to_owned()),
        ..GeocoderConfig::default()
    };
    config
}

fn geocoder(config: &CoreConfig) -> ConfiguredGeocoder {
    ConfiguredGeocoder::new(
        config.geocoder.clone(),
        Arc::new(Fetcher::new(config.fetch.clone()).unwrap()),
    )
    .unwrap()
}

fn query(q: &str) -> SearchQuery {
    SearchQuery { q: q.to_owned(), near: None, limit: Some(8), places: Some(true) }
}

#[tokio::test]
async fn nominatim_recorded_contract_groups_places_and_credit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/places/search"))
        .and(query_param("q", "London"))
        .and(query_param("format", "jsonv2"))
        .and(query_param("addressdetails", "1"))
        .and(query_param("limit", "8"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NOMINATIM))
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server, GeocoderFormat::Nominatim)).build().unwrap();
    let results = core.search(&query("London")).await.unwrap();
    assert!(results.coordinates.is_empty() && results.entities.is_empty());
    assert_eq!(results.places.len(), 1);
    let london = &results.places[0];
    assert_eq!(london.name, "London");
    assert_eq!(london.context.as_deref(), Some("England, United Kingdom"));
    assert_eq!(london.kind.as_deref(), Some("city"));
    assert!((london.position.lat - 51.507_321_9).abs() < 1e-9);
    assert!((london.position.lon + 0.127_647_4).abs() < 1e-9);
    let bbox = london.bbox.unwrap();
    assert!((bbox.west + 0.287_647_4).abs() < 1e-9);
    assert!((bbox.north - 51.667_321_9).abs() < 1e-9);
    let source =
        core.catalog().sources.into_iter().find(|source| source.id == london.source_id).unwrap();
    assert!(source.attribution.contains("OpenStreetMap"));
    assert!(results.notices.iter().any(|notice| notice.contains("OpenStreetMap")));
    core.shutdown();
}

#[tokio::test]
async fn photon_recorded_contract_preserves_rank_and_normalizes_extent() {
    let server = MockServer::start().await;
    Mock::given(path("/places/api/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PHOTON))
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server, GeocoderFormat::Photon)).build().unwrap();
    let results = core.search(&query("Berlin")).await.unwrap();
    assert_eq!(results.places.len(), 2);
    assert_eq!(results.places[0].name, "Berlin");
    assert_eq!(results.places[0].context.as_deref(), Some("Germany"));
    assert!((results.places[0].position.lon - 13.388_859_9).abs() < 1e-9);
    let stadium = &results.places[1];
    assert_eq!(stadium.kind.as_deref(), Some("stadium"));
    let bbox = stadium.bbox.unwrap();
    assert!((bbox.south - 52.513_597_2).abs() < 1e-9);
    assert!((bbox.north - 52.515_715_1).abs() < 1e-9);
    core.shutdown();
}

#[tokio::test]
async fn encoded_queries_cannot_inject_parameters_and_near_bias_is_sent() {
    let server = MockServer::start().await;
    let text = "Київ & limit=999?#";
    Mock::given(path("/places/search"))
        .and(query_param("q", text))
        .and(query_param("limit", "8"))
        .and(query_param("viewbox", "23,61,25,59"))
        .and(query_param("bounded", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    let geocoder = geocoder(&config(&server, GeocoderFormat::Nominatim));
    let mut query = query(text);
    query.near = Some(GeoPoint::new(60.0, 24.0).unwrap());
    let results = geocoder.geocode(&query, CancellationToken::new()).await.unwrap();
    assert_eq!(results.places, Vec::<PlaceHit>::new());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].url.query_pairs().filter(|(key, _)| key == "limit").count(), 1);
    assert!(requests[0].url.fragment().is_none());
}

#[tokio::test]
async fn photon_bias_and_limits_are_in_the_cache_key() {
    let server = MockServer::start().await;
    Mock::given(path("/places/api/"))
        .and(query_param("lat", "60"))
        .and(query_param("lon", "24"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PHOTON))
        .expect(2)
        .mount(&server)
        .await;
    let geocoder = geocoder(&config(&server, GeocoderFormat::Photon));
    let mut query = query("Berlin");
    query.near = Some(GeoPoint::new(60.0, 24.0).unwrap());
    query.limit = Some(1);
    assert_eq!(geocoder.geocode(&query, CancellationToken::new()).await.unwrap().places.len(), 1);
    query.limit = Some(2);
    assert_eq!(geocoder.geocode(&query, CancellationToken::new()).await.unwrap().places.len(), 2);
    geocoder.geocode(&query, CancellationToken::new()).await.unwrap();
}

#[tokio::test]
async fn malformed_rows_are_skipped_duplicates_collapse_and_optional_bbox_is_lenient() {
    let server = MockServer::start().await;
    let response = r#"[{"name":"London\u0000","lat":"51.5","lon":"-0.1","boundingbox":["bad"]},{"name":"London","lat":51.5,"lon":-0.1},{"name":"Bad","lat":"NaN","lon":2},{"name":"Wrong","lat":100,"lon":0},null]"#;
    Mock::given(path("/places/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(response))
        .mount(&server)
        .await;
    let geocoder = geocoder(&config(&server, GeocoderFormat::Nominatim));
    let results = geocoder.geocode(&query("London"), CancellationToken::new()).await.unwrap();
    assert_eq!(results.places.len(), 1);
    assert_eq!(results.places[0].name, "London");
    assert!(results.places[0].bbox.is_none());
    assert_eq!(results.notices, vec!["Place search: 3 malformed records skipped."]);
}

#[tokio::test]
async fn empty_answers_are_cached_but_malformed_and_quota_answers_are_not() {
    let server = MockServer::start().await;
    for (q, response, status, count) in [
        ("empty", "[]", 200, 1),
        ("wrong-shape", r#"{"private-search":"secret"}"#, 200, 2),
        ("bad-row", r#"[{"lat":999}]"#, 200, 2),
        ("quota", "private-search secret", 429, 2),
    ] {
        Mock::given(path("/places/search"))
            .and(query_param("q", q))
            .respond_with(ResponseTemplate::new(status).set_body_string(response))
            .expect(count)
            .mount(&server)
            .await;
    }
    let geocoder = geocoder(&config(&server, GeocoderFormat::Nominatim));
    for _ in 0..2 {
        assert_eq!(
            geocoder.geocode(&query("empty"), CancellationToken::new()).await.unwrap().places,
            Vec::<PlaceHit>::new()
        );
        for (q, expected) in [
            ("wrong-shape", GeocoderError::Malformed),
            ("bad-row", GeocoderError::Malformed),
            ("quota", GeocoderError::Busy),
        ] {
            let error = geocoder.geocode(&query(q), CancellationToken::new()).await.unwrap_err();
            assert_eq!(error, expected);
            assert!(!error.to_string().contains("secret"));
            assert!(!error.to_string().contains(q));
        }
    }
}

#[tokio::test]
async fn disabled_local_keystroke_and_coordinate_queries_do_not_fetch() {
    let server = MockServer::start().await;
    let mut config = config(&server, GeocoderFormat::Nominatim);
    config.geocoder.enabled = false;
    let disabled = geocoder(&config);
    assert_eq!(
        disabled.geocode(&query("London"), CancellationToken::new()).await.unwrap_err(),
        GeocoderError::Disabled
    );
    let core = LavenderCore::builder(config.clone()).build().unwrap();
    let results = core.search(&query("London")).await.unwrap();
    assert!(results.notices.iter().any(|notice| notice.contains("not configured")));
    core.shutdown();
    config.geocoder.enabled = true;
    let core = LavenderCore::builder(config).build().unwrap();
    let mut local = query("London");
    local.places = None;
    assert_eq!(core.search(&local).await.unwrap().places, Vec::<PlaceHit>::new());
    for q in ["60.1699, 24.9384", "60°10′11.64″N 24°56′18.24″E", "35V LH 85985 71614"] {
        let results = core.search(&query(q)).await.unwrap();
        assert_eq!(results.coordinates.len(), 1, "{q}");
        assert_eq!(results.places, Vec::<PlaceHit>::new());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    core.shutdown();
}

#[tokio::test]
async fn commercial_service_refusal_makes_no_request_until_acknowledged() {
    let server = MockServer::start().await;
    Mock::given(path("/places/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NOMINATIM))
        .expect(1)
        .mount(&server)
        .await;
    let mut config = config(&server, GeocoderFormat::Nominatim);
    config.licence_mode = LicenceMode::Commercial;
    let core = LavenderCore::builder(config.clone()).build().unwrap();
    let refused = core.search(&query("London")).await.unwrap();
    assert_eq!(refused.places, Vec::<PlaceHit>::new());
    assert!(refused.notices.iter().any(|notice| notice.contains("commercial terms")));
    assert!(server.received_requests().await.unwrap().is_empty());
    core.shutdown();

    config.licence_acknowledged = vec!["osm-geocoder-nominatim".to_owned()];
    let cleared = LavenderCore::builder(config).build().unwrap();
    assert_eq!(cleared.search(&query("London")).await.unwrap().places.len(), 1);
    cleared.shutdown();
}

#[tokio::test]
async fn identical_concurrent_requests_share_one_cache_miss() {
    let server = MockServer::start().await;
    Mock::given(path("/places/search"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(NOMINATIM)
                .set_delay(Duration::from_millis(100)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let geocoder = Arc::new(geocoder(&config(&server, GeocoderFormat::Nominatim)));
    let tasks = (0..12).map(|_| {
        let geocoder = geocoder.clone();
        tokio::spawn(
            async move { geocoder.geocode(&query("London"), CancellationToken::new()).await },
        )
    });
    for result in futures::future::join_all(tasks).await {
        assert_eq!(result.unwrap().unwrap().places.len(), 1);
    }
}

#[derive(Clone)]
struct TimedResponse(Arc<Mutex<Vec<Instant>>>);

impl Respond for TimedResponse {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0.lock().unwrap().push(Instant::now());
        ResponseTemplate::new(200).set_body_string("[]")
    }
}

#[derive(Clone)]
struct SignalledResponse {
    started: Arc<tokio::sync::Notify>,
    delay: Duration,
}

impl Respond for SignalledResponse {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.started.notify_one();
        ResponseTemplate::new(200).set_body_string(NOMINATIM).set_delay(self.delay)
    }
}

#[tokio::test]
async fn pending_misses_are_bounded_and_cancelled_duplicate_does_not_stop_the_leader() {
    let server = MockServer::start().await;
    let started = Arc::new(tokio::sync::Notify::new());
    Mock::given(path("/places/search"))
        .respond_with(SignalledResponse {
            started: started.clone(),
            delay: Duration::from_millis(200),
        })
        .expect(1)
        .mount(&server)
        .await;
    let mut config = config(&server, GeocoderFormat::Nominatim);
    config.geocoder.max_pending = 1;
    let geocoder = Arc::new(geocoder(&config));
    let leader = {
        let geocoder = geocoder.clone();
        tokio::spawn(
            async move { geocoder.geocode(&query("London"), CancellationToken::new()).await },
        )
    };
    started.notified().await;
    assert_eq!(
        geocoder.geocode(&query("other"), CancellationToken::new()).await.unwrap_err(),
        GeocoderError::Busy
    );
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let duplicate = query("London");
    let (waiter, ()) = tokio::join!(geocoder.geocode(&duplicate, cancel), async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        trigger.cancel();
    });
    assert_eq!(waiter.unwrap_err(), GeocoderError::Cancelled);
    assert_eq!(leader.await.unwrap().unwrap().places.len(), 1);
    assert_eq!(
        geocoder.geocode(&query("London"), CancellationToken::new()).await.unwrap().places.len(),
        1
    );
}

#[tokio::test]
async fn cancellation_during_fetch_is_prompt_and_does_not_cache_an_answer() {
    let server = MockServer::start().await;
    let started = Arc::new(tokio::sync::Notify::new());
    Mock::given(path("/places/search"))
        .respond_with(SignalledResponse {
            started: started.clone(),
            delay: Duration::from_secs(10),
        })
        .expect(1)
        .mount(&server)
        .await;
    let geocoder = Arc::new(geocoder(&config(&server, GeocoderFormat::Nominatim)));
    let cancel = CancellationToken::new();
    let request = {
        let geocoder = geocoder.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { geocoder.geocode(&query("London"), cancel).await })
    };
    started.notified().await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_millis(200), request).await.unwrap();
    assert_eq!(result.unwrap().unwrap_err(), GeocoderError::Cancelled);
    server.reset().await;
    Mock::given(path("/places/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NOMINATIM))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        geocoder.geocode(&query("London"), CancellationToken::new()).await.unwrap().places.len(),
        1
    );
}

#[tokio::test]
async fn independent_instances_and_fetchers_share_one_second_host_pacing() {
    let server = MockServer::start().await;
    let sent = Arc::new(Mutex::new(Vec::new()));
    Mock::given(path("/places/search"))
        .respond_with(TimedResponse(sent.clone()))
        .expect(2)
        .mount(&server)
        .await;
    let first = geocoder(&config(&server, GeocoderFormat::Nominatim));
    let second = geocoder(&config(&server, GeocoderFormat::Nominatim));
    let first_query = query("one");
    let second_query = query("two");
    let (first, second) = tokio::join!(
        first.geocode(&first_query, CancellationToken::new()),
        second.geocode(&second_query, CancellationToken::new()),
    );
    first.unwrap();
    second.unwrap();
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert!(sent[1].duration_since(sent[0]) >= Duration::from_secs(1));
}

#[tokio::test]
async fn cache_expires_and_capacity_evicts_answers() {
    let server = MockServer::start().await;
    Mock::given(path("/places/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    let mut config = config(&server, GeocoderFormat::Nominatim);
    config.geocoder.cache_ttl_s = 1;
    config.geocoder.cache_max_entries = 1;
    let cached = geocoder(&config);
    cached.geocode(&query("one"), CancellationToken::new()).await.unwrap();
    cached.geocode(&query("one"), CancellationToken::new()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    tokio::time::sleep(Duration::from_millis(1050)).await;
    cached.geocode(&query("one"), CancellationToken::new()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // A longer TTL isolates capacity eviction from expiry.
    config.geocoder.cache_ttl_s = 300;
    let bounded = geocoder(&config);
    for q in ["alpha", "beta", "gamma", "alpha", "beta", "gamma"] {
        bounded.geocode(&query(q), CancellationToken::new()).await.unwrap();
    }
    assert!(server.received_requests().await.unwrap().len() >= 6);
}

#[tokio::test]
async fn cancellation_prevents_queued_and_already_cancelled_requests() {
    let server = MockServer::start().await;
    Mock::given(path("/places/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    let geocoder = geocoder(&config(&server, GeocoderFormat::Nominatim));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        geocoder.geocode(&query("never"), cancel).await.unwrap_err(),
        GeocoderError::Cancelled
    );
    geocoder.geocode(&query("warm"), CancellationToken::new()).await.unwrap();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let queued = query("queued");
    let operation = geocoder.geocode(&queued, cancel);
    let (result, ()) = tokio::join!(operation, async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        trigger.cancel();
    });
    assert_eq!(result.unwrap_err(), GeocoderError::Cancelled);
}
