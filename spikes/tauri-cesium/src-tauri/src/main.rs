//! Feasibility spike: proves the IPC shapes Lavender's desktop adapter will use.
//! `spike_core_info` stands in for a call into the shared Rust core,
//! `stream_ticks` for a live-feed subscription over a Channel, and
//! `report_smoke` records the WebView's own measurements on disk.

use base64::Engine as _;
use serde::Serialize;
use std::{env, fs, path::PathBuf};
use tauri::{AppHandle, ipc::Channel};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreInfo {
    core: &'static str,
    distance_km: f64,
}

/// Stand-in for a shared-core computation: great-circle distance Helsinki → Tallinn.
fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6_371.008_8 * h.sqrt().asin()
}

#[tauri::command]
fn log_stage(line: String) {
    eprintln!("[webview] {line}");
}

#[tauri::command]
fn spike_core_info() -> CoreInfo {
    CoreInfo { core: "lavender-spike", distance_km: haversine_km((60.1699, 24.9384), (59.4370, 24.7536)) }
}

#[derive(Clone, Serialize)]
struct Tick {
    seq: u32,
}

#[tauri::command]
async fn stream_ticks(on_tick: Channel<Tick>, count: u32) -> Result<(), String> {
    for seq in 0..count {
        on_tick.send(Tick { seq }).map_err(|e| e.to_string())?;
        if seq % 50 == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }
    Ok(())
}

#[tauri::command]
fn report_smoke(app: AppHandle, report: serde_json::Value, png_data_url: String) -> Result<(), String> {
    let dir = env::var_os("SPIKE_OUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("spike-out"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pretty = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(dir.join("report.json"), pretty).map_err(|e| e.to_string())?;
    if let Some(b64) = png_data_url.strip_prefix("data:image/png;base64,") {
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())?;
        fs::write(dir.join("globe.png"), bytes).map_err(|e| e.to_string())?;
    }
    if env::var("SPIKE_EXIT").is_ok_and(|v| v == "1") {
        app.exit(0);
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .on_page_load(|webview, payload| {
            eprintln!("[native] page load {:?} {}", payload.event(), payload.url());
            let _ = webview;
        })
        .append_invoke_initialization_script(
            "window.addEventListener('DOMContentLoaded', () => { window.__TAURI_INTERNALS__.invoke('log_stage', { line: 'init-script DOMContentLoaded ' + location.href }).catch(e => {}); });",
        )
        .invoke_handler(tauri::generate_handler![log_stage, spike_core_info, stream_ticks, report_smoke])
        .setup(|_app| {
            eprintln!("[native] setup");
            Ok(())
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("tauri build failed: {e}"))
        .run(|_app, event| {
            if !matches!(event, tauri::RunEvent::MainEventsCleared) {
                eprintln!("[native] event {event:?}");
            }
        });
}
