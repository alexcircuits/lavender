#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::assert_is_empty
)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use lavender_server::local_receiver_daemon::{
    LocalReceiverDaemon, LocalReceiverStreamFormat, LocalReceiverTcpEndpoint, parse_tcp_endpoints,
    run_tcp_worker,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(uri).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn post_bytes(
    app: &axum::Router,
    uri: &str,
    body: Vec<u8>,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::HOST, "localhost:7410")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn http_local_receiver_aircraft_and_json_ingest() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // Initial state: unconfigured
    let (status, json) = get_json(&app, "/api/v1/local-receivers/aircraft").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["configured"], false);
    assert_eq!(json["stale"], true);
    assert_eq!(json["aircraft"].as_array().unwrap().len(), 0);

    // Ingest valid dump1090 JSON
    let now_s = jiff::Timestamp::now().as_second() as f64;
    let payload = serde_json::json!({
        "now": now_s,
        "aircraft": [
            {
                "hex": "a1b2c3",
                "flight": "UAL123",
                "lat": 37.6188,
                "lon": -122.375,
                "alt_baro": 34000,
                "seen": 0.5
            }
        ]
    });
    let (status, res) = post_bytes(
        &app,
        "/api/v1/local-receivers/ingest?feedId=feed-1&band=1090",
        serde_json::to_vec(&payload).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["ingested"], 1);

    // Snapshot reflects ingested aircraft
    let (status, json) = get_json(&app, "/api/v1/local-receivers/aircraft").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["configured"], true);
    assert_eq!(json["stale"], false);
    let aircraft = json["aircraft"].as_array().unwrap();
    assert_eq!(aircraft.len(), 1);
    assert_eq!(aircraft[0]["hex"], "a1b2c3");
    assert_eq!(aircraft[0]["callsign"], "UAL123");

    // Rejects invalid band
    let (status, _) = post_bytes(
        &app,
        "/api/v1/local-receivers/ingest?feedId=feed-1&band=invalid",
        serde_json::to_vec(&payload).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn http_local_receiver_raw_avr_and_beast_ingest() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let app = lavender_server::app(core, &ServerSettings::default());

    // Ingest raw Mode-S AVR hex frame for KLM1023
    let avr_frame = b"*8d4840d6202cc371c32ce0576098;\n".to_vec();
    let (status, res) =
        post_bytes(&app, "/api/v1/local-receivers/ingest?feedId=raw-1090&band=1090", avr_frame)
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["ingested"], 1);

    // Snapshot contains KLM1023
    let (status, json) = get_json(&app, "/api/v1/local-receivers/aircraft").await;
    assert_eq!(status, StatusCode::OK);
    let aircraft = json["aircraft"].as_array().unwrap();
    assert_eq!(aircraft.len(), 1);
    assert_eq!(aircraft[0]["hex"], "4840d6");
    assert_eq!(aircraft[0]["callsign"], "KLM1023");

    // Ingest Beast binary Mode-S Short packet
    let mut beast_packet = vec![0x1a, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xcc];
    beast_packet.extend_from_slice(&[0x5d, 0xae, 0x04, 0x56, 0x20, 0x2c, 0xc3]);
    let (status, res) =
        post_bytes(&app, "/api/v1/local-receivers/ingest?feedId=raw-1090&band=1090", beast_packet)
            .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(res["ingested"], 1);

    // Snapshot now has both aircraft
    let (_, json) = get_json(&app, "/api/v1/local-receivers/aircraft").await;
    assert_eq!(json["aircraft"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn tcp_local_receiver_daemon_streaming_and_ssrf() {
    // 1. Start mock TCP receiver server on loopback
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            // Stream raw AVR hex frame
            let _ = stream.write_all(b"*8d4840d6202cc371c32ce0576098;\n").await;
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Stream Beast binary packet
            let mut beast = vec![0x1a, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xaa];
            beast.extend_from_slice(&[0x5d, 0x33, 0x44, 0x55, 0x20, 0x2c, 0xc3]);
            let _ = stream.write_all(&beast).await;
        }
    });

    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let engine = core.local_receivers();
    let cancel = CancellationToken::new();

    // 2. Run TCP worker connecting to the mock receiver (AVR format)
    let endpoint = LocalReceiverTcpEndpoint {
        host: "127.0.0.1".to_owned(),
        port: server_addr.port(),
        format: LocalReceiverStreamFormat::Avr,
    };
    let cancel_clone = cancel.clone();
    let engine_clone = Arc::clone(&engine);
    let worker_handle = tokio::spawn(async move {
        run_tcp_worker(endpoint, engine_clone, cancel_clone).await;
    });

    // Wait for frames to stream in
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();
    let _ = worker_handle.await;
    let _ = server_task.await;

    // Verify snapshot received the aircraft streamed over TCP
    let snap = engine.snapshot(jiff::Timestamp::now());
    assert!(snap.configured);
    assert!(!snap.stale);
    assert!(!snap.aircraft.is_empty());
    assert_eq!(snap.aircraft[0].hex, "4840d6");
    assert_eq!(snap.aircraft[0].callsign.as_deref(), Some("KLM1023"));

    // 3. Test SSRF protection
    let public_ep = LocalReceiverTcpEndpoint::parse("8.8.8.8:30005");
    assert!(public_ep.is_err());
    assert!(public_ep.unwrap_err().contains("not an allowed local receiver target"));

    let cloud_metadata_ep = LocalReceiverTcpEndpoint::parse("169.254.169.254:30005");
    assert!(cloud_metadata_ep.is_err());

    // 4. Test endpoint parsing variations
    let parsed = parse_tcp_endpoints(
        "127.0.0.1:30002, 192.168.1.100:30005=beast, avr://[::1]:30002, 8.8.8.8:30005",
    );
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].format, LocalReceiverStreamFormat::Avr);
    assert_eq!(parsed[1].format, LocalReceiverStreamFormat::Beast);
    assert_eq!(parsed[2].host, "::1");
    assert_eq!(parsed[2].format, LocalReceiverStreamFormat::Avr);

    // 5. Test daemon spawn and shutdown
    let daemon = LocalReceiverDaemon::spawn(vec![], &engine);
    daemon.shutdown();
}
