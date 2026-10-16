//! Lavender desktop (ADR 0002, docs/architecture/tauri.md). The Tauri side only adapts:
//! every command validates its arguments into domain types, calls one `LavenderCore`
//! method and returns domain types or an `ApiError`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod subscriptions;

use std::sync::Arc;

use lavender_core::config::CoreConfig;
use lavender_core::{FileSettingsStore, LavenderCore, MemorySecretStore};
use tauri::Manager;

use crate::subscriptions::Subscriptions;

/// Builds the core with the OS app directories. Secrets are session-only until the
/// keychain store lands (ENG-997).
fn build_core(app: &tauri::App) -> Result<LavenderCore, String> {
    let paths = app.path();
    let config_dir = paths.app_config_dir().map_err(|e| e.to_string())?;
    let cache_dir = paths.app_cache_dir().map_err(|e| e.to_string())?;
    LavenderCore::builder(CoreConfig::default())
        .secrets(Arc::new(MemorySecretStore::default()))
        .settings(Arc::new(FileSettingsStore::new(config_dir.join("settings.json"))))
        .cache_dir(cache_dir)
        .build()
        .map_err(|e| e.to_string())
}

fn main() {
    lavender_core::diagnostics::install_redacting_panic_hook();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let result = tauri::Builder::default()
        .setup(|app| {
            let core = build_core(app)?;
            app.manage(core);
            app.manage(Subscriptions::default());
            if std::env::var_os("LAVENDER_SMOKE_OUT").is_some() {
                // Automated verification only: WKWebView stops rendering while its window is
                // covered or on another Space (spike finding 3), so keep the window in front and on
                // every Space for the self-check.
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_visible_on_all_workspaces(true);
                    let _ = window.set_always_on_top(true);
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<Subscriptions>().close_window(window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog,
            commands::status,
            commands::feed_snapshot,
            commands::feed_subscribe,
            commands::feed_update,
            commands::feed_unsubscribe,
            commands::search,
            commands::aircraft_track,
            commands::vessel_track,
            commands::satellite_track,
            commands::satellite_passes,
            commands::wind_manifest,
            commands::wind_status,
            commands::wind_grid,
            commands::smoke_report,
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|handle, event| {
            if let tauri::RunEvent::Exit = event {
                handle.state::<LavenderCore>().shutdown();
            }
        }),
        Err(e) => {
            eprintln!("Lavender failed to start: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    /// The command names `build.rs` registers in the app manifest.
    fn registered_commands() -> Vec<String> {
        let build = include_str!("../build.rs");
        let start = build.find("const COMMANDS").unwrap();
        let list = &build[start..start + build[start..].find("];").unwrap()];
        list.split('"').skip(1).step_by(2).map(str::to_owned).collect()
    }

    #[test]
    fn the_main_window_is_granted_exactly_the_registered_commands() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let mut granted: Vec<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .filter(|p| p != "core:default")
            .collect();
        let mut expected: Vec<String> = registered_commands()
            .iter()
            .map(|command| format!("allow-{}", command.replace('_', "-")))
            .collect();
        granted.sort();
        expected.sort();
        assert_eq!(granted, expected);
        for wind in ["wind_manifest", "wind_status", "wind_grid"] {
            assert!(registered_commands().iter().any(|c| c == wind), "{wind}");
        }
    }
}
