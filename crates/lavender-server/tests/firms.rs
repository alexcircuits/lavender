//! The FIRMS feed over HTTP: the worldwide answer stays under 1 MB as served, and the
//! `MAP_KEY` (part of every upstream URL path) never leaves the server in a response,
//! notice, snapshot, status report or log line. Synthetic data only; no live NASA calls.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::config::CoreConfig;
use lavender_core::{LavenderCore, MemorySecretStore, SecretKey};
use lavender_domain::{FeedId, FeedQuery, Timestamp};
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MAP_KEY: &str = "mapkey-sentinel-7f3a9c0e";
const HEADER: &str =
    "latitude,longitude,bright_ti4,acq_date,acq_time,satellite,confidence,frp,daynight\n";

fn core_for(upstream: &MockServer, sources: &[&str]) -> LavenderCore {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    // FIRMS downloads have their own timeout; the SLOW source below must run into it.
    config.providers.firms.request_timeout_s = 1;
    config.hub.linger_s = 0;
    config.hub.first_result_timeout_s = 20;
    config.providers.firms.base_url = Url::parse(&format!("{}/firms/", upstream.uri())).unwrap();
    config.providers.firms.sources = sources.iter().map(|s| (*s).to_owned()).collect();
    LavenderCore::builder(config)
        .secrets(Arc::new(MemorySecretStore::with([(SecretKey::FirmsMapKey, MAP_KEY)])))
        .build()
        .unwrap()
}

async fn mount(upstream: &MockServer, source: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/firms/{MAP_KEY}/{source}/world/2")))
        .respond_with(response)
        .expect(1)
        .mount(upstream)
        .await;
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String) {
    let request =
        Request::get(uri).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn acquired_now() -> (String, String) {
    let now = Timestamp::now();
    (now.strftime("%Y-%m-%d").to_string(), now.strftime("%H%M").to_string())
}

#[tokio::test]
async fn worldwide_http_answer_is_at_most_one_megabyte_with_every_detection() {
    let upstream = MockServer::start().await;
    let (date, time) = acquired_now();
    // Every 1° cell occupied plus a 20,000-detection hot spot, with the longest
    // metadata the parser keeps in 4-byte characters (the most JSON per cell).
    let wide = "𐀀".repeat(16);
    let mut csv = String::from(HEADER);
    let mut detections = 0_u64;
    for lat in -90..90 {
        for lon in -180..180 {
            let (lat, lon) = (f64::from(lat) + 0.123_456_789, f64::from(lon) + 0.987_654_321);
            writeln!(csv, "{lat},{lon},330,{date},{time},{wide},{wide},12.5,{wide}").unwrap();
            detections += 1;
        }
    }
    for i in 0..20_000_u32 {
        let (lat, lon) = (-15.0 + f64::from(i % 100) * 0.001, 130.0 + f64::from(i / 100) * 0.001);
        writeln!(csv, "{lat},{lon},330,{date},{time},{wide},{wide},1.7976931348623157e308,{wide}")
            .unwrap();
        detections += 1;
    }
    mount(&upstream, "VIIRS_NOAA20_NRT", ResponseTemplate::new(200).set_body_string(csv)).await;
    let core = core_for(&upstream, &["VIIRS_NOAA20_NRT"]);
    let app = lavender_server::app(core.clone(), &ServerSettings::default());
    for uri in ["/api/v1/feeds/fires", "/api/v1/feeds/fires?bbox=-180,-90,180,90"] {
        let (status, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::OK);
        println!("{uri}: {} bytes", body.len());
        assert!(body.len() <= 1_000_000, "{uri}: {} bytes", body.len());
        let snapshot: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(snapshot["freshness"]["state"], "live");
        let counted: u64 = snapshot["items"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell["count"].as_u64().unwrap())
            .sum();
        assert_eq!(counted, detections, "{uri}");
    }
    core.shutdown();
}

#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl LogBuffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

#[tokio::test]
async fn map_key_never_reaches_responses_notices_snapshots_status_or_logs() {
    // The test runtime is single-threaded, so this thread-local subscriber also sees the
    // hub's refresh tasks and the HTTP client, at every level.
    let logs = LogBuffer::default();
    let writer = logs.clone();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish(),
    );
    let upstream = MockServer::start().await;
    let (date, time) = acquired_now();
    let echo = format!("Invalid MAP_KEY {MAP_KEY}");
    // Upstreams that echo the key back in every way they could.
    mount(&upstream, "ECHO_ERROR", ResponseTemplate::new(500).set_body_string(echo.clone())).await;
    mount(&upstream, "ECHO_TEXT", ResponseTemplate::new(200).set_body_string(echo.clone())).await;
    let elsewhere = format!("https://elsewhere.invalid/api/area/csv/{MAP_KEY}/X/world/2");
    mount(&upstream, "REDIRECT", ResponseTemplate::new(302).insert_header("Location", elsewhere))
        .await;
    let slow = ResponseTemplate::new(200).set_body_string(echo).set_delay(Duration::from_secs(3));
    mount(&upstream, "SLOW", slow).await;
    let csv = format!(
        "{HEADER}10,20,330,{date},{time},N20,n,5,D\n{MAP_KEY},20,330,{date},{time},{MAP_KEY},n,5,D\n"
    );
    mount(&upstream, "DATA", ResponseTemplate::new(200).set_body_string(csv)).await;

    let partial = core_for(&upstream, &["ECHO_ERROR", "ECHO_TEXT", "REDIRECT", "SLOW", "DATA"]);
    let subscription = partial.subscribe(FeedQuery::new(FeedId::Fires)).unwrap();
    let app = lavender_server::app(partial.clone(), &ServerSettings::default());
    let mut seen = Vec::new();
    for uri in [
        "/api/v1/feeds/fires",
        "/api/v1/feeds/fires?bbox=19,9,21,11",
        "/api/v1/feeds/fires?minFrp=not-a-number",
        "/api/v1/status",
        "/api/v1/catalog",
    ] {
        let (status, body) = get(&app, uri).await;
        seen.push(format!("{uri} → {status}: {body}"));
    }
    let (_, snapshot) = get(&app, "/api/v1/feeds/fires").await;
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["freshness"]["state"], "degraded", "{snapshot}");
    assert_eq!(snapshot["sourceCount"], 1, "only the valid DATA row is served");
    assert!(snapshot["notices"][0].as_str().unwrap().contains("1 malformed record skipped"));
    seen.push(format!("{:?}", subscription.latest()));
    seen.push(format!("{partial:?}"));
    drop(subscription);

    // With every source failing there is no data, only reasons.
    let failing_upstream = MockServer::start().await;
    let echo = format!("Invalid MAP_KEY {MAP_KEY}");
    mount(
        &failing_upstream,
        "ECHO_ERROR",
        ResponseTemplate::new(500).set_body_string(echo.clone()),
    )
    .await;
    mount(&failing_upstream, "ECHO_TEXT", ResponseTemplate::new(200).set_body_string(echo)).await;
    let failing = core_for(&failing_upstream, &["ECHO_ERROR", "ECHO_TEXT"]);
    let app = lavender_server::app(failing.clone(), &ServerSettings::default());
    for uri in ["/api/v1/feeds/fires", "/api/v1/status"] {
        let (status, body) = get(&app, uri).await;
        seen.push(format!("{uri} (all failing) → {status}: {body}"));
    }
    assert!(seen.iter().any(|s| s.contains("unavailable")), "{seen:#?}");

    // The key did travel upstream in every request path ...
    let mut requests = upstream.received_requests().await.unwrap();
    requests.extend(failing_upstream.received_requests().await.unwrap());
    assert_eq!(requests.len(), 7);
    assert!(requests.iter().all(|r| r.url.path().contains(MAP_KEY)));
    // ... and nowhere else.
    for text in &seen {
        assert!(!text.contains(MAP_KEY), "the key leaked: {text}");
    }
    let logs = logs.text();
    assert!(logs.contains("FIRMS fetch failed") && logs.contains("FIRMS parse failed"), "{logs}");
    assert!(logs.contains("did not respond in time"), "the SLOW source timed out: {logs}");
    assert!(!logs.contains(MAP_KEY), "the key leaked into the logs");
    partial.shutdown();
    failing.shutdown();
}
