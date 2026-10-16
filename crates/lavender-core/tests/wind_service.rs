//! The wind service end to end on real data (ENG-955): `LavenderCore` → `WindProvider` →
//! a local server that answers with the recorded NOAA GFS and ECMWF IFS inventories and
//! messages at their real byte offsets. Covers the served `.bin` byte order, the source
//! catalogue and attribution, health, the licence policy and the disable switch.
//! Fixture provenance: `tests/fixtures/wind/PROVENANCE.md`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lavender_core::config::CoreConfig;
use lavender_core::{LavenderCore, sources};
use lavender_domain::{
    ApiError, ErrorCode, HealthState, LicenceMode, ProviderHealth, WeatherOverlay, WindManifest,
    WindModel,
};
use serde::Deserialize;
use url::Url;
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/wind");
const GFS_OBJECT: &str = r"^/noaa/gfs\.\d{8}/\d{2}/atmos/gfs\.t\d{2}z\.pgrb2\.0p25\.f\d{3}";
const IFS_OBJECT: &str = r"^/ecmwf/\d{8}/\d{2}z/ifs/0p25/oper/\d{14}-\d+h-oper-fc";

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).unwrap()
}

async fn serve(server: &MockServer, object: &str, suffix: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path_regex(format!("{object}{suffix}$")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

async fn serve_range(server: &MockServer, object: &str, suffix: &str, range: &str, file: &str) {
    Mock::given(method("GET"))
        .and(path_regex(format!("{object}{suffix}$")))
        .and(header("range", range))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(fixture(file)))
        .mount(server)
        .await;
}

/// The recorded upstream for whichever cycle the wall clock selects. Companion scalars
/// (2 m temperature, MSLP) are listed in the inventories but not recorded, so they 404.
async fn recorded_upstream() -> MockServer {
    let server = MockServer::start().await;
    serve(&server, GFS_OBJECT, r"\.idx", fixture("gfs-20261015-00z-f006.idx")).await;
    serve_range(
        &server,
        GFS_OBJECT,
        "",
        "bytes=430503386-431473669",
        "gfs-20261015-00z-f006-ugrd-10m.grib2",
    )
    .await;
    serve_range(
        &server,
        GFS_OBJECT,
        "",
        "bytes=431473670-432424956",
        "gfs-20261015-00z-f006-vgrd-10m.grib2",
    )
    .await;
    serve(&server, IFS_OBJECT, r"\.index", fixture("ifs-20261015-00z-6h.index")).await;
    serve_range(
        &server,
        IFS_OBJECT,
        r"\.grib2",
        "bytes=9967974-10837157",
        "ifs-20261015-00z-6h-10u.grib2",
    )
    .await;
    serve_range(
        &server,
        IFS_OBJECT,
        r"\.grib2",
        "bytes=12250976-13114795",
        "ifs-20261015-00z-6h-10v.grib2",
    )
    .await;
    server
}

fn config(server: &MockServer) -> CoreConfig {
    let mut c = CoreConfig::default();
    c.fetch.allow_http_hosts = vec!["127.0.0.1".to_owned()];
    c.fetch.retries = 0;
    c.providers.wind.gfs_base_url = Url::parse(&format!("{}/noaa/", server.uri())).unwrap();
    c.providers.wind.ifs_base_url = Url::parse(&format!("{}/ecmwf/", server.uri())).unwrap();
    c
}

fn health(core: &LavenderCore, source: &str) -> ProviderHealth {
    core.status().sources.into_iter().find(|s| s.source_id == source).unwrap()
}

#[derive(Debug, Deserialize)]
struct Reference {
    file: String,
    #[serde(rename = "packingPrecision")]
    packing_precision: f64,
    points: Vec<ReferencePoint>,
}

#[derive(Debug, Deserialize)]
struct ReferencePoint {
    row: usize,
    col: usize,
    value: f64,
}

fn reference(file: &str) -> Reference {
    std::fs::read_to_string(format!("{FIXTURES}/reference.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Reference>(line).unwrap())
        .find(|r| r.file == file)
        .unwrap()
}

/// Reads value `(row, column)` of `block` (0 = U, 1 = V) from the served bytes as the
/// documented format says: little-endian f32, row-major, 360 × 181 per block.
fn served_value(bytes: &[u8], block: usize, row: usize, column: usize) -> f32 {
    let at = 4 * (block * 360 * 181 + row * 360 + column);
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Every ecCodes reference point that falls on a whole degree must appear, unchanged, at
/// its documented place in the served grid (1° nodes coincide with 0.25° source points).
/// `first_column` is the source's first longitude in quarter degrees (GFS 0°, IFS 180°).
fn assert_grid_matches(bytes: &[u8], block: usize, file: &str, first_column: usize) {
    let reference = reference(file);
    let mut checked = 0;
    for point in reference.points.iter().filter(|p| p.row % 4 == 0 && p.col % 4 == 0) {
        let row = point.row / 4; // latitude 90° − row
        let column = ((first_column + point.col) % 1440) / 4; // longitude column° E
        let actual = f64::from(served_value(bytes, block, row, column));
        assert!(
            (actual - point.value).abs() <= reference.packing_precision / 2.0,
            "{file} row {row} column {column}: served {actual}, ecCodes {}",
            point.value
        );
        checked += 1;
    }
    assert!(checked >= 20, "{file}: only {checked} points on whole degrees");
}

async fn manifest_and_grid(core: &LavenderCore, model: WindModel) -> (WindManifest, Vec<u8>) {
    let manifest = core.wind_manifest(model, WeatherOverlay::None).await.unwrap();
    assert!(!manifest.stale && !manifest.unavailable, "{manifest:?}");
    let id = manifest.grid_id.clone().unwrap();
    assert!(manifest.grid_url.as_deref().unwrap().contains(&format!("/{id}.bin?")));
    let grid = manifest.grid.as_ref().unwrap();
    assert_eq!(
        (grid.nx, grid.ny, grid.lo1, grid.la1, grid.dx, grid.dy),
        (360, 181, 0.0, 90.0, 1.0, 1.0)
    );
    let bytes = core.wind_grid(model, WeatherOverlay::None, &id).await.unwrap();
    assert_eq!(bytes.len(), 360 * 181 * 4 * 2, "U and V blocks only");
    (manifest, bytes)
}

#[tokio::test]
async fn served_grids_are_little_endian_f32_in_the_documented_order() {
    let server = recorded_upstream().await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();

    let (_, gfs) = manifest_and_grid(&core, WindModel::Gfs).await;
    // U at 90° N, 0° E is the first value: f32 0x40187BC2 (2.3825536 m/s) stored
    // least-significant byte first. Read big-endian, the same bytes would be −62.77.
    assert_eq!(&gfs[..4], &[0xc2, 0x7b, 0x18, 0x40]);
    assert!((f32::from_be_bytes([0xc2, 0x7b, 0x18, 0x40]) + 62.773_68).abs() < 1e-3);
    assert_grid_matches(&gfs, 0, "gfs-20261015-00z-f006-ugrd-10m.grib2", 0);
    assert_grid_matches(&gfs, 1, "gfs-20261015-00z-f006-vgrd-10m.grib2", 0);

    // IFS rows start at 180° E; the served grid still starts at 0° E.
    let (_, ifs) = manifest_and_grid(&core, WindModel::Ifs).await;
    assert_grid_matches(&ifs, 0, "ifs-20261015-00z-6h-10u.grib2", 720);
    assert_grid_matches(&ifs, 1, "ifs-20261015-00z-6h-10v.grib2", 720);
    core.shutdown();
}

#[tokio::test]
async fn catalogue_lists_both_wind_sources_with_their_mandated_attribution() {
    let server = recorded_upstream().await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let catalog = core.catalog();
    let source = |id: &str| catalog.sources.iter().find(|s| s.id == id).unwrap().clone();

    let gfs = source(sources::NOAA_GFS);
    assert!(gfs.attribution.contains("NOAA Global Forecast System (GFS)"));
    assert!(gfs.attribution.contains("no endorsement implied"));
    assert!(gfs.licence.name.contains("Public domain"));
    let ifs = source(sources::ECMWF_IFS);
    for required in [
        "This service is based on data and products of the European Centre for Medium-Range Weather Forecasts (ECMWF).",
        "Source: www.ecmwf.int.",
        "Creative Commons Attribution 4.0 International (CC BY 4.0)",
        "https://creativecommons.org/licenses/by/4.0/",
        "Modified:",
        "ECMWF does not accept any liability whatsoever for any error or omission in the data, their availability, or for any loss or damage arising from their use.",
    ] {
        assert!(ifs.attribution.contains(required), "missing {required:?}");
    }
    assert_eq!(ifs.licence.url, "https://creativecommons.org/licenses/by/4.0/");
    assert!(!gfs.requires_key && !ifs.requires_key);

    // Every manifest, even an unavailable one, carries its source's text.
    let (gfs_manifest, _) = manifest_and_grid(&core, WindModel::Gfs).await;
    assert_eq!(gfs_manifest.attribution.as_deref(), Some(gfs.attribution.as_str()));
    let (ifs_manifest, _) = manifest_and_grid(&core, WindModel::Ifs).await;
    assert_eq!(ifs_manifest.attribution.as_deref(), Some(ifs.attribution.as_str()));
    core.shutdown();
}

#[tokio::test]
async fn health_reports_successes_partial_answers_and_failures_honestly() {
    let server = recorded_upstream().await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    assert_eq!(health(&core, sources::NOAA_GFS).state, HealthState::Idle);

    manifest_and_grid(&core, WindModel::Gfs).await;
    let gfs = health(&core, sources::NOAA_GFS);
    assert_eq!(gfs.state, HealthState::Healthy);
    assert!(gfs.last_success.is_some() && gfs.latency_ms_p50.is_some());

    // The 2 m temperature message is not recorded: the wind is served without it, the
    // manifest says so, and health reports a partial answer rather than a success.
    let partial = core.wind_manifest(WindModel::Gfs, WeatherOverlay::Temperature).await.unwrap();
    assert!(!partial.stale && !partial.unavailable);
    assert_eq!(partial.scalar_error.as_deref(), Some("Temperature field unavailable"));
    assert!(partial.scalar.is_none());
    assert!(partial.grid_id.as_deref().unwrap().ends_with("-temperature-wind-only"));
    let gfs = health(&core, sources::NOAA_GFS);
    assert_eq!(gfs.state, HealthState::Degraded);
    assert_eq!(gfs.last_error.as_deref(), Some("Temperature field unavailable"));
    core.shutdown();

    // An upstream with nothing for IFS: unavailable, and failing in health (never succeeded).
    let empty = MockServer::start().await;
    let core = LavenderCore::builder(config(&empty)).build().unwrap();
    let manifest = core.wind_manifest(WindModel::Ifs, WeatherOverlay::None).await.unwrap();
    assert!(manifest.unavailable && manifest.stale);
    assert_eq!(manifest.reason.as_deref(), Some("Wind upstream unavailable"));
    assert!(manifest.grid_id.is_none() && manifest.grid_url.is_none());
    let ifs = health(&core, sources::ECMWF_IFS);
    assert_eq!(ifs.state, HealthState::Failing);
    assert_eq!(ifs.consecutive_failures, 1);
    assert!(ifs.last_error.as_deref().is_some_and(|e| !e.is_empty()));
    assert_eq!(health(&core, sources::NOAA_GFS).state, HealthState::Idle, "per model");
    core.shutdown();
}

#[tokio::test]
async fn an_inventory_that_points_at_the_wrong_field_is_refused_not_served_as_wind() {
    // The recorded object, but an inventory that names its two 10 m wind messages the
    // wrong way round (an index that drifted from its object, or a reordering mirror): the
    // range the provider fetches as U holds V and the other way round.
    let idx = String::from_utf8(fixture("gfs-20261015-00z-f006.idx")).unwrap();
    let swapped = idx
        .replace(":UGRD:10 m above ground:", ":SWAP:10 m above ground:")
        .replace(":VGRD:10 m above ground:", ":UGRD:10 m above ground:")
        .replace(":SWAP:10 m above ground:", ":VGRD:10 m above ground:");
    assert_ne!(swapped, idx);
    let server = MockServer::start().await;
    serve(&server, GFS_OBJECT, r"\.idx", swapped.into_bytes()).await;
    serve_range(
        &server,
        GFS_OBJECT,
        "",
        "bytes=430503386-431473669",
        "gfs-20261015-00z-f006-ugrd-10m.grib2",
    )
    .await;
    serve_range(
        &server,
        GFS_OBJECT,
        "",
        "bytes=431473670-432424956",
        "gfs-20261015-00z-f006-vgrd-10m.grib2",
    )
    .await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let manifest = core.wind_manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert!(manifest.unavailable && manifest.grid_id.is_none(), "{manifest:?}");
    let gfs = health(&core, sources::NOAA_GFS);
    assert_eq!(gfs.state, HealthState::Failing);
    let error = gfs.last_error.unwrap_or_default();
    assert!(error.contains("10 m eastward wind"), "{error}");
    core.shutdown();
}

#[tokio::test]
async fn status_variant_omits_the_grid_reference() {
    let server = recorded_upstream().await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    let status = core.wind_status(WindModel::Gfs, WeatherOverlay::None).await.unwrap();
    assert!(status.grid_url.is_none() && status.grid_id.is_none());
    assert!(status.cycle.is_some() && status.grid.is_some());
    core.shutdown();
}

#[tokio::test]
async fn disabled_wind_is_reported_disabled_and_never_contacts_upstream() {
    let server = recorded_upstream().await;
    let mut c = config(&server);
    c.providers.wind.enabled = false;
    let core = LavenderCore::builder(c).build().unwrap();
    for source in [sources::NOAA_GFS, sources::ECMWF_IFS] {
        assert_eq!(health(&core, source).state, HealthState::Disabled, "{source}");
    }
    for model in [WindModel::Gfs, WindModel::Ifs] {
        for overlay in [WeatherOverlay::None, WeatherOverlay::Temperature, WeatherOverlay::Pressure]
        {
            let error_code = |e: lavender_core::CoreError| ApiError::from(&e).code;
            let manifest = core.wind_manifest(model, overlay).await.unwrap_err();
            assert_eq!(error_code(manifest), ErrorCode::Unconfigured);
            let status = core.wind_status(model, overlay).await.unwrap_err();
            assert_eq!(error_code(status), ErrorCode::Unconfigured);
            let grid = core.wind_grid(model, overlay, "gfs-20261015-0-f6-1.bin").await.unwrap_err();
            assert_eq!(error_code(grid), ErrorCode::Unconfigured);
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    core.shutdown();
}

#[tokio::test]
async fn commercial_licence_mode_permits_public_domain_and_cc_by_wind() {
    let server = recorded_upstream().await;
    let mut c = config(&server);
    c.licence_mode = LicenceMode::Commercial;
    let core = LavenderCore::builder(c).build().unwrap();
    manifest_and_grid(&core, WindModel::Gfs).await;
    manifest_and_grid(&core, WindModel::Ifs).await;
    core.shutdown();
}

#[tokio::test]
async fn calls_after_shutdown_are_refused_without_network() {
    let server = recorded_upstream().await;
    let core = LavenderCore::builder(config(&server)).build().unwrap();
    core.shutdown();
    let error = core.wind_manifest(WindModel::Gfs, WeatherOverlay::None).await.unwrap_err();
    assert!(matches!(error, lavender_core::CoreError::ShuttingDown));
    assert!(server.received_requests().await.unwrap().is_empty());
}
