//! Live-provider timestamp contracts use synthetic, delayed HTTP responses. No test
//! contacts adsb.lol. Seconds/milliseconds and `seen_pos` units follow the upstream docs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::adsb_lol::AdsbLolProvider;
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use lavender_core::{
    CoreError, FeedSubscription, LavenderCore, MemorySecretStore, SubscriptionEvent,
};
use lavender_domain::{BBox, FeedId, FeedItems, FeedQuery, Freshness};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn config(server: &MockServer) -> CoreConfig {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.fetch.retries = 0;
    config.hub.first_result_timeout_s = 3;
    config.hub.linger_s = 0;
    config.providers.adsb_lol.base_url = Url::parse(&format!("{}/", server.uri())).unwrap();
    config.providers.adsb_lol.cadence_s = 3600;
    config.providers.adsb_lol.min_request_interval_ms = 0;
    config
}

fn document(now_ms: i64, lat: f64, lon: f64) -> serde_json::Value {
    serde_json::json!({"now": now_ms,"ac":[
        {"hex":"abcdef","lat":lat,"lon":lon,"seen_pos":2.125,"seen":0.25}
    ]})
}

async fn completed_snapshot(subscription: &mut FeedSubscription) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match subscription.next().await.unwrap() {
                SubscriptionEvent::Snapshot(snapshot) if snapshot.pending_regions == 0 => return,
                SubscriptionEvent::Snapshot(_) => {}
                SubscriptionEvent::Error(error) => panic!("{error:?}"),
            }
        }
    })
    .await
    .unwrap();
}

#[derive(Clone)]
struct DelayedDocument {
    started: Arc<Notify>,
    document: serde_json::Value,
}

impl Respond for DelayedDocument {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.started.notify_one();
        ResponseTemplate::new(200)
            .set_body_json(&self.document)
            .set_delay(Duration::from_millis(500))
    }
}

#[tokio::test]
async fn older_document_arriving_later_cannot_append_or_replace_a_newer_trail() {
    let server = MockServer::start().await;
    let newer_ms = Timestamp::now().as_millisecond() - 30_000;
    let old_started = Arc::new(Notify::new());
    Mock::given(path("/v2/mil"))
        .respond_with(DelayedDocument {
            started: old_started.clone(),
            document: document(newer_ms - 10_000, 1.0, 1.0),
        })
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path_regex(r"^/v2/point/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(document(newer_ms, 3.0, 3.0)))
        .expect(1)
        .mount(&server)
        .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let mut older = core.subscribe(FeedQuery::new(FeedId::Aircraft)).unwrap();
    tokio::time::timeout(Duration::from_secs(2), old_started.notified()).await.unwrap();
    let newer = core
        .snapshot(
            FeedQuery::new(FeedId::Aircraft).with_bbox(BBox::new(0.5, 0.5, 4.5, 4.5).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(newer.fetched_at, Some(Timestamp::from_millisecond(newer_ms).unwrap()));
    let retained = core.aircraft_track("abcdef").unwrap();
    assert_eq!(retained.points.len(), 1);
    assert_eq!(retained.points[0].time, Timestamp::from_millisecond(newer_ms - 2125).unwrap());
    assert!((retained.points[0].position.lat - 3.0).abs() < f64::EPSILON);
    completed_snapshot(&mut older).await;
    assert_eq!(core.aircraft_track("abcdef").unwrap(), retained);
    core.shutdown();
}

#[tokio::test]
async fn unreliable_document_time_preserves_display_rows_but_is_ineligible_for_history() {
    let server = MockServer::start().await;
    let config = config(&server);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let provider = AdsbLolProvider::new(config.providers.adsb_lol);
    let secrets = MemorySecretStore::default();
    let context = FetchContext {
        fetcher: &fetcher,
        secrets: &secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    for now in [
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!("private timestamp")),
        Some(serde_json::json!(-1)),
        Some(serde_json::json!(1e30)),
        Some(serde_json::json!(Timestamp::now().as_millisecond() + 86_400_000)),
    ] {
        let mut body = document(Timestamp::now().as_millisecond() - 1000, 1.0, 1.0);
        if let Some(now) = now {
            body["now"] = now;
        } else {
            body.as_object_mut().unwrap().remove("now");
        }
        Mock::given(path("/v2/mil"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        let answer = provider.fetch(&context, Region::Global).await.unwrap();
        let FeedItems::Aircraft(aircraft) = answer.items else { panic!() };
        assert_eq!(aircraft.len(), 1);
        assert_eq!(answer.as_of, None);
        let reason = answer.degraded.unwrap();
        assert!(reason.contains("history was not updated"));
        assert!(!reason.contains("private timestamp"));
        server.reset().await;
    }
}

#[tokio::test]
async fn a_mixed_response_with_an_unreliable_position_age_refuses_all_history_capture() {
    let server = MockServer::start().await;
    for age in [None, Some(serde_json::json!(-1)), Some(serde_json::json!("unknown"))] {
        let mut body = document(Timestamp::now().as_millisecond() - 1000, 1.0, 1.0);
        let mut bad = serde_json::json!({"hex":"123456","lat":2,"lon":2,"seen":0.0});
        if let Some(age) = age {
            bad["seen_pos"] = age;
        }
        body["ac"].as_array_mut().unwrap().push(bad);
        Mock::given(path("/v2/mil"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        let core = LavenderCore::builder(config(&server)).build().unwrap();
        let snapshot = core.snapshot(FeedQuery::new(FeedId::Aircraft)).await.unwrap();
        let FeedItems::Aircraft(aircraft) = &snapshot.items else { panic!() };
        assert_eq!(aircraft.len(), 2);
        assert!(matches!(snapshot.freshness, Freshness::Degraded { .. }));
        for hex in ["abcdef", "123456"] {
            assert!(matches!(core.aircraft_track(hex), Err(CoreError::NotFound(_))));
        }
        core.shutdown();
        server.reset().await;
    }
}
