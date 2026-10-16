//! Thin Tauri commands: argument → domain type → one `LavenderCore` call → result.

// Tauri command arguments (State, AppHandle, deserialised payloads) are extractors that
// must be taken by value; the lint does not apply to them.
#![allow(clippy::needless_pass_by_value)]

use lavender_core::LavenderCore;
use lavender_domain::{
    AircraftTrack, ApiError, Catalog, FeedQuery, FeedSnapshot, SatellitePassRequest,
    SatellitePasses, SatelliteTrack, SatelliteTrackRequest, SearchQuery, SearchResults,
    ServerFrame, SystemStatus, VesselTrackResponse, WeatherOverlay, WindManifest, WindModel,
    WindStatus,
};
use tauri::ipc::{Channel, Response};
use tauri::{State, WebviewWindow};

use crate::subscriptions::Subscriptions;

#[tauri::command]
pub fn catalog(core: State<'_, LavenderCore>) -> Catalog {
    core.catalog()
}

#[tauri::command]
pub fn status(core: State<'_, LavenderCore>) -> SystemStatus {
    core.status()
}

#[tauri::command]
pub async fn feed_snapshot(
    core: State<'_, LavenderCore>,
    query: FeedQuery,
) -> Result<FeedSnapshot, ApiError> {
    core.snapshot(query).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn feed_subscribe(
    window: WebviewWindow,
    core: State<'_, LavenderCore>,
    subs: State<'_, Subscriptions>,
    id: String,
    query: FeedQuery,
    channel: Channel<ServerFrame>,
) -> Result<(), ApiError> {
    let sub = core.subscribe(query).map_err(ApiError::from)?;
    subs.open(window.label(), id, sub, channel)
}

#[tauri::command]
pub async fn feed_update(
    window: WebviewWindow,
    subs: State<'_, Subscriptions>,
    id: String,
    query: FeedQuery,
) -> Result<(), ApiError> {
    subs.update(window.label(), &id, query)
}

#[tauri::command]
pub async fn feed_unsubscribe(
    window: WebviewWindow,
    subs: State<'_, Subscriptions>,
    id: String,
) -> Result<(), ApiError> {
    subs.close(window.label(), &id);
    Ok(())
}

#[tauri::command]
pub async fn search(
    core: State<'_, LavenderCore>,
    query: SearchQuery,
) -> Result<SearchResults, ApiError> {
    core.search(&query).await.map_err(ApiError::from)
}

#[tauri::command]
pub fn aircraft_track(
    core: State<'_, LavenderCore>,
    hex: String,
) -> Result<AircraftTrack, ApiError> {
    core.aircraft_track(&hex).map_err(ApiError::from)
}

#[tauri::command]
pub fn vessel_track(
    core: State<'_, LavenderCore>,
    mmsi: String,
) -> Result<VesselTrackResponse, ApiError> {
    core.vessel_track(&mmsi).map_err(ApiError::from)
}

#[tauri::command]
pub async fn satellite_track(
    core: State<'_, LavenderCore>,
    request: SatelliteTrackRequest,
) -> Result<SatelliteTrack, ApiError> {
    core.satellite_track(request).await.map_err(ApiError::from)
}

#[tauri::command]
pub async fn satellite_passes(
    core: State<'_, LavenderCore>,
    request: SatellitePassRequest,
) -> Result<SatellitePasses, ApiError> {
    core.satellite_passes(request).await.map_err(ApiError::from)
}

/// The wind manifest, as `GET /api/v1/wind/manifest?model=…&overlay=…`.
#[tauri::command]
pub async fn wind_manifest(
    core: State<'_, LavenderCore>,
    model: WindModel,
    overlay: Option<WeatherOverlay>,
) -> Result<WindManifest, ApiError> {
    core.wind_manifest(model, overlay.unwrap_or_default()).await.map_err(ApiError::from)
}

/// The wind status (manifest without grid reference), as `GET /api/v1/wind/status`.
#[tauri::command]
pub async fn wind_status(
    core: State<'_, LavenderCore>,
    model: WindModel,
    overlay: Option<WeatherOverlay>,
) -> Result<WindStatus, ApiError> {
    core.wind_status(model, overlay.unwrap_or_default()).await.map_err(ApiError::from)
}

/// One binary wind grid (little-endian f32, `WindGridDef` layout), as
/// `GET /api/v1/wind/grid/{id}.bin`. Raw bytes reach the WebView as an `ArrayBuffer`.
#[tauri::command]
pub async fn wind_grid(
    core: State<'_, LavenderCore>,
    model: WindModel,
    overlay: Option<WeatherOverlay>,
    id: String,
) -> Result<Response, ApiError> {
    let bytes =
        core.wind_grid(model, overlay.unwrap_or_default(), &id).await.map_err(ApiError::from)?;
    Ok(Response::new(bytes))
}

/// Writes the frontend's self-measured smoke report when `LAVENDER_SMOKE_OUT` is set
/// (automated desktop verification, like the spike) and exits if `LAVENDER_SMOKE_EXIT=1`.
/// Does nothing in normal use.
#[tauri::command]
pub fn smoke_report(app: tauri::AppHandle, report: serde_json::Value) -> Result<(), ApiError> {
    let Some(dir) = std::env::var_os("LAVENDER_SMOKE_OUT") else { return Ok(()) };
    let dir = std::path::PathBuf::from(dir);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("desktop-report.json"),
            serde_json::to_vec_pretty(&report).unwrap_or_default(),
        )
    };
    write().map_err(|e| ApiError::new(lavender_domain::ErrorCode::Internal, e.to_string()))?;
    if std::env::var("LAVENDER_SMOKE_EXIT").is_ok_and(|v| v == "1") {
        app.exit(0);
    }
    Ok(())
}
