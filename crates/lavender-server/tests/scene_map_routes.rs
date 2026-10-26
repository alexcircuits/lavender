#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::{CoreConfig, LavenderCore, MemorySecretStore, SecretKey};
use lavender_domain::{EvidencePack, MapCatalogSnapshot};
use lavender_server::config::ServerSettings;
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

fn app_with(secrets: MemorySecretStore) -> axum::Router {
    let core =
        LavenderCore::builder(CoreConfig::default()).secrets(Arc::new(secrets)).build().unwrap();
    lavender_server::app(core, &ServerSettings::default())
}

#[tokio::test]
async fn evidence_packs_are_listed() {
    let app = app_with(MemorySecretStore::default());
    let (status, body) = get_request(&app, "/api/v1/scenes/packs").await;
    assert_eq!(status, StatusCode::OK);
    let packs: Vec<EvidencePack> = serde_json::from_slice(&body).unwrap();
    assert!(packs.iter().any(|p| p.id == "global-spaceport-corridors"));
    assert!(packs.iter().all(|p| !p.shots.is_empty()));
}

#[tokio::test]
async fn map_catalog_reflects_credentials() {
    let app = app_with(MemorySecretStore::default());
    let (status, body) = get_request(&app, "/api/v1/maps/catalog").await;
    assert_eq!(status, StatusCode::OK);
    let bare: MapCatalogSnapshot = serde_json::from_slice(&body).unwrap();
    let unavailable: Vec<&str> =
        bare.stacks.iter().filter(|s| !s.available).map(|s| s.id.as_str()).collect();
    assert_eq!(unavailable, ["photoreal", "bing-aerial", "bing-labels"]);

    let app = app_with(MemorySecretStore::with([
        (SecretKey::GoogleMapsApiKey, "g-key"),
        (SecretKey::CesiumIonToken, "ion-token"),
    ]));
    let (status, body) = get_request(&app, "/api/v1/maps/catalog").await;
    assert_eq!(status, StatusCode::OK);
    let full: MapCatalogSnapshot = serde_json::from_slice(&body).unwrap();
    assert!(full.stacks.iter().all(|s| s.available));
    assert!(!String::from_utf8_lossy(&body).contains("g-key"));
}
