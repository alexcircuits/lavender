#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreadable_literal,
    clippy::cast_precision_loss
)]

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jiff::Timestamp;
use lavender_core::{CoreConfig, LavenderCore};
use lavender_domain::{GnssLevel, GnssProvenance, GnssSnapshot};
use lavender_server::config::ServerSettings;
use serde_json::json;
use tower::ServiceExt;

async fn get_request(app: &axum::Router, uri: &str) -> (StatusCode, Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::get(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

async fn post_bytes(app: &axum::Router, uri: &str, body: Vec<u8>) -> (StatusCode, Bytes) {
    let response = app
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::HOST, "localhost:7410")
                .header(header::ORIGIN, "http://localhost:7410")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

#[tokio::test]
async fn http_gnss_endpoints_and_filtering() {
    let core = LavenderCore::builder(CoreConfig::default()).build().unwrap();
    let settings = ServerSettings::default();
    let app = lavender_server::app(core, &settings);

    // 1. Provenance endpoint
    let (status, body) = get_request(&app, "/api/v1/gnss/provenance").await;
    assert_eq!(status, StatusCode::OK);
    let prov: GnssProvenance = serde_json::from_slice(&body).unwrap();
    assert_eq!(prov.classifier_id, "gev-nic-nacp-v2");
    assert_eq!(prov.min_aircraft, 3);
    assert_eq!(prov.window_minutes, 30);

    // 2. Cells endpoint initially empty
    let (status, body) = get_request(&app, "/api/v1/gnss/cells").await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: GnssSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(snapshot.total_cells, 0);

    // 3. Ingest readsb snapshot with 3 aircraft in Tallinn cell (lat 59.4, lon 24.7)
    let current_epoch = Timestamp::now().as_second() as f64;
    let payload = json!({
        "now": current_epoch,
        "ac": [
            {
                "hex": "4ca001",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 30000,
                "lat": 59.43,
                "lon": 24.75,
                "seen_pos": 1.0,
                "nic": 8,
                "nac_p": 9,
                "gpsOkBefore": 0
            },
            {
                "hex": "4ca002",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 32000,
                "lat": 59.44,
                "lon": 24.76,
                "seen_pos": 2.0,
                "nic": 5, // degraded
                "nac_p": 8,
                "gpsOkBefore": 0
            },
            {
                "hex": "4ca003",
                "type": "adsb_icao",
                "version": 2,
                "alt_baro": 34000,
                "lat": 59.42,
                "lon": 24.74,
                "seen_pos": 3.0,
                "nic": 4, // degraded
                "nac_p": 6,
                "gpsOkBefore": 0
            }
        ]
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    let (status, body) = post_bytes(&app, "/api/v1/gnss/ingest", bytes).await;
    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["ingested"], 3);

    // 4. Query cells after ingestion
    let (status, body) = get_request(&app, "/api/v1/gnss/cells").await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: GnssSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(snapshot.total_cells, 1);
    assert_eq!(snapshot.cells[0].aircraft, 3);
    assert_eq!(snapshot.cells[0].degraded, 2);
    assert_eq!(snapshot.cells[0].level, GnssLevel::High);

    // 5. Query with matching bounding box
    let (status, body) = get_request(&app, "/api/v1/gnss/cells?bbox=24.0,59.0,26.0,60.0").await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: GnssSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(snapshot.total_cells, 1);

    // 6. Query with non-matching bounding box
    let (status, body) = get_request(&app, "/api/v1/gnss/cells?bbox=0.0,0.0,1.0,1.0").await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: GnssSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(snapshot.total_cells, 0);

    // 7. Legacy parity endpoint: /api/gnss-integrity
    let (status, body) = get_request(&app, "/api/gnss-integrity?lat=59.4&lon=24.7").await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: GnssSnapshot = serde_json::from_slice(&body).unwrap();
    assert_eq!(snapshot.total_cells, 1);
}
