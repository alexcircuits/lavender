//! `lavender-server` integration tests (ENG-946 / ENG-1002 acceptance criteria): HTTP via
//! the real router in-process, the WebSocket protocol against a real listening server.

// Test helpers outside #[test] fns are not covered by clippy.toml's test allowances.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use lavender_core::config::{CoreConfig, HubConfig};
use lavender_core::providers::{FeedProvider, FetchContext, Fetched, ProviderError, Region};
use lavender_core::{LavenderCore, sources};
use lavender_domain::{Earthquake, FeedId, FeedItems, GeoPoint, ProviderScope, ServerFrame};
use lavender_server::config::ServerSettings;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tower::ServiceExt;

#[derive(Debug)]
struct Quakes;

#[async_trait]
impl FeedProvider for Quakes {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(60)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(&self, _: &FetchContext<'_>, _: Region) -> Result<Fetched, ProviderError> {
        let q = |id: &str, lat: f64, lon: f64, magnitude: f64| Earthquake {
            id: id.into(),
            magnitude,
            magnitude_type: None,
            depth_km: 5.0,
            position: GeoPoint::new(lat, lon).unwrap(),
            place: Some(id.into()),
            time: "2026-10-11T00:00:00Z".parse().unwrap(),
            updated: None,
            url: None,
            tsunami: false,
            alert: None,
            felt: None,
            significance: None,
            status: None,
        };
        Ok(FeedItems::Earthquakes(vec![
            q("fiji", -17.0, 179.5, 5.1),
            q("helsinki", 60.2, 24.9, 2.6),
        ])
        .into())
    }
}

/// A regional aircraft provider: one aircraft at each cell's centre.
#[derive(Debug)]
struct Planes;

#[async_trait]
impl FeedProvider for Planes {
    fn feed(&self) -> FeedId {
        FeedId::Aircraft
    }
    fn source_id(&self) -> &'static str {
        sources::ADSB_LOL
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(60)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Regional { cell_deg: 5.0, max_cells: 4 }
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(&self, _: &FetchContext<'_>, region: Region) -> Result<Fetched, ProviderError> {
        let Region::Cell(cell) = region else { return Ok(FeedItems::Aircraft(vec![]).into()) };
        let c = cell.center();
        let plane: lavender_domain::Aircraft = serde_json::from_value(serde_json::json!({
            "hex": "4ca1b2", "position": {"lat": c.lat, "lon": c.lon}, "onGround": false,
            "isMilitary": false, "seenPosS": 0.0, "source": "adsb-lol"
        }))
        .unwrap();
        Ok(FeedItems::Aircraft(vec![plane]).into())
    }
}

/// An earthquake provider that takes longer than any request timeout in these tests.
#[derive(Debug)]
struct Slow;

#[async_trait]
impl FeedProvider for Slow {
    fn feed(&self) -> FeedId {
        FeedId::Earthquakes
    }
    fn source_id(&self) -> &'static str {
        sources::USGS
    }
    fn cadence(&self) -> Duration {
        Duration::from_secs(60)
    }
    fn scope(&self) -> ProviderScope {
        ProviderScope::Global
    }
    fn enabled(&self) -> bool {
        true
    }
    async fn fetch(&self, _: &FetchContext<'_>, _: Region) -> Result<Fetched, ProviderError> {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(FeedItems::Earthquakes(vec![]).into())
    }
}

struct Fixture {
    app: axum::Router,
    core: LavenderCore,
    _static_dir: tempfile::TempDir,
}

fn fixture_with(settings: impl FnOnce(&mut ServerSettings)) -> Fixture {
    fixture_full(vec![Arc::new(Quakes)], &HubConfig::default(), settings)
}

fn fixture_full(
    providers: Vec<Arc<dyn FeedProvider>>,
    hub: &HubConfig,
    settings: impl FnOnce(&mut ServerSettings),
) -> Fixture {
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<!doctype html><title>Lavender</title>")
        .unwrap();
    std::fs::create_dir(static_dir.path().join("assets")).unwrap();
    std::fs::write(static_dir.path().join("assets/app-3f9a.js"), "console.log(1)").unwrap();
    let config =
        CoreConfig { hub: HubConfig { linger_s: 60, ..hub.clone() }, ..CoreConfig::default() };
    let core = LavenderCore::builder(config).providers(providers).build().unwrap();
    let mut s =
        ServerSettings { static_dir: static_dir.path().to_owned(), ..ServerSettings::default() };
    settings(&mut s);
    Fixture { app: lavender_server::app(core.clone(), &s), core, _static_dir: static_dir }
}

fn fixture() -> Fixture {
    fixture_with(|_| {})
}

fn get(path: &str) -> axum::http::request::Builder {
    Request::get(path).header(header::HOST, "localhost:7410")
}

async fn send(
    app: &axum::Router,
    req: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or(serde_json::Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, headers, json)
}

#[tokio::test]
async fn health_status_and_catalog() {
    let f = fixture();
    let (s, _, body) = send(&f.app, get("/healthz").body(Body::empty()).unwrap()).await;
    assert_eq!((s, body), (StatusCode::OK, serde_json::json!("ok")));
    let (s, _, body) = send(&f.app, get("/api/v1/status").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["licenceMode"], "evaluation");
    let (s, _, body) = send(&f.app, get("/api/v1/catalog").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["layers"][0]["feed"], "earthquakes");
}

#[tokio::test]
async fn feed_snapshot_with_etag_revalidation() {
    let f = fixture();
    let (s, headers, body) = send(
        &f.app,
        get("/api/v1/feeds/earthquakes?bbox=170,-30,-170,0&minMagnitude=4")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"]["kind"], "earthquakes");
    assert_eq!(body["items"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"]["items"][0]["id"], "fiji");
    let etag = headers.get(header::ETAG).unwrap().to_str().unwrap().to_owned();
    assert!(etag.starts_with("W/\""));
    let req = get("/api/v1/feeds/earthquakes?bbox=170,-30,-170,0&minMagnitude=4")
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    let (s, _, _) = send(&f.app, req).await;
    assert_eq!(s, StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn errors_use_the_api_envelope_and_status_codes() {
    let f = fixture();
    let (s, _, body) = send(&f.app, get("/api/v1/feeds/ufos").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "notFound");
    let (s, _, body) =
        send(&f.app, get("/api/v1/feeds/earthquakes?bbox=0,95,1,96").body(Body::empty()).unwrap())
            .await;
    assert_eq!((s, body["code"].as_str()), (StatusCode::BAD_REQUEST, Some("invalidRequest")));
    let (s, _, body) =
        send(&f.app, get("/api/v1/feeds/aircraft").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{body}");
    let (s, _, body) = send(&f.app, get("/api/v2/anything").body(Body::empty()).unwrap()).await;
    assert_eq!((s, body["code"].as_str()), (StatusCode::NOT_FOUND, Some("notFound")));
    let (s, _, _) = send(&f.app, get("/api/v1/search?q=").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, body) =
        send(&f.app, get("/api/v1/search?q=60.17,%2024.94").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["coordinates"][0]["format"], "decimal");
}

#[tokio::test]
async fn foreign_hosts_and_cross_site_origins_are_refused() {
    let f = fixture();
    let req = Request::get("/api/v1/status")
        .header(header::HOST, "evil.example")
        .body(Body::empty())
        .unwrap();
    let (s, _, body) = send(&f.app, req).await;
    assert_eq!((s, body["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")));
    let req = Request::get("/")
        .header(header::HOST, "attacker.example:7410")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&f.app, req).await.0,
        StatusCode::FORBIDDEN,
        "rebinding guard covers static files too"
    );
    let req = get("/api/v1/status")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::FORBIDDEN);
    let req =
        get("/api/v1/status").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::FORBIDDEN);
    let req = get("/api/v1/status")
        .header(header::ORIGIN, "http://localhost:7410")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_returns_429_with_retry_after() {
    let f = fixture_with(|s| {
        s.rate_limit_per_s = 1;
        s.rate_limit_burst = 2;
    });
    for _ in 0..2 {
        assert_eq!(
            send(&f.app, get("/api/v1/status").body(Body::empty()).unwrap()).await.0,
            StatusCode::OK
        );
    }
    let (s, headers, body) = send(&f.app, get("/api/v1/status").body(Body::empty()).unwrap()).await;
    assert_eq!((s, body["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rateLimited")));
    assert!(headers.contains_key(header::RETRY_AFTER));
}

#[tokio::test]
async fn static_files_spa_fallback_caching_and_security_headers() {
    let f = fixture();
    let (s, headers, body) = send(&f.app, get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.as_str().unwrap().contains("<title>Lavender</title>"));
    let csp = headers.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap();
    assert!(
        csp.contains("script-src 'self' 'wasm-unsafe-eval'") && !csp.contains("'unsafe-eval'"),
        "{csp}"
    );
    assert!(csp.contains("https://gibs.earthdata.nasa.gov"), "{csp}");
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-cache");
    assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    assert_eq!(headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");

    let (s, headers, _) =
        send(&f.app, get("/assets/app-3f9a.js").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "public, max-age=31536000, immutable");

    let (s, _, body) =
        send(&f.app, get("/workspace/some/deep/link").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.as_str().unwrap().contains("Lavender"), "SPA fallback serves index.html");

    let (s, _, _) = send(&f.app, get("/assets/missing-0000.js").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "a missing bundle is a 404, not index.html");
}

fn assert_envelope(status: StatusCode, body: &serde_json::Value, expected: StatusCode, code: &str) {
    assert_eq!(status, expected, "{body}");
    assert_eq!(body["code"], code, "{body}");
    assert!(body["message"].as_str().is_some_and(|m| !m.is_empty()), "{body}");
}

#[tokio::test]
async fn query_and_path_rejections_use_the_api_envelope() {
    // Review finding 12: axum's own rejections were text/plain.
    let f = fixture();
    let (s, _, body) =
        send(&f.app, get("/api/v1/search?q=x&limit=lots").body(Body::empty()).unwrap()).await;
    assert_envelope(s, &body, StatusCode::BAD_REQUEST, "invalidRequest");
    let (s, _, body) =
        send(&f.app, get("/api/v1/satellites/25544/track?minutes=-5").body(Body::empty()).unwrap())
            .await;
    assert_envelope(s, &body, StatusCode::BAD_REQUEST, "invalidRequest");
}

#[tokio::test]
async fn non_websocket_requests_to_the_stream_use_the_api_envelope() {
    let f = fixture();
    let (s, _, body) = send(&f.app, get("/api/v1/stream").body(Body::empty()).unwrap()).await;
    assert!(s.is_client_error(), "{s}");
    assert_eq!(body["code"], "invalidRequest", "{body}");
}

#[tokio::test]
async fn method_mismatches_and_oversized_bodies_use_the_api_envelope() {
    let f = fixture();
    let req = Request::post("/api/v1/status")
        .header(header::HOST, "localhost:7410")
        .body(Body::empty())
        .unwrap();
    let (s, headers, body) = send(&f.app, req).await;
    assert_envelope(s, &body, StatusCode::METHOD_NOT_ALLOWED, "invalidRequest");
    assert!(headers.contains_key(header::ALLOW));
    let req = Request::post("/api/v1/status")
        .header(header::HOST, "localhost:7410")
        .header(header::CONTENT_LENGTH, "100000")
        .body(Body::from(vec![b'x'; 100_000]))
        .unwrap();
    let (s, _, body) = send(&f.app, req).await;
    assert!(s.is_client_error(), "{s}");
    assert_eq!(body["code"], "invalidRequest", "{body}");
}

#[tokio::test]
async fn timeouts_use_the_api_envelope() {
    // Review finding 12: the timeout layer answered 504 with an empty body.
    let f = fixture_full(vec![Arc::new(Slow)], &HubConfig::default(), |s| s.request_timeout_s = 1);
    let (s, _, body) =
        send(&f.app, get("/api/v1/feeds/earthquakes").body(Body::empty()).unwrap()).await;
    assert_envelope(s, &body, StatusCode::GATEWAY_TIMEOUT, "upstreamUnavailable");
}

#[tokio::test]
async fn hub_capacity_limits_are_429_not_502() {
    // Review finding 12: core.snapshot mapped a hub limit to an upstream error.
    let f = fixture_full(
        vec![Arc::new(Quakes), Arc::new(Planes)],
        &HubConfig { max_slots: 1, ..HubConfig::default() },
        |_| {},
    );
    let mut hog = f.core.subscribe(lavender_domain::FeedQuery::new(FeedId::Earthquakes)).unwrap();
    tokio::time::timeout(Duration::from_secs(5), hog.next()).await.unwrap();
    let (s, _, body) = send(
        &f.app,
        get("/api/v1/feeds/aircraft?bbox=20.5,60.5,24.5,64.5").body(Body::empty()).unwrap(),
    )
    .await;
    assert_envelope(s, &body, StatusCode::TOO_MANY_REQUESTS, "rateLimited");
    assert_eq!(body["retryable"], true);
}

#[tokio::test]
async fn host_refusals_carry_the_security_headers() {
    // Review finding 12: the host check ran outside the header layers.
    let f = fixture();
    let req =
        Request::get("/").header(header::HOST, "attacker.example").body(Body::empty()).unwrap();
    let (s, headers, body) = send(&f.app, req).await;
    assert_envelope(s, &body, StatusCode::FORBIDDEN, "forbidden");
    assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
    assert_eq!(headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
}

#[tokio::test]
async fn requests_without_a_host_header_use_the_target_authority() {
    // Review finding 12: HTTP/2 (`:authority`) and absolute-form requests have no Host
    // header and were refused.
    let f = fixture();
    let req = Request::get("http://localhost:7410/api/v1/status").body(Body::empty()).unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::OK);
    let req = Request::get("http://evil.example/api/v1/status")
        .header(header::HOST, "localhost:7410")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&f.app, req).await.0, StatusCode::FORBIDDEN);
}

async fn serve(f: &Fixture) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = f.app.clone();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    });
    addr
}

async fn next_frame(
    ws: &mut (
             impl futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin
         ),
) -> ServerFrame {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("frame in time")
            .unwrap()
            .unwrap();
        if let Message::Text(t) = msg {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

#[tokio::test]
async fn websocket_subscribe_update_errors_and_unsubscribe() {
    let f = fixture_with(|s| s.max_subscriptions_per_connection = 2);
    let addr = serve(&f).await;
    let mut req =
        format!("ws://127.0.0.1:{}/api/v1/stream", addr.port()).into_client_request().unwrap();
    req.headers_mut().insert(header::HOST, format!("localhost:{}", addr.port()).parse().unwrap());
    req.headers_mut()
        .insert(header::ORIGIN, format!("http://localhost:{}", addr.port()).parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();

    ws.send(Message::Text(r#"{"op":"subscribe","id":"q","query":{"feed":"earthquakes"}}"#.into()))
        .await
        .unwrap();
    let ServerFrame::Snapshot { id, snapshot } = next_frame(&mut ws).await else {
        panic!("expected snapshot")
    };
    assert_eq!((id.as_str(), snapshot.items.len()), ("q", 2));

    ws.send(Message::Text(r#"{"op":"update","id":"q","query":{"feed":"earthquakes","bbox":{"west":20,"south":55,"east":30,"north":65}}}"#.into())).await.unwrap();
    let ServerFrame::Snapshot { snapshot, .. } = next_frame(&mut ws).await else {
        panic!("expected snapshot")
    };
    assert_eq!(snapshot.items.len(), 1, "viewport update re-filtered");

    ws.send(Message::Text("not json".into())).await.unwrap();
    let ServerFrame::Error { id, error } = next_frame(&mut ws).await else {
        panic!("expected error")
    };
    assert_eq!((id, error.code), (None, lavender_domain::ErrorCode::InvalidRequest));

    ws.send(Message::Text(r#"{"op":"subscribe","id":"q","query":{"feed":"earthquakes"}}"#.into()))
        .await
        .unwrap();
    let ServerFrame::Error { error, .. } = next_frame(&mut ws).await else {
        panic!("duplicate id refused")
    };
    assert_eq!(error.code, lavender_domain::ErrorCode::InvalidRequest);

    ws.send(Message::Text(r#"{"op":"subscribe","id":"a","query":{"feed":"aircraft"}}"#.into()))
        .await
        .unwrap();
    let ServerFrame::Error { id, error } = next_frame(&mut ws).await else {
        panic!("unknown feed refused")
    };
    assert_eq!((id.as_deref(), error.code), (Some("a"), lavender_domain::ErrorCode::NotFound));

    ws.send(Message::Text(r#"{"op":"unsubscribe","id":"q"}"#.into())).await.unwrap();
    ws.close(None).await.unwrap();
}

async fn connect(addr: SocketAddr) -> Result<WsStream, tokio_tungstenite::tungstenite::Error> {
    let mut req =
        format!("ws://127.0.0.1:{}/api/v1/stream", addr.port()).into_client_request().unwrap();
    req.headers_mut().insert(header::HOST, format!("localhost:{}", addr.port()).parse().unwrap());
    tokio_tungstenite::connect_async(req).await.map(|(ws, _)| ws)
}

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Reads until the server's close frame and returns its code.
async fn close_code(ws: &mut WsStream) -> u16 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
                Some(Ok(_)) => {}
                other => panic!("connection ended without a close frame: {other:?}"),
            }
        }
    })
    .await
    .expect("close frame in time")
}

#[tokio::test]
async fn websocket_message_floods_are_closed() {
    // Review finding 1/7: there was no inbound message rate limit.
    let f = fixture_with(|s| {
        s.ws_messages_per_s = 1;
        s.ws_message_burst = 5;
    });
    let addr = serve(&f).await;
    let mut ws = connect(addr).await.unwrap();
    for _ in 0..20 {
        if ws.send(Message::Text(r#"{"op":"unsubscribe","id":"x"}"#.into())).await.is_err() {
            break;
        }
    }
    assert_eq!(close_code(&mut ws).await, 1008, "policy violation");
}

#[tokio::test]
async fn idle_websockets_are_closed() {
    // Review finding 7: a client that stopped answering was kept forever.
    let f = fixture_with(|s| s.ws_idle_timeout_s = 1);
    let addr = serve(&f).await;
    let mut ws = connect(addr).await.unwrap();
    // Not reading means not answering the server's pings.
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(close_code(&mut ws).await, 1001, "going away: idle");
}

#[tokio::test]
async fn websocket_connections_are_capped_per_client() {
    // Review finding 7: there was no cap on concurrent connections.
    let f = fixture_with(|s| s.max_connections_per_client = 1);
    let addr = serve(&f).await;
    let _first = connect(addr).await.unwrap();
    match connect(addr).await.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            let body: serde_json::Value =
                serde_json::from_slice(response.body().as_deref().unwrap_or_default()).unwrap();
            assert_eq!(body["code"], "rateLimited");
        }
        other => panic!("expected an HTTP refusal, got {other}"),
    }
}

#[tokio::test]
async fn websocket_from_a_foreign_origin_is_refused() {
    let f = fixture();
    let addr = serve(&f).await;
    let mut req =
        format!("ws://127.0.0.1:{}/api/v1/stream", addr.port()).into_client_request().unwrap();
    req.headers_mut().insert(header::HOST, format!("localhost:{}", addr.port()).parse().unwrap());
    req.headers_mut().insert(header::ORIGIN, "https://evil.example".parse().unwrap());
    let err = tokio_tungstenite::connect_async(req).await.unwrap_err();
    assert!(err.to_string().contains("403"), "{err}");
}
