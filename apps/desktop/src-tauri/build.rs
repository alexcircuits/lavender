//! Registers Lavender's commands in an app manifest so each one needs an explicit
//! permission grant in a capability file (least privilege, docs/architecture/tauri.md §5.1).

const COMMANDS: &[&str] = &[
    "catalog",
    "status",
    "feed_snapshot",
    "feed_subscribe",
    "feed_update",
    "feed_unsubscribe",
    "search",
    "satellite_track",
    "smoke_report",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(e) = tauri_build::try_build(attributes) {
        eprintln!("tauri-build failed: {e:#}");
        std::process::exit(1);
    }
}
