//! The AISStream connection against local servers: subscribe, reconnect after the server
//! closes, rejected keys and rate limits on the upgrade and in error frames, silent, dead
//! and oversized connections (quiet narrow subscriptions stay up while they answer
//! pings), unreadable streams, idle disconnect, tracks ageing out after it, and no
//! connection at all without a key. Everything runs on 127.0.0.1 with synthetic
//! messages; no AISStream key was available. The `wss://` path is covered by the link's
//! unit tests (`src/providers/aisstream/link/tests.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use lavender_core::config::{AisStreamConfig, CoreConfig, FetchConfig};
use lavender_core::fetch::Fetcher;
use lavender_core::providers::aisstream::{AisStreamProvider, Failure, LinkState, LinkStatus};
use lavender_core::providers::{FeedProvider, FetchContext, ProviderError, Region};
use lavender_core::{LavenderCore, MemorySecretStore, SecretKey, SecretStore, SubscriptionEvent};
use lavender_domain::{BBox, FeedId, FeedItems, FeedQuery, Freshness, Timestamp};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use url::Url;

const KEY: &str = "test-key-not-real";

fn position(mmsi: u32, lat: f64, lon: f64) -> String {
    let now = Timestamp::now().strftime("%Y-%m-%d %H:%M:%S%.f +0000 UTC").to_string();
    serde_json::json!({
        "MessageType": "PositionReport",
        "MetaData": {"MMSI": mmsi, "ShipName": format!("TEST {mmsi}"), "time_utc": now},
        "Message": {"PositionReport": {
            "UserID": mmsi, "Valid": true, "Latitude": lat, "Longitude": lon,
            "Sog": 10.0, "Cog": 90.0, "TrueHeading": 90
        }}
    })
    .to_string()
}

const CONFIRMATION: &str =
    r#"{"MessageType":"SubscriptionConfirmation","Message":{"CompressionEnabled":false}}"#;

struct Server {
    port: u16,
    connections: Arc<AtomicUsize>,
    subscriptions: mpsc::UnboundedReceiver<serde_json::Value>,
}

/// A WebSocket server that runs `script` for each connection (numbered from 1) after
/// reading the client's subscription. AISStream sends JSON in binary frames.
async fn ws_server<F, Fut>(script: F) -> Server
where
    F: Fn(usize, tokio_tungstenite::WebSocketStream<TcpStream>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let (tx, subscriptions) = mpsc::unbounded_channel();
    let count = connections.clone();
    let script = Arc::new(script);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let n = count.fetch_add(1, Ordering::SeqCst) + 1;
            let (tx, script) = (tx.clone(), script.clone());
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(tcp).await else { return };
                if let Some(Ok(Message::Text(text))) = ws.next().await {
                    let _ = tx.send(serde_json::from_str(text.as_str()).unwrap_or_default());
                }
                script(n, ws).await;
            });
        }
    });
    Server { port, connections, subscriptions }
}

/// A plain TCP server that answers every WebSocket upgrade with `response`.
async fn http_server(response: &'static str) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            let mut request = Vec::new();
            let mut buf = [0_u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match tcp.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let _ = tcp.write_all(response.as_bytes()).await;
        }
    });
    (port, connections)
}

fn config(port: u16) -> AisStreamConfig {
    AisStreamConfig {
        url: Url::parse(&format!("ws://127.0.0.1:{port}/v0/stream")).unwrap(),
        update_interval_s: 1,
        read_timeout_s: 30,
        reconnect_min_ms: 50,
        reconnect_max_s: 1,
        idle_disconnect_s: 0,
        ..AisStreamConfig::default()
    }
}

fn fetcher() -> Arc<Fetcher> {
    let config =
        FetchConfig { allow_http_hosts: vec!["127.0.0.1".into()], ..FetchConfig::default() };
    Arc::new(Fetcher::new(config).unwrap())
}

fn with_key() -> MemorySecretStore {
    MemorySecretStore::with([(SecretKey::AisStreamApiKey, KEY)])
}

/// One provider fetch, as the hub would make it.
async fn fetch(
    provider: &AisStreamProvider,
    secrets: &dyn SecretStore,
) -> Result<lavender_core::providers::Fetched, ProviderError> {
    let fetcher = fetcher();
    let ctx = FetchContext {
        fetcher: &fetcher,
        secrets,
        cancel: CancellationToken::new(),
        cache_dir: None,
    };
    provider.fetch(&ctx, Region::Global).await
}

async fn wait_for(
    provider: &AisStreamProvider,
    what: &str,
    done: impl Fn(&LinkStatus) -> bool,
) -> LinkStatus {
    for _ in 0..500 {
        let status = provider.link_status();
        if done(&status) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}: {:?}", provider.link_status());
}

fn waiting(status: &LinkStatus) -> Option<(&Failure, Timestamp)> {
    match &status.state {
        LinkState::Waiting { failure, retry_at } => Some((failure, *retry_at)),
        _ => None,
    }
}

#[tokio::test]
async fn reconnects_after_the_server_closes_and_feeds_the_hub() {
    let mut server = ws_server(|n, mut ws| async move {
        ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await.unwrap();
        if n == 1 {
            ws.send(Message::binary(position(230_990_001, 60.1, 24.9).into_bytes())).await.unwrap();
            let _ = ws.close(None).await;
        } else {
            ws.send(Message::binary(position(230_990_002, 60.2, 24.8).into_bytes())).await.unwrap();
            // Stay connected until the client goes away.
            while ws.next().await.is_some() {}
        }
    })
    .await;
    let provider = Arc::new(AisStreamProvider::new(config(server.port), fetcher()));
    let core = LavenderCore::builder(CoreConfig::default())
        .providers(vec![provider.clone() as Arc<dyn FeedProvider>])
        .secrets(Arc::new(with_key()))
        .build()
        .unwrap();
    let query =
        FeedQuery::new(FeedId::Vessels).with_bbox(BBox::new(24.0, 59.5, 25.5, 60.5).unwrap());
    let mut sub = core.subscribe(query).unwrap();
    let snapshot = loop {
        match tokio::time::timeout(Duration::from_secs(10), sub.next()).await.unwrap().unwrap() {
            SubscriptionEvent::Snapshot(s)
                if s.items.len() == 2 && s.freshness == Freshness::Live =>
            {
                break s;
            }
            SubscriptionEvent::Snapshot(_) => {}
            SubscriptionEvent::Error(e) => panic!("{e:?}"),
        }
    };
    let FeedItems::Vessels(vessels) = &snapshot.items else { panic!("vessels") };
    assert_eq!(vessels[0].mmsi, "230990001");
    assert_eq!(vessels[1].name.as_deref(), Some("TEST 230990002"));
    let status = provider.link_status();
    assert_eq!((status.attempts, status.sessions), (2, 2), "{status:?}");
    assert!(matches!(status.state, LinkState::Streaming { .. }));
    assert!(status.last_failure.is_none(), "a working session clears the failure");
    assert_eq!(server.connections.load(Ordering::SeqCst), 2);
    for _ in 0..2 {
        let subscription = server.subscriptions.recv().await.unwrap();
        assert_eq!(subscription["APIKey"], KEY);
        assert_eq!(
            subscription["BoundingBoxes"],
            serde_json::json!([[[-90.0, -180.0], [90.0, 180.0]]])
        );
        assert_eq!(subscription["FilterMessageTypes"][0], "PositionReport");
    }
    let track = core.vessel_track("230990001").unwrap();
    assert_eq!(track.samples.len(), 1);
    core.shutdown();
    wait_for(&provider, "shutdown", |s| s.state == LinkState::Stopped).await;
}

#[tokio::test]
async fn rejected_keys_on_the_upgrade_wait_an_hour_and_report_why() {
    for (code, response) in [
        (401, "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        (403, "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    ] {
        let (port, connections) = http_server(response).await;
        let provider = AisStreamProvider::new(config(port), fetcher());
        let secrets = with_key();
        let first = fetch(&provider, &secrets).await.unwrap();
        assert_eq!(
            first.degraded.as_deref(),
            Some("Connecting to AISStream; vessels appear as their reports arrive.")
        );
        let status = wait_for(&provider, "rejection", |s| waiting(s).is_some()).await;
        let (failure, retry_at) = waiting(&status).unwrap();
        assert_eq!(*failure, Failure::Rejected { status: Some(code) });
        assert!(retry_at.duration_since(Timestamp::now()).as_secs() >= 59 * 60, "{retry_at}");
        let error = fetch(&provider, &secrets).await.unwrap_err();
        let message = error.public_message();
        assert!(message.contains(&format!("rejected the API key (HTTP {code})")), "{message}");
        let minutes: u64 = message
            .split("next attempt in ")
            .nth(1)
            .and_then(|rest| rest.strip_suffix(" min."))
            .and_then(|m| m.parse().ok())
            .unwrap_or_else(|| panic!("{message}"));
        assert!((59..=67).contains(&minutes), "{message}");
        assert!(!message.contains(KEY));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(connections.load(Ordering::SeqCst), 1, "no retry within the long wait");
    }
}

#[tokio::test]
async fn rate_limits_on_the_upgrade_honour_retry_after() {
    let (port, connections) = http_server(
        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
    .await;
    let provider = AisStreamProvider::new(config(port), fetcher());
    let secrets = with_key();
    fetch(&provider, &secrets).await.unwrap();
    let status = wait_for(&provider, "rate limit", |s| waiting(s).is_some()).await;
    let (failure, retry_at) = waiting(&status).unwrap();
    assert_eq!(*failure, Failure::RateLimited { retry_after: Some(Duration::from_secs(120)) });
    assert!(retry_at.duration_since(Timestamp::now()).as_secs() >= 115, "{retry_at}");
    let message = fetch(&provider, &secrets).await.unwrap_err().public_message();
    assert!(message.contains("limiting connections"), "{message}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn error_frames_are_classified_and_never_echoed() {
    for (text, rejected) in [
        (format!("Api Key Is Not Valid: {KEY}"), true),
        ("Too many concurrent connections for this API key".to_owned(), false),
    ] {
        let server = ws_server(move |_, mut ws| {
            let frame = serde_json::json!({ "error": text.clone() }).to_string();
            async move {
                let _ = ws.send(Message::binary(frame.into_bytes())).await;
                while ws.next().await.is_some() {}
            }
        })
        .await;
        let provider = AisStreamProvider::new(config(server.port), fetcher());
        let secrets = with_key();
        fetch(&provider, &secrets).await.unwrap();
        let status = wait_for(&provider, "error frame", |s| waiting(s).is_some()).await;
        let (failure, _) = waiting(&status).unwrap();
        if rejected {
            assert_eq!(*failure, Failure::Rejected { status: None });
        } else {
            assert!(matches!(failure, Failure::RateLimited { .. }), "{failure:?}");
        }
        let message = fetch(&provider, &secrets).await.unwrap_err().public_message();
        assert!(!message.contains(KEY), "{message}");
        assert_eq!(status.sessions, 0, "an error frame is never a working stream");
    }
}

#[tokio::test]
async fn a_silent_connection_is_replaced() {
    let server = ws_server(|n, mut ws| async move {
        if n == 1 {
            // Accept the subscription, then say nothing.
            tokio::time::sleep(Duration::from_secs(30)).await;
        } else {
            ws.send(Message::binary(position(230_990_003, 59.5, 24.7).into_bytes())).await.unwrap();
            while ws.next().await.is_some() {}
        }
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig { read_timeout_s: 1, ..config(server.port) },
        fetcher(),
    );
    fetch(&provider, &with_key()).await.unwrap();
    let status =
        wait_for(&provider, "second session", |s| s.sessions == 1 && s.attempts == 2).await;
    assert!(matches!(status.state, LinkState::Streaming { .. }), "{status:?}");
    assert_eq!(provider.store().stats().positioned, 1);
    assert_eq!(server.connections.load(Ordering::SeqCst), 2);
}

/// A harbour-sized area for subscriptions that may legitimately be quiet.
fn harbour() -> Vec<BBox> {
    vec![BBox::new(24.9, 60.1, 25.0, 60.2).unwrap()]
}

#[tokio::test]
async fn a_quiet_narrow_subscription_stays_connected_while_it_answers_pings() {
    let server = ws_server(|_, mut ws| async move {
        let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
        // Nobody reports in this harbour: the server only answers pings (while reading).
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig { bounding_boxes: harbour(), read_timeout_s: 1, ..config(server.port) },
        fetcher(),
    );
    let secrets = with_key();
    fetch(&provider, &secrets).await.unwrap();
    wait_for(&provider, "streaming", |s| matches!(s.state, LinkState::Streaming { .. })).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let status = provider.link_status();
    assert!(matches!(status.state, LinkState::Streaming { .. }), "{status:?}");
    assert_eq!((status.attempts, &status.last_failure), (1, &None), "{status:?}");
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
    let fetched = fetch(&provider, &secrets).await.unwrap();
    assert_eq!(fetched.degraded, None, "a quiet area is live, not failing");
}

#[tokio::test]
async fn a_narrow_subscription_whose_connection_stops_answering_is_replaced() {
    let server = ws_server(|_, mut ws| async move {
        let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
        // Then stop reading: pings go unanswered, as on a dead connection.
        tokio::time::sleep(Duration::from_secs(30)).await;
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig {
            bounding_boxes: harbour(),
            read_timeout_s: 1,
            // Wait about a second before reconnecting, so the failure can be observed.
            reconnect_min_ms: 1_000,
            ..config(server.port)
        },
        fetcher(),
    );
    fetch(&provider, &with_key()).await.unwrap();
    let failed = wait_for(&provider, "the dead connection", |s| s.last_failure.is_some()).await;
    assert_eq!(
        failed.last_failure,
        Some(Failure::Transport("no response for 1 s".to_owned())),
        "{failed:?}"
    );
    assert_eq!(failed.sessions, 1, "it streamed before it went dead");
    wait_for(&provider, "a new session", |s| s.sessions >= 2).await;
    assert!(server.connections.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn a_worldwide_subscription_that_stops_reporting_is_replaced() {
    let server = ws_server(|n, mut ws| async move {
        // The first connection confirms and then only answers pings; later ones do not
        // even confirm, so the first failure stays visible.
        if n == 1 {
            let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
        }
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig { read_timeout_s: 1, ..config(server.port) },
        fetcher(),
    );
    fetch(&provider, &with_key()).await.unwrap();
    let status =
        wait_for(&provider, "a replaced session", |s| s.attempts >= 2 && s.last_failure.is_some())
            .await;
    assert_eq!(status.sessions, 1, "{status:?}");
    assert_eq!(
        status.last_failure,
        Some(Failure::Transport("no AIS messages for 1 s".to_owned())),
        "{status:?}"
    );
}

#[tokio::test]
async fn a_stream_lavender_cannot_read_is_a_failure_not_a_pending_connection() {
    let server = ws_server(|_, mut ws| async move {
        // A changed envelope: every message is unreadable.
        let unreadable = br#"{"MessageType":"PositionReport","Message":{"Position":{}}}"#;
        for _ in 0..60 {
            if ws.send(Message::binary(unreadable.to_vec())).await.is_err() {
                return;
            }
        }
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig { bounding_boxes: harbour(), ..config(server.port) },
        fetcher(),
    );
    let secrets = with_key();
    fetch(&provider, &secrets).await.unwrap();
    let status = wait_for(&provider, "a failure", |s| s.last_failure.is_some()).await;
    assert_eq!(
        status.last_failure,
        Some(Failure::Transport(
            "AISStream sent 50 messages in a row that Lavender could not read".to_owned()
        ))
    );
    assert_eq!(status.sessions, 0, "unreadable messages never prove the stream");
    let message = fetch(&provider, &secrets).await.unwrap_err().public_message();
    assert!(message.contains("could not read"), "{message}");
}

#[tokio::test]
async fn skipped_malformed_messages_are_reported_with_the_vessels() {
    let server = ws_server(|_, mut ws| async move {
        let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
        for frame in ["not json", r#"{"MessageType":"PositionReport","Message":{}}"#] {
            let _ = ws.send(Message::binary(frame.as_bytes().to_vec())).await;
        }
        let _ = ws.send(Message::binary(position(230_990_012, 60.15, 24.95).into_bytes())).await;
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = AisStreamProvider::new(config(server.port), fetcher());
    let secrets = with_key();
    fetch(&provider, &secrets).await.unwrap();
    wait_for(&provider, "all four frames", |s| s.frames >= 4).await;
    let fetched = fetch(&provider, &secrets).await.unwrap();
    assert!(
        fetched
            .notices
            .iter()
            .any(|n| n == "AISStream: 2 malformed messages skipped since connecting."),
        "{:?}",
        fetched.notices
    );
    let FeedItems::Vessels(vessels) = &fetched.items else { panic!("vessels") };
    assert_eq!(vessels.len(), 1);
}

#[tokio::test]
async fn tracks_age_out_after_the_link_goes_idle() {
    let server = ws_server(|_, mut ws| async move {
        let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
        let _ = ws.send(Message::binary(position(230_990_011, 60.1, 24.9).into_bytes())).await;
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = Arc::new(AisStreamProvider::new(
        AisStreamConfig { retention_s: 2, idle_disconnect_s: 1, ..config(server.port) },
        fetcher(),
    ));
    fetch(&provider, &with_key()).await.unwrap();
    wait_for(&provider, "the report", |s| s.frames >= 2).await;
    let core = LavenderCore::builder(CoreConfig::default())
        .providers(vec![provider.clone() as Arc<dyn FeedProvider>])
        .secrets(Arc::new(with_key()))
        .build()
        .unwrap();
    assert_eq!(core.vessel_track("230990011").unwrap().samples.len(), 1, "fresh: retained");
    // Nobody asks for vessels: the link goes idle, and the store hears nothing more.
    wait_for(&provider, "idle", |s| s.state == LinkState::Idle).await;
    tokio::time::sleep(Duration::from_millis(3_200)).await;
    let error = core.vessel_track("230990011").unwrap_err();
    assert!(matches!(error, lavender_core::CoreError::NotFound(_)), "{error:?}");
    assert_eq!(provider.store().stats().vessels, 0, "the old ship is gone from memory too");
    core.shutdown();
}

#[tokio::test]
async fn oversized_frames_end_the_session() {
    let server = ws_server(|n, mut ws| async move {
        if n == 1 {
            let _ = ws.send(Message::binary(vec![b' '; (1 << 20) + 1])).await;
        }
        while ws.next().await.is_some() {}
    })
    .await;
    let provider = AisStreamProvider::new(config(server.port), fetcher());
    fetch(&provider, &with_key()).await.unwrap();
    let status = wait_for(&provider, "reconnect", |s| s.attempts >= 2).await;
    assert_eq!(
        status.last_failure,
        Some(Failure::Transport("AISStream sent a frame larger than 1 MiB".to_owned()))
    );
}

#[tokio::test]
async fn the_link_disconnects_when_vessels_are_no_longer_wanted() {
    let closed = Arc::new(AtomicUsize::new(0));
    let seen = closed.clone();
    let server = ws_server(move |_, mut ws| {
        let seen = seen.clone();
        async move {
            let _ = ws.send(Message::binary(CONFIRMATION.as_bytes().to_vec())).await;
            while ws.next().await.is_some() {}
            seen.fetch_add(1, Ordering::SeqCst);
        }
    })
    .await;
    let provider = AisStreamProvider::new(
        AisStreamConfig { idle_disconnect_s: 1, ..config(server.port) },
        fetcher(),
    );
    fetch(&provider, &with_key()).await.unwrap();
    wait_for(&provider, "streaming", |s| matches!(s.state, LinkState::Streaming { .. })).await;
    wait_for(&provider, "idle", |s| s.state == LinkState::Idle).await;
    for _ in 0..100 {
        if closed.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(closed.load(Ordering::SeqCst), 1, "the server saw the connection close");
    // The next request for vessels connects again.
    fetch(&provider, &with_key()).await.unwrap();
    wait_for(&provider, "second session", |s| s.sessions == 2).await;
}

#[tokio::test]
async fn without_a_key_nothing_connects_and_the_source_is_unconfigured() {
    let server = ws_server(|_, _| async {}).await;
    let provider = Arc::new(AisStreamProvider::new(config(server.port), fetcher()));
    let none = MemorySecretStore::default();
    let error = fetch(&provider, &none).await.unwrap_err();
    assert!(
        matches!(&error, ProviderError::Unconfigured(m) if m.contains("LAVENDER_AISSTREAM_API_KEY")),
        "{error:?}"
    );
    let core = LavenderCore::builder(CoreConfig::default())
        .providers(vec![provider.clone() as Arc<dyn FeedProvider>])
        .build()
        .unwrap();
    let layer = core.catalog().layers.into_iter().find(|l| l.feed == FeedId::Vessels).unwrap();
    assert_eq!(
        layer.availability,
        lavender_domain::Availability::Unconfigured {
            missing: vec!["AISStream API key".to_owned()]
        }
    );
    assert!(core.subscribe(FeedQuery::new(FeedId::Vessels)).is_err());
    let health = core.status().sources.into_iter().find(|s| s.source_id == "aisstream").unwrap();
    assert_eq!(health.state, lavender_domain::HealthState::Idle);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
    assert_eq!(provider.link_status().attempts, 0);
    core.shutdown();
}
