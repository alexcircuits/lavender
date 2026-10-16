//! Regression tests for the parent module.

use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config() -> FetchConfig {
    FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        retries: 2,
        request_timeout_s: 2,
        ..FetchConfig::default()
    }
}

fn policy(server: &MockServer) -> HostPolicy {
    HostPolicy::for_url(&Url::parse(&server.uri()).unwrap())
}

#[tokio::test]
async fn fetches_json_from_an_allowed_host() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"a":1}"#))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/ok", server.uri())).unwrap();
    let (v, _): (serde_json::Value, _) =
        f.get_json(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
    assert_eq!(v["a"], 1);
}

#[tokio::test]
async fn refuses_hosts_outside_the_policy_and_plain_http() {
    let f = Fetcher::new(FetchConfig::default()).unwrap();
    let url = Url::parse("https://evil.example/x").unwrap();
    let err = f
        .get(&url, &HostPolicy::new(&["good.example"]), &FetchOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err, FetchError::HostNotAllowed { host: "evil.example".into() });
    let url = Url::parse("http://good.example/x").unwrap();
    let err = f
        .get(&url, &HostPolicy::new(&["good.example"]), &FetchOptions::default())
        .await
        .unwrap_err();
    assert_eq!(err, FetchError::InsecureScheme { host: "good.example".into() });
}

#[tokio::test]
async fn refuses_redirects_to_other_hosts() {
    let server = MockServer::start().await;
    Mock::given(path("/r"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "https://elsewhere.example/steal"),
        )
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/r", server.uri())).unwrap();
    let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
    assert_eq!(err, FetchError::HostNotAllowed { host: "elsewhere.example".into() });
}

#[tokio::test]
async fn follows_same_host_redirects() {
    let server = MockServer::start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/old", server.uri())).unwrap();
    let r = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
    assert_eq!(&r.body[..], b"ok");
}

#[tokio::test]
async fn caps_body_size() {
    let server = MockServer::start().await;
    Mock::given(path("/big"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 4096]))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/big", server.uri())).unwrap();
    let opts = FetchOptions { max_body_bytes: Some(1024), ..FetchOptions::default() };
    let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
    assert!(matches!(err, FetchError::BodyTooLarge { limit: 1024, .. }), "{err:?}");
}

#[tokio::test]
async fn retries_429_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(path("/flaky"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/flaky"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/flaky", server.uri())).unwrap();
    let r = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap();
    assert_eq!(&r.body[..], b"ok");
}

#[tokio::test]
async fn does_not_retry_404_and_reports_host_only() {
    let server = MockServer::start().await;
    Mock::given(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/missing?key=SECRET", server.uri())).unwrap();
    let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
    assert!(matches!(err, FetchError::Status { status: 404, .. }));
    assert!(!err.to_string().contains("SECRET"), "{err}");
}

#[tokio::test]
async fn paces_requests_to_a_rate_limited_host() {
    let server = MockServer::start().await;
    Mock::given(path("/p"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Arc::new(Fetcher::new(config()).unwrap());
    let policy = policy(&server).with_min_interval(Duration::from_millis(150));
    let url = Url::parse(&format!("{}/p", server.uri())).unwrap();
    let started = Instant::now();
    let calls = (0..4).map(|_| {
        let (f, policy, url) = (f.clone(), policy.clone(), url.clone());
        tokio::spawn(async move { f.get(&url, &policy, &FetchOptions::default()).await.unwrap() })
    });
    for c in calls.collect::<Vec<_>>() {
        c.await.unwrap();
    }
    assert!(
        started.elapsed() >= Duration::from_millis(450),
        "4 requests at ≥150 ms spacing took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn cancelled_waiters_do_not_delay_later_requests() {
    // Review finding 5b: each cancelled waiter used to keep its pacing reservation, so
    // five cancellations pushed the next real request out by five intervals.
    let server = MockServer::start().await;
    Mock::given(path("/p"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Arc::new(Fetcher::new(config()).unwrap());
    let policy = policy(&server).with_min_interval(Duration::from_millis(300));
    let url = Url::parse(&format!("{}/p", server.uri())).unwrap();
    let started = Instant::now();
    f.get(&url, &policy, &FetchOptions::default()).await.unwrap();
    for _ in 0..5 {
        let cancel = CancellationToken::new();
        let opts = FetchOptions { cancel: Some(cancel.clone()), ..FetchOptions::default() };
        let (f, policy, url) = (f.clone(), policy.clone(), url.clone());
        let waiter = tokio::spawn(async move { f.get(&url, &policy, &opts).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
        assert_eq!(waiter.await.unwrap().unwrap_err(), FetchError::Cancelled);
    }
    f.get(&url, &policy, &FetchOptions::default()).await.unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(290), "still paced: {elapsed:?}");
    assert!(elapsed < Duration::from_millis(900), "cancelled waiters delayed it: {elapsed:?}");
}

#[tokio::test]
async fn per_request_body_caps_are_authoritative_within_the_ceiling() {
    let server = MockServer::start().await;
    Mock::given(path("/mid"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 4096]))
        .mount(&server)
        .await;
    let f = Fetcher::new(FetchConfig { max_body_bytes: 1024, ..config() }).unwrap();
    let url = Url::parse(&format!("{}/mid", server.uri())).unwrap();
    assert!(
        f.get(&url, &policy(&server), &FetchOptions::default()).await.is_err(),
        "the configured default applies without a provider cap"
    );
    let opts = FetchOptions { max_body_bytes: Some(8192), ..FetchOptions::default() };
    assert_eq!(f.get(&url, &policy(&server), &opts).await.unwrap().body.len(), 4096);
    let huge = FetchOptions { max_body_bytes: Some(usize::MAX), ..FetchOptions::default() };
    assert_eq!(f.body_limit(&huge), MAX_BODY_CEILING);
}

#[tokio::test]
async fn per_request_retry_count_overrides_the_default() {
    let server = MockServer::start().await;
    Mock::given(path("/down"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/down", server.uri())).unwrap();
    let opts = FetchOptions { retries: Some(0), ..FetchOptions::default() };
    let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
    assert!(matches!(err, FetchError::Status { status: 503, .. }), "{err:?}");
}

#[tokio::test]
async fn treats_420_as_a_retryable_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(path("/calm"))
        .respond_with(ResponseTemplate::new(420))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/calm"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/calm", server.uri())).unwrap();
    assert_eq!(
        &f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap().body[..],
        b"ok"
    );
}

#[tokio::test]
async fn times_out() {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let f = Fetcher::new(FetchConfig { retries: 0, request_timeout_s: 1, ..config() }).unwrap();
    let url = Url::parse(&format!("{}/slow", server.uri())).unwrap();
    let err = f.get(&url, &policy(&server), &FetchOptions::default()).await.unwrap_err();
    assert!(matches!(err, FetchError::Timeout { .. }), "{err:?}");
}

#[tokio::test]
async fn per_request_timeout_replaces_the_configured_one() {
    let server = MockServer::start().await;
    Mock::given(path("/large"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("ok")
                .set_delay(Duration::from_millis(1_500)),
        )
        .mount(&server)
        .await;
    let f = Fetcher::new(FetchConfig { retries: 0, request_timeout_s: 1, ..config() }).unwrap();
    let url = Url::parse(&format!("{}/large", server.uri())).unwrap();
    // A large download may take longer than ordinary requests are allowed ...
    let patient = FetchOptions { timeout: Some(Duration::from_secs(5)), ..FetchOptions::default() };
    assert_eq!(&f.get(&url, &policy(&server), &patient).await.unwrap().body[..], b"ok");
    // ... and a request can also be given less time.
    let hasty =
        FetchOptions { timeout: Some(Duration::from_millis(200)), ..FetchOptions::default() };
    let started = std::time::Instant::now();
    let err = f.get(&url, &policy(&server), &hasty).await.unwrap_err();
    assert!(matches!(err, FetchError::Timeout { .. }), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn cancellation_stops_a_request() {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/slow", server.uri())).unwrap();
    let cancel = CancellationToken::new();
    let opts = FetchOptions { cancel: Some(cancel.clone()), ..FetchOptions::default() };
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
    canceller.await.unwrap();
    assert_eq!(err, FetchError::Cancelled);
}

#[tokio::test]
async fn supports_byte_range_requests_with_206() {
    let server = MockServer::start().await;
    Mock::given(path("/range"))
        .and(wiremock::matchers::header("range", "bytes=10-19"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 10-19/100")
                .set_body_bytes(b"0123456789".to_vec()),
        )
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/range", server.uri())).unwrap();
    let opts = FetchOptions { range: Some((10, 19)), ..FetchOptions::default() };
    let resp = f.get(&url, &policy(&server), &opts).await.unwrap();
    assert_eq!(resp.status, 206);
    assert_eq!(&resp.body[..], b"0123456789");
}

#[tokio::test]
async fn supports_byte_range_requests_with_200_when_start_is_zero() {
    let server = MockServer::start().await;
    Mock::given(path("/range-zero"))
        .and(wiremock::matchers::header("range", "bytes=0-9"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"0123456789".to_vec()))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/range-zero", server.uri())).unwrap();
    let opts = FetchOptions { range: Some((0, 9)), ..FetchOptions::default() };
    let resp = f.get(&url, &policy(&server), &opts).await.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"0123456789");
}

#[tokio::test]
async fn refuses_200_for_non_zero_start_range_requests() {
    let server = MockServer::start().await;
    Mock::given(path("/range-nonzero"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"full content".to_vec()))
        .mount(&server)
        .await;
    let f = Fetcher::new(FetchConfig { retries: 0, ..config() }).unwrap();
    let url = Url::parse(&format!("{}/range-nonzero", server.uri())).unwrap();
    let opts = FetchOptions { range: Some((5, 10)), ..FetchOptions::default() };
    let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
    assert!(matches!(err, FetchError::Status { status: 200, .. }), "{err:?}");
}

#[tokio::test]
async fn a_zero_redirect_override_sends_exactly_one_request() {
    let server = MockServer::start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    let f = Fetcher::new(config()).unwrap();
    let url = Url::parse(&format!("{}/old", server.uri())).unwrap();
    let opts = FetchOptions { redirects: Some(0), ..FetchOptions::default() };
    let err = f.get(&url, &policy(&server), &opts).await.unwrap_err();
    assert!(matches!(err, FetchError::TooManyRedirects { .. }), "{err:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
