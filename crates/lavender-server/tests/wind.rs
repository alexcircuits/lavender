//! Wind routes over HTTP (ENG-955) against the recorded NOAA GFS fixtures of
//! `lavender-core/tests/fixtures/wind` (provenance in `PROVENANCE.md` there): manifest,
//! status, the little-endian binary grid, and the error envelopes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use lavender_core::LavenderCore;
use lavender_core::config::CoreConfig;
use lavender_server::config::ServerSettings;
use tower::ServiceExt;
use url::Url;
use wiremock::matchers::{header as header_is, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../lavender-core/tests/fixtures/wind");
const GFS_OBJECT: &str = r"^/noaa/gfs\.\d{8}/\d{2}/atmos/gfs\.t\d{2}z\.pgrb2\.0p25\.f\d{3}";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

async fn recorded_gfs() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(r"{GFS_OBJECT}\.idx$")))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fixture("gfs-20261015-00z-f006.idx")),
        )
        .mount(&server)
        .await;
    for (range, file) in [
        ("bytes=430503386-431473669", "gfs-20261015-00z-f006-ugrd-10m.grib2"),
        ("bytes=431473670-432424956", "gfs-20261015-00z-f006-vgrd-10m.grib2"),
    ] {
        Mock::given(method("GET"))
            .and(path_regex(format!("{GFS_OBJECT}$")))
            .and(header_is("range", range))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(fixture(file)))
            .mount(&server)
            .await;
    }
    server
}

struct App {
    router: axum::Router,
    core: LavenderCore,
    _static_dir: tempfile::TempDir,
}

fn app(server: &MockServer, enabled: bool) -> App {
    let static_dir = tempfile::tempdir().unwrap();
    std::fs::write(static_dir.path().join("index.html"), "<!doctype html><title>Lavender</title>")
        .unwrap();
    let mut config = CoreConfig::default();
    config.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    config.fetch.retries = 0;
    config.providers.wind.enabled = enabled;
    config.providers.wind.gfs_base_url = Url::parse(&format!("{}/noaa/", server.uri())).unwrap();
    config.providers.wind.ifs_base_url = Url::parse(&format!("{}/ecmwf/", server.uri())).unwrap();
    let core = LavenderCore::builder(config).build().unwrap();
    let settings =
        ServerSettings { static_dir: static_dir.path().to_owned(), ..ServerSettings::default() };
    App { router: lavender_server::app(core.clone(), &settings), core, _static_dir: static_dir }
}

async fn get(app: &App, path: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let request =
        Request::get(path).header(header::HOST, "localhost:7410").body(Body::empty()).unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, body)
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).unwrap()
}

#[tokio::test]
async fn manifest_status_and_binary_grid_over_http() {
    let server = recorded_gfs().await;
    let app = app(&server, true);

    let (status, headers, body) = get(&app, "/api/v1/wind/manifest?model=gfs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let manifest = json(&body);
    assert_eq!(manifest["unavailable"], false);
    assert_eq!(manifest["grid"]["nx"], 360);
    assert_eq!(manifest["grid"]["ny"], 181);
    let grid_id = manifest["gridId"].as_str().unwrap();
    let grid_url = manifest["gridUrl"].as_str().unwrap();
    assert!(grid_url.starts_with(&format!("/api/v1/wind/grid/{grid_id}.bin?model=gfs")));
    assert!(manifest["attribution"].as_str().unwrap().contains("NOAA Global Forecast System"));

    let (status, headers, grid) = get(&app, grid_url).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=3600, immutable");
    assert_eq!(grid.len(), 360 * 181 * 4 * 2);
    // U at 90° N, 0° E: f32 2.3825536 m/s, least-significant byte first.
    assert_eq!(&grid[..4], &[0xc2, 0x7b, 0x18, 0x40]);

    let (status, _, body) = get(&app, "/api/v1/wind/status?model=gfs").await;
    assert_eq!(status, StatusCode::OK);
    let wind_status = json(&body);
    assert!(wind_status.get("gridUrl").is_none() && wind_status.get("gridId").is_none());
    assert_eq!(wind_status["cycle"], manifest["cycle"]);
    app.core.shutdown();
}

#[tokio::test]
async fn invalid_requests_and_unknown_grids_use_the_error_envelope() {
    let server = recorded_gfs().await;
    let app = app(&server, true);
    for (path, key) in [
        ("/api/v1/wind/manifest?model=hrrr", "unknown_model"),
        ("/api/v1/wind/status?overlay=rain", "unknown_overlay"),
        ("/api/v1/wind/grid/x.bin?model=nope", "unknown_model"),
    ] {
        let (status, _, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        let envelope = json(&body);
        assert_eq!(envelope["error"], key);
        assert_eq!(envelope["code"], "invalidRequest");
        assert_eq!(envelope["retryable"], false);
    }
    let (status, _, body) = get(&app, "/api/v1/wind/grid/gfs-19990101-0-f0-1.bin?model=gfs").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json(&body)["error"], "unknown_grid");
    app.core.shutdown();
}

#[tokio::test]
async fn disabled_wind_answers_unconfigured_without_upstream_requests() {
    let server = recorded_gfs().await;
    let app = app(&server, false);
    for path in [
        "/api/v1/wind/manifest?model=gfs",
        "/api/v1/wind/status?model=ifs&overlay=pressure",
        "/api/v1/wind/grid/gfs-20261015-0-f6-1.bin?model=gfs",
    ] {
        let (status, _, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(json(&body)["code"], "unconfigured");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    app.core.shutdown();
}
