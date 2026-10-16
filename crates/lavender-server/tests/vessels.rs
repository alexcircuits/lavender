//! `GET /api/v1/vessels/{mmsi}/track` and the vessels feed through the real router, on
//! the synthetic AISStream replay used by fixture mode.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::config::CoreConfig;
use lavender_core::providers::fixture::FixtureProvider;
use lavender_core::{LavenderCore, MemorySecretStore, SecretKey};
use lavender_server::config::ServerSettings;
use tower::ServiceExt;

const RECORDING: &str = include_str!("../../../apps/web/e2e/fixtures/aisstream.jsonl");

struct Fixture {
    app: axum::Router,
    core: LavenderCore,
    _dir: tempfile::TempDir,
}

fn fixture(key: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("aisstream.jsonl"), RECORDING).unwrap();
    let secrets = if key {
        MemorySecretStore::with([(SecretKey::AisStreamApiKey, "fixture-not-a-key")])
    } else {
        MemorySecretStore::default()
    };
    let core = LavenderCore::builder(CoreConfig::default())
        .providers(FixtureProvider::all(dir.path()))
        .secrets(Arc::new(secrets))
        .build()
        .unwrap();
    let settings =
        ServerSettings { static_dir: dir.path().to_owned(), ..ServerSettings::default() };
    Fixture { app: lavender_server::app(core.clone(), &settings), core, _dir: dir }
}

async fn get(app: &axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let request =
        Request::get(path).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn vessel_tracks_validate_the_mmsi_and_use_the_api_envelope() {
    let f = fixture(true);
    let (status, body) = get(&f.app, "/api/v1/feeds/vessels?bbox=24.8,60.0,25.1,60.2").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"]["kind"], "vessels");
    assert_eq!(body["items"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["items"]["items"][0]["mmsi"], "230990001");
    assert_eq!(body["items"]["items"][0]["vesselType"], "Cargo");

    let (status, body) = get(&f.app, "/api/v1/vessels/230990001/track").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["mmsi"], "230990001");
    assert_eq!(body["source"], "aisstream");
    assert_eq!(body["retainedSec"], 1_800);
    let samples = body["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 7);
    assert_eq!(samples[0]["lat"], 60.1);
    assert!(samples[0]["epochSec"].is_i64());
    // Voyage details travel with the selected ship's track, not in every feed item.
    assert_eq!(body["voyage"]["callSign"], "XTEST");
    assert_eq!(body["voyage"]["dimensions"]["lengthM"], 70);
    let (_, feed) = get(&f.app, "/api/v1/feeds/vessels?bbox=24.8,60.0,25.1,60.2").await;
    assert!(feed["items"]["items"][0].get("callSign").is_none());

    for bad in ["12345", "2309900011", "23099000x", "000000000"] {
        let (status, body) = get(&f.app, &format!("/api/v1/vessels/{bad}/track")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(body["code"], "invalidRequest", "{bad}");
    }
    let (status, body) = get(&f.app, "/api/v1/vessels/230990099/track").await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::NOT_FOUND, Some("notFound")));
    f.core.shutdown();
}

#[tokio::test]
async fn without_a_key_vessel_routes_report_an_unconfigured_source() {
    let f = fixture(false);
    let (status, body) = get(&f.app, "/api/v1/vessels/230990001/track").await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("unconfigured"))
    );
    let (status, body) = get(&f.app, "/api/v1/feeds/vessels").await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("unconfigured"))
    );
    let (_, catalog) = get(&f.app, "/api/v1/catalog").await;
    let layer =
        catalog["layers"].as_array().unwrap().iter().find(|l| l["feed"] == "vessels").unwrap();
    assert_eq!(layer["availability"]["state"], "unconfigured");
    let source =
        catalog["sources"].as_array().unwrap().iter().find(|s| s["id"] == "aisstream").unwrap();
    assert_eq!(source["commercial"], "unknown");
    assert_eq!(source["requiresKey"], true);
    f.core.shutdown();
}
