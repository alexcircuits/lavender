//! Pass routes exercise the production core with explicitly recorded orbital elements.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::providers::fixture::FixtureProvider;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;

async fn request(app: &axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::get(path).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn satellite_pass_route_returns_bounded_geometry_and_errors() {
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/e2e/fixtures");
    let core = LavenderCore::builder(lavender_core::CoreConfig::default())
        .providers(FixtureProvider::all(&fixtures))
        .build()
        .unwrap();
    let app = lavender_server::app(core.clone(), &ServerSettings::default());
    let (status, body) = request(&app, "/api/v1/satellites/25544/passes?observer=30.2672,-97.7431&hours=24&from=2026-10-11T06:00:00Z").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["noradId"], 25544);
    assert_eq!(body["observer"]["lat"], 30.2672);
    assert_eq!(body["minElevationDeg"], 0.0);
    let passes = body["passes"].as_array().unwrap();
    assert!(!passes.is_empty() && passes.len() <= 64);
    for pass in passes {
        assert!(pass["maxElevationDeg"].as_f64().unwrap().is_finite());
        let rise: jiff::Timestamp = pass["rise"].as_str().unwrap().parse().unwrap();
        let peak: jiff::Timestamp = pass["peak"].as_str().unwrap().parse().unwrap();
        let set: jiff::Timestamp = pass["set"].as_str().unwrap().parse().unwrap();
        assert!(rise <= peak && peak <= set);
    }
    for query in [
        "observer=bad",
        "observer=91,0",
        "observer=0,0&hours=25",
        "observer=0,0&altM=20000",
        "observer=0,0&minElevationDeg=NaN",
        "observer=0,0&from=bad",
    ] {
        let (status, body) =
            request(&app, &format!("/api/v1/satellites/25544/passes?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
        assert_eq!(body["code"], "invalidRequest");
    }
    core.shutdown();
}
