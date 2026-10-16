//! Streaming connections (the `AISStream` WebSocket) leave through the same egress policy as
//! HTTP fetches: allowed hosts only, secure scheme, public addresses only.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use lavender_core::config::FetchConfig;
use lavender_core::fetch::{FetchError, Fetcher, HostPolicy};
use tokio::net::TcpListener;
use url::Url;

async fn listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

async fn nothing_accepted(listener: &TcpListener) -> bool {
    tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_err()
}

#[tokio::test]
async fn private_targets_are_refused_before_any_connection() {
    let (listener, port) = listener().await;
    let fetcher = Fetcher::new(FetchConfig::default()).unwrap();
    for host in ["127.0.0.1", "localhost", "[::ffff:127.0.0.1]"] {
        let url = Url::parse(&format!("wss://{host}:{port}/v0/stream")).unwrap();
        let error = fetcher.connect_stream(&url, &HostPolicy::for_url(&url)).await.unwrap_err();
        assert!(matches!(error, FetchError::NonPublicAddress { .. }), "{host}: {error:?}");
    }
    assert!(nothing_accepted(&listener).await);
}

#[tokio::test]
async fn plain_websockets_hosts_outside_the_policy_and_other_schemes_are_refused() {
    let fetcher = Fetcher::new(FetchConfig::default()).unwrap();
    let plain = Url::parse("ws://stream.example/v0/stream").unwrap();
    let error = fetcher.connect_stream(&plain, &HostPolicy::for_url(&plain)).await.unwrap_err();
    assert!(matches!(error, FetchError::InsecureStream { .. }), "{error:?}");

    let https = Url::parse("https://stream.example/v0/stream").unwrap();
    let error = fetcher.connect_stream(&https, &HostPolicy::for_url(&https)).await.unwrap_err();
    assert!(matches!(error, FetchError::InsecureStream { .. }), "{error:?}");

    let other = Url::parse("wss://evil.example/v0/stream").unwrap();
    let error =
        fetcher.connect_stream(&other, &HostPolicy::new(&["stream.example"])).await.unwrap_err();
    assert!(matches!(error, FetchError::HostNotAllowed { .. }), "{error:?}");
}

#[tokio::test]
async fn listed_development_hosts_may_use_plain_websockets() {
    let (listener, port) = listener().await;
    let config =
        FetchConfig { allow_http_hosts: vec!["127.0.0.1".into()], ..FetchConfig::default() };
    let fetcher = Fetcher::new(config).unwrap();
    let url = Url::parse(&format!("ws://127.0.0.1:{port}/v0/stream")).unwrap();
    let policy = HostPolicy::for_url(&url);
    let (connected, accepted) =
        tokio::join!(fetcher.connect_stream(&url, &policy), listener.accept());
    let stream = connected.unwrap();
    assert_eq!(stream.peer_addr().unwrap(), accepted.unwrap().0.local_addr().unwrap());
}

#[tokio::test]
async fn an_unreachable_host_is_a_network_error_naming_only_the_host() {
    let (listener, port) = listener().await;
    drop(listener);
    let config =
        FetchConfig { allow_http_hosts: vec!["127.0.0.1".into()], ..FetchConfig::default() };
    let fetcher = Fetcher::new(config).unwrap();
    let url = Url::parse(&format!("ws://127.0.0.1:{port}/v0/stream?token=STREAM_CANARY")).unwrap();
    let error = fetcher.connect_stream(&url, &HostPolicy::for_url(&url)).await.unwrap_err();
    assert!(matches!(error, FetchError::Network { .. } | FetchError::Timeout { .. }), "{error:?}");
    assert!(!error.to_string().contains("STREAM_CANARY"), "{error}");
}
