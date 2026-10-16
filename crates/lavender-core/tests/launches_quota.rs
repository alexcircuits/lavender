//! Launch Library 2 free-tier budget (ENG-957): every upstream HTTP attempt counts against
//! the hourly limit, including attempts after a restart, and a warm disk cache is reused.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use lavender_core::MemorySecretStore;
use lavender_core::config::CoreConfig;
use lavender_core::fetch::Fetcher;
use lavender_core::providers::launches::LaunchesProvider;
use lavender_core::providers::{FeedProvider, FetchContext, ProviderError, Region};
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURE_LL2: &[u8] = include_bytes!("fixtures/launches-ll2.json");
const LAUNCHES_PATH: &str = "/2.3.0/launches/";

fn config(server: &MockServer, max_requests_per_hour: u32) -> CoreConfig {
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    // The generic fetcher would retry 5xx and 429 twice; the provider must not let it.
    config.fetch.retries = 2;
    config.providers.launches.base_url = Url::parse(&format!("{}/2.3.0/", server.uri())).unwrap();
    config.providers.launches.cadence_s = 60;
    config.providers.launches.max_requests_per_hour = max_requests_per_hour;
    config
}

fn context<'a>(
    fetcher: &'a Fetcher,
    secrets: &'a MemorySecretStore,
    cache_dir: Option<&'a Path>,
) -> FetchContext<'a> {
    FetchContext { fetcher, secrets, cancel: CancellationToken::new(), cache_dir }
}

async fn upstream_requests(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// Makes the response cached on disk two hours old, as after a long shutdown, and returns
/// its new download time.
fn expire_disk_cache(dir: &Path) -> jiff::Timestamp {
    let path = dir.join("launches").join("launch-library-2.json");
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let old = jiff::Timestamp::now() - jiff::SignedDuration::from_hours(2);
    envelope["downloaded_at"] = serde_json::Value::String(old.to_string());
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    old
}

#[tokio::test]
async fn hourly_budget_counts_every_attempt_across_failures_and_restarts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FIXTURE_LL2))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let config = config(&server, 3);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&fetcher, &secrets, Some(dir.path()));
    let start = || LaunchesProvider::new(config.providers.launches.clone());

    let first = start().fetch(&ctx, Region::Global).await.unwrap();
    assert!(first.degraded.is_none());
    assert_eq!(upstream_requests(&server).await, 1);

    // A restart within the cadence reuses the warm disk cache without asking upstream.
    let warm = start().fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(warm.items, first.items);
    assert_eq!(warm.as_of, first.as_of);
    assert!(warm.degraded.is_none());
    assert_eq!(upstream_requests(&server).await, 1);

    // Once the cache is old, each restarted process may ask again, but every attempt is
    // one HTTP request (no fetcher retries) and counts against the same hourly budget.
    let expired_at = expire_disk_cache(dir.path());
    for expected in [2, 3] {
        let failed = start().fetch(&ctx, Region::Global).await.unwrap();
        assert!(failed.degraded.unwrap().contains("503"));
        assert_eq!(failed.as_of, Some(expired_at), "the stale copy keeps its real age");
        assert_eq!(upstream_requests(&server).await, expected);
    }

    let refused = start();
    for _ in 0..3 {
        let answer = refused.fetch(&ctx, Region::Global).await.unwrap();
        assert!(answer.degraded.unwrap().contains("limit 3 per hour"));
        assert_eq!(answer.items, first.items);
    }
    assert_eq!(upstream_requests(&server).await, 3, "never more than the configured budget");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_attempts_are_all_on_disk_before_a_restart() {
    // Overlapping fetches each record an attempt; whichever stores the budget last must
    // store all of them, or a restart could spend the hour's budget a second time.
    const CONCURRENT: u32 = 8;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let config = config(&server, CONCURRENT);
    let fetcher = Arc::new(Fetcher::new(config.fetch.clone()).unwrap());
    let secrets = Arc::new(MemorySecretStore::default());
    for round in 1..=10 {
        let cache = tempfile::tempdir().unwrap();
        let provider = Arc::new(LaunchesProvider::new(config.providers.launches.clone()));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..CONCURRENT {
            let (provider, fetcher, secrets) = (provider.clone(), fetcher.clone(), secrets.clone());
            let cache_dir = cache.path().to_owned();
            tasks.spawn(async move {
                let ctx = context(&fetcher, &secrets, Some(&cache_dir));
                provider.fetch(&ctx, Region::Global).await.is_err()
            });
        }
        while let Some(failed) = tasks.join_next().await {
            assert!(failed.unwrap(), "503 with nothing cached is an error");
        }
        drop(provider);
        let sent = upstream_requests(&server).await;
        assert_eq!(sent, usize::try_from(CONCURRENT * round).unwrap());

        let restarted = LaunchesProvider::new(config.providers.launches.clone());
        let refused = restarted
            .fetch(&context(&fetcher, &secrets, Some(cache.path())), Region::Global)
            .await
            .unwrap_err();
        assert!(
            matches!(&refused, ProviderError::Unavailable(m) if m.contains("hourly budget")),
            "round {round}: {refused}"
        );
        assert_eq!(upstream_requests(&server).await, sent, "round {round}");
    }
}

#[tokio::test]
async fn a_restart_without_a_warm_cache_cannot_reset_the_hourly_budget() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let config = config(&server, 1);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&fetcher, &secrets, Some(dir.path()));

    let first = LaunchesProvider::new(config.providers.launches.clone());
    assert!(matches!(first.fetch(&ctx, Region::Global).await, Err(ProviderError::Fetch(_))));
    drop(first);
    let restarted = LaunchesProvider::new(config.providers.launches.clone());
    let refused = restarted.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(
        matches!(&refused, ProviderError::Unavailable(m) if m.contains("limit 1 per hour")),
        "{refused}"
    );
    assert_eq!(upstream_requests(&server).await, 1);
}

#[tokio::test]
async fn a_429_cooldown_survives_a_restart() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let config = config(&server, 15);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&fetcher, &secrets, Some(dir.path()));

    let first = LaunchesProvider::new(config.providers.launches.clone());
    let throttled = first.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(
        matches!(&throttled, ProviderError::Unavailable(m) if m.contains("429")),
        "{throttled}"
    );
    drop(first);
    let restarted = LaunchesProvider::new(config.providers.launches.clone());
    let cooling = restarted.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(
        matches!(&cooling, ProviderError::Unavailable(m) if m.contains("cooldown")),
        "{cooling}"
    );
    assert_eq!(upstream_requests(&server).await, 1);
}

#[tokio::test]
async fn a_redirect_cannot_send_a_second_uncounted_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/2.3.0/moved/"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3.0/moved/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FIXTURE_LL2))
        .mount(&server)
        .await;
    let config = config(&server, 1);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = context(&fetcher, &secrets, None);
    let provider = LaunchesProvider::new(config.providers.launches);

    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    assert_eq!(upstream_requests(&server).await, 1, "one reserved slot is one HTTP request");
}

#[tokio::test]
async fn a_cancelled_request_still_counts_against_the_budget() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(FIXTURE_LL2)
                .set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let config = config(&server, 1);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = context(&fetcher, &secrets, None);
    let provider = LaunchesProvider::new(config.providers.launches);

    let cancel = ctx.cancel.clone();
    let (cancelled, ()) = tokio::join!(provider.fetch(&ctx, Region::Global), async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        cancel.cancel();
    });
    assert!(cancelled.is_err());
    // The request may already have reached Launch Library 2, so its slot stays used.
    let fresh_ctx = context(&fetcher, &secrets, None);
    let refused = provider.fetch(&fresh_ctx, Region::Global).await.unwrap_err();
    assert!(
        matches!(&refused, ProviderError::Unavailable(m) if m.contains("limit 1 per hour")),
        "{refused}"
    );
    assert_eq!(upstream_requests(&server).await, 1);
}

#[tokio::test]
async fn restart_reuses_the_recent_disk_cache_without_another_upstream_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FIXTURE_LL2))
        .expect(1)
        .mount(&server)
        .await;
    let config = config(&server, 15);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let dir = tempfile::tempdir().unwrap();
    let ctx = context(&fetcher, &secrets, Some(dir.path()));
    let first = LaunchesProvider::new(config.providers.launches.clone());
    let result = first.fetch(&ctx, Region::Global).await.unwrap();
    drop(first);
    let restarted = LaunchesProvider::new(config.providers.launches);
    let restored = restarted.fetch(&ctx, Region::Global).await.unwrap();
    assert_eq!(restored.items, result.items);
    assert_eq!(restored.as_of, result.as_of);
    server.verify().await;
}

#[tokio::test]
async fn transient_http_errors_cannot_retry_outside_the_provider_quota() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let config = config(&server, 1);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = context(&fetcher, &secrets, None);
    let provider = LaunchesProvider::new(config.providers.launches);
    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    assert!(provider.fetch(&ctx, Region::Global).await.is_err());
    server.verify().await;
}

#[tokio::test]
async fn a_fresh_memory_cache_answers_without_spending_the_budget() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FIXTURE_LL2))
        .expect(1)
        .mount(&server)
        .await;
    let config = config(&server, 1);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = context(&fetcher, &secrets, None);
    let provider = LaunchesProvider::new(config.providers.launches);

    for _ in 0..3 {
        let answer = provider.fetch(&ctx, Region::Global).await.unwrap();
        assert_eq!(answer.items.len(), 3);
        assert!(answer.degraded.is_none());
    }
    server.verify().await;
}

#[tokio::test]
async fn http_429_starts_a_cooldown_without_further_requests() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LAUNCHES_PATH))
        .respond_with(ResponseTemplate::new(429).set_body_string("Rate limit exceeded"))
        .expect(1)
        .mount(&server)
        .await;
    let config = config(&server, 15);
    let fetcher = Fetcher::new(config.fetch.clone()).unwrap();
    let secrets = MemorySecretStore::default();
    let ctx = context(&fetcher, &secrets, None);
    let provider = LaunchesProvider::new(config.providers.launches);

    let first = provider.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(matches!(&first, ProviderError::Unavailable(m) if m.contains("429")), "{first}");
    let second = provider.fetch(&ctx, Region::Global).await.unwrap_err();
    assert!(matches!(&second, ProviderError::Unavailable(m) if m.contains("cooldown")), "{second}");
    server.verify().await;
}
