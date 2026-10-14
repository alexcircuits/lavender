//! Conditional source validation, restart persistence and malformed-cache regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jiff::{SignedDuration, Timestamp};
use lavender_core::MemorySecretStore;
use lavender_core::config::{CelestrakConfig, FetchConfig};
use lavender_core::fetch::{CacheValidators, FetchError, FetchOptions, Fetcher, HostPolicy};
use lavender_core::providers::celestrak::CelestrakProvider;
use lavender_core::providers::{FeedProvider, FetchContext, Region};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OMM: &str = r#"[{"OBJECT_NAME":"TEST STATION","OBJECT_ID":"1998-067A","EPOCH":"2026-10-10T12:00:00.000000","MEAN_MOTION":15.5,"ECCENTRICITY":0.0007,"INCLINATION":51.64,"RA_OF_ASC_NODE":247.46,"ARG_OF_PERICENTER":130.53,"MEAN_ANOMALY":325.03,"NORAD_CAT_ID":25544,"BSTAR":0.0003,"MEAN_MOTION_DOT":0.00016,"MEAN_MOTION_DDOT":0,"EXTRA_ATTRIBUTE":"retained-raw-schema"}]"#;
const MODIFIED: &str = "Sat, 10 Oct 2026 12:00:00 GMT";

fn fetcher() -> Fetcher {
    Fetcher::new(FetchConfig {
        allow_http_hosts: vec!["127.0.0.1".into()],
        retries: 2,
        ..FetchConfig::default()
    })
    .unwrap()
}

fn provider(server: &MockServer) -> CelestrakProvider {
    CelestrakProvider::new(CelestrakConfig {
        base_url: Url::parse(&format!("{}/celestrak/", server.uri())).unwrap(),
        groups: vec!["stations".into()],
        ..CelestrakConfig::default()
    })
}

fn ctx<'a>(
    fetcher: &'a Fetcher,
    secrets: &'a MemorySecretStore,
    dir: Option<&'a Path>,
) -> FetchContext<'a> {
    FetchContext { fetcher, secrets, cancel: CancellationToken::new(), cache_dir: dir }
}

fn cache_file(dir: &Path) -> PathBuf {
    dir.join("celestrak/stations.json")
}

fn cache(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(cache_file(dir)).unwrap()).unwrap()
}

fn write_cache(dir: &Path, value: &Value) {
    std::fs::create_dir_all(dir.join("celestrak")).unwrap();
    std::fs::write(cache_file(dir), serde_json::to_vec(value).unwrap()).unwrap();
}

fn expire_disk(dir: &Path) -> Timestamp {
    let mut value = cache(dir);
    let downloaded = Timestamp::now().checked_sub(SignedDuration::from_hours(6)).unwrap();
    let validated = Timestamp::now().checked_sub(SignedDuration::from_hours(3)).unwrap();
    value["downloaded_at"] = Value::String(downloaded.to_string());
    value["validated_at"] = Value::String(validated.to_string());
    write_cache(dir, &value);
    downloaded
}

#[tokio::test]
async fn restart_revalidates_unchanged_bytes_and_preserves_original_download_time() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(OMM)
                .insert_header("etag", "\"v1\"")
                .insert_header("last-modified", MODIFIED),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/celestrak/gp.php"))
        .and(header("if-none-match", "\"v1\""))
        .and(|request: &wiremock::Request| {
            request
                .headers
                .get("if-modified-since")
                .is_some_and(|value| value.as_bytes() == MODIFIED.as_bytes())
        })
        .respond_with(ResponseTemplate::new(304).insert_header("etag", "W/\"v1\""))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let context = ctx(&fetcher, &secrets, Some(dir.path()));
    let original = provider(&server).fetch(&context, Region::Global).await.unwrap();
    let downloaded = expire_disk(dir.path());
    let before = Timestamp::now();
    let restarted = provider(&server);
    let revalidated = restarted.fetch(&context, Region::Global).await.unwrap();
    assert_eq!(revalidated.items, original.items);
    assert!(revalidated.as_of.unwrap() >= before);
    assert_eq!(revalidated.degraded, None);
    let saved = cache(dir.path());
    assert_eq!(saved["downloaded_at"], downloaded.to_string());
    assert_eq!(saved["body"], OMM, "the complete raw schema remains unchanged");
    assert_eq!(saved["etag"], "W/\"v1\"");
    assert_eq!(saved["last_modified"], MODIFIED, "omitted 304 metadata retains prior value");
    let cached_again = provider(&server).fetch(&context, Region::Global).await.unwrap();
    assert_eq!(
        cached_again.as_of, revalidated.as_of,
        "another restart does not renew age or refetch"
    );
    assert_eq!(restarted.fetch(&context, Region::Global).await.unwrap().as_of, revalidated.as_of);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn last_modified_only_is_a_valid_conditional_request() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(OMM)
                .insert_header("last-modified", MODIFIED),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/celestrak/gp.php"))
        .and(|request: &wiremock::Request| {
            request
                .headers
                .get("if-modified-since")
                .is_some_and(|value| value.as_bytes() == MODIFIED.as_bytes())
        })
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let context = ctx(&fetcher, &secrets, Some(dir.path()));
    let first = provider(&server).fetch(&context, Region::Global).await.unwrap();
    expire_disk(dir.path());
    let second = provider(&server).fetch(&context, Region::Global).await.unwrap();
    assert_eq!(first.items, second.items);
    let requests = server.received_requests().await.unwrap();
    assert!(requests[1].headers.get("if-none-match").is_none());
}

#[tokio::test]
async fn failed_revalidation_keeps_age_and_obeys_failure_floor_without_fetcher_retries() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(OMM).insert_header("etag", "\"v1\""),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/celestrak/gp.php"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let context = ctx(&fetcher, &secrets, Some(dir.path()));
    provider(&server).fetch(&context, Region::Global).await.unwrap();
    expire_disk(dir.path());
    let saved = cache(dir.path());
    let original_validation: Timestamp = saved["validated_at"].as_str().unwrap().parse().unwrap();
    let restarted = provider(&server);
    let failed = restarted.fetch(&context, Region::Global).await.unwrap();
    assert_eq!(failed.as_of, Some(original_validation));
    assert!(failed.degraded.unwrap().contains("429"));
    let throttled = restarted.fetch(&context, Region::Global).await.unwrap();
    assert!(throttled.degraded.unwrap().contains("next request allowed"));
    assert_eq!(throttled.as_of, Some(original_validation));
    assert_eq!(cache(dir.path()), saved, "failure does not rewrite timestamps");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn cancelled_conditional_request_does_not_reserve_the_failure_floor() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(OMM).insert_header("etag", "\"v1\""),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    let slow = Mock::given(path("/celestrak/gp.php"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304).set_delay(Duration::from_secs(5)))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let mut context = ctx(&fetcher, &secrets, Some(dir.path()));
    provider(&server).fetch(&context, Region::Global).await.unwrap();
    expire_disk(dir.path());
    let restarted = provider(&server);
    let cancel = context.cancel.clone();
    let operation = restarted.fetch(&context, Region::Global);
    let (result, ()) = tokio::join!(operation, async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.received_requests().await.unwrap().len() < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
    });
    assert!(result.unwrap().degraded.unwrap().contains("cancelled"));
    drop(slow);
    Mock::given(path("/celestrak/gp.php"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    context.cancel = CancellationToken::new();
    assert_eq!(restarted.fetch(&context, Region::Global).await.unwrap().degraded, None);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn unsolicited_304_never_becomes_empty_success() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(304))
        .expect(2)
        .mount(&server)
        .await;
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let error =
        provider(&server).fetch(&ctx(&fetcher, &secrets, None), Region::Global).await.unwrap_err();
    assert!(error.to_string().contains("304"));
    let url = Url::parse(&format!("{}/celestrak/gp.php", server.uri())).unwrap();
    assert!(matches!(
        fetcher.get(&url, &HostPolicy::for_url(&url), &FetchOptions::default()).await,
        Err(FetchError::Status { status: 304, .. })
    ));
}

#[test]
fn malformed_validator_values_are_refused_without_echoing_them() {
    for etag in
        ["plain", "*", "\"one\", \"two\"", "\"a\r\nAuthorization: secret\"", "W/\"bad\"quote\""]
    {
        let error = CacheValidators::new(Some(etag), None).unwrap_err();
        assert_eq!(error.to_string(), "conditional request validators are invalid");
    }
    assert!(CacheValidators::new(Some(&format!("\"{}\"", "a".repeat(513))), None).is_err());
    for modified in [
        "invalid",
        "Sat, 40 Oct 2026 12:00:00 GMT",
        "Sat, 10 Oct 2026 12:00:00 GMT\r\nCookie: secret",
    ] {
        assert!(CacheValidators::new(None, Some(modified)).is_err());
    }
    assert!(CacheValidators::new(Some("W/\"good\""), Some(MODIFIED)).is_ok());
    assert!(CacheValidators::new(None, None).is_err());
}

#[tokio::test]
async fn conditional_headers_are_not_replayed_after_a_different_url_redirect() {
    let server = MockServer::start().await;
    Mock::given(path("/original"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/replacement"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/replacement"))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let url = Url::parse(&format!("{}/original", server.uri())).unwrap();
    let options = FetchOptions {
        conditional: Some(CacheValidators::new(Some("\"v1\""), Some(MODIFIED)).unwrap()),
        ..FetchOptions::default()
    };
    assert!(matches!(
        fetcher().get(&url, &HostPolicy::for_url(&url), &options).await,
        Err(FetchError::Status { status: 304, .. })
    ));
    let requests = server.received_requests().await.unwrap();
    assert!(requests[1].headers.get("if-none-match").is_none());
    assert!(requests[1].headers.get("if-modified-since").is_none());
}

#[tokio::test]
async fn redirected_response_validators_are_not_persisted_for_the_original_resource() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/replacement"))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(path("/replacement"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(OMM)
                .insert_header("etag", "\"target-only\"")
                .insert_header("last-modified", MODIFIED),
        )
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let context = ctx(&fetcher, &secrets, Some(dir.path()));
    let original = provider(&server).fetch(&context, Region::Global).await.unwrap();
    expire_disk(dir.path());
    let restarted = provider(&server).fetch(&context, Region::Global).await.unwrap();
    assert_eq!(restarted.items, original.items);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    for request in requests {
        assert!(request.headers.get("if-none-match").is_none());
        assert!(request.headers.get("if-modified-since").is_none());
    }
    let saved = cache(dir.path());
    assert!(saved["etag"].is_null());
    assert!(saved["last_modified"].is_null());
    assert_eq!(saved["body"], OMM);
}

#[tokio::test]
async fn malformed_envelopes_are_never_used_or_revalidated() {
    let server = MockServer::start().await;
    Mock::given(path("/celestrak/gp.php"))
        .respond_with(ResponseTemplate::new(200).set_body_string(OMM))
        .expect(8)
        .mount(&server)
        .await;
    let now = Timestamp::now();
    let base = serde_json::json!({
        "schema": 1, "resource": format!("{}/celestrak/gp.php?GROUP=stations&FORMAT=json",server.uri()),
        "body": OMM, "downloaded_at": now.to_string(), "validated_at": now.to_string(),
        "etag": "\"v1\"", "last_modified": MODIFIED
    });
    let mut malformed = Vec::new();
    for (key, replacement) in [
        ("schema", Value::from(2)),
        ("etag", Value::from("\"bad\r\nAuthorization: secret\"")),
        ("last_modified", Value::from("not a date")),
        ("resource", Value::from("https://user:secret@other.example/gp.php")),
        ("body", Value::from("not OMM")),
        (
            "validated_at",
            Value::from(now.checked_add(SignedDuration::from_hours(1)).unwrap().to_string()),
        ),
        ("authorization", Value::from("secret")),
    ] {
        let mut value = base.clone();
        value[key] = replacement;
        malformed.push(value);
    }
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    for malformed in malformed {
        let dir = tempfile::tempdir().unwrap();
        write_cache(dir.path(), &malformed);
        assert_eq!(
            provider(&server)
                .fetch(&ctx(&fetcher, &secrets, Some(dir.path())), Region::Global)
                .await
                .unwrap()
                .items
                .len(),
            1
        );
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("celestrak")).unwrap();
    std::fs::File::create(cache_file(dir.path())).unwrap().set_len(42 * 1024 * 1024).unwrap();
    assert_eq!(
        provider(&server)
            .fetch(&ctx(&fetcher, &secrets, Some(dir.path())), Region::Global)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    for request in server.received_requests().await.unwrap() {
        assert!(request.headers.get("if-none-match").is_none());
        assert!(request.headers.get("if-modified-since").is_none());
    }
}

#[tokio::test]
async fn credential_urls_are_refused_before_cache_access_or_networking() {
    let server = MockServer::start().await;
    let mut base = Url::parse(&format!("{}/celestrak/", server.uri())).unwrap();
    base.set_username("private-user").unwrap();
    base.set_password(Some("private-secret")).unwrap();
    let provider = CelestrakProvider::new(CelestrakConfig {
        base_url: base,
        groups: vec!["stations".into()],
        ..CelestrakConfig::default()
    });
    let fetcher = fetcher();
    let secrets = MemorySecretStore::default();
    let error = provider.fetch(&ctx(&fetcher, &secrets, None), Region::Global).await.unwrap_err();
    assert!(error.to_string().contains("must not contain credentials"));
    assert!(!error.to_string().contains("private-"));
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test]
async fn a_conditional_304_has_no_body_even_with_representation_content_length() {
    let server = MockServer::start().await;
    Mock::given(path("/unchanged"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("content-length", "67108865"))
        .expect(1)
        .mount(&server)
        .await;
    let url = Url::parse(&format!("{}/unchanged", server.uri())).unwrap();
    let options = FetchOptions {
        max_body_bytes: Some(1),
        conditional: Some(CacheValidators::new(Some("\"v1\""), None).unwrap()),
        ..FetchOptions::default()
    };
    let response = fetcher().get(&url, &HostPolicy::for_url(&url), &options).await.unwrap();
    assert_eq!(response.status, 304);
    assert_eq!(response.body.len(), 0);
}
