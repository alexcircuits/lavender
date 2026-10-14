//! The actual Fetcher client rejects private literal and DNS targets before connecting.
#![allow(clippy::unwrap_used)]

use lavender_core::config::FetchConfig;
use lavender_core::fetch::{FetchError, FetchOptions, Fetcher, HostPolicy};
use std::time::Duration;
use tokio::net::TcpListener;
use url::Url;

#[tokio::test]
async fn private_literals_and_localhost_dns_cannot_reach_the_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fetcher = Fetcher::new(FetchConfig { retries: 0, ..FetchConfig::default() }).unwrap();
    for host in ["127.0.0.1", "localhost", "[::ffff:127.0.0.1]", "169.254.169.254"] {
        let url = Url::parse(&format!(
            "https://{host}:{}/private?token=DNS_SECRET_CANARY",
            listener.local_addr().unwrap().port()
        ))
        .unwrap();
        let result = fetcher.get(&url, &HostPolicy::for_url(&url), &FetchOptions::default()).await;
        let error = result.unwrap_err();
        assert!(matches!(error, FetchError::NonPublicAddress { .. }), "{error:?}");
        assert!(!error.to_string().contains("DNS_SECRET_CANARY"));
    }
    assert!(tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_err());
}
