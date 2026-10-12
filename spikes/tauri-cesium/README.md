# Spike: CesiumJS inside a Tauri 2 WebView

Throwaway evidence for `docs/architecture/tauri.md`. Not product code; nothing imports it.

It renders `@cesium/engine` (no Widgets/Knockout) with Cesium's bundled offline
NaturalEarthII imagery, 20 000 GPU points and a worker-built polygon under a
strict CSP. Then it calls a Rust command and streams 200 messages over a
`tauri::ipc::Channel`. The page measures itself and reports to Rust, which writes
`report.json` and `globe.png` to `$SPIKE_OUT`.

```bash
pnpm install
pnpm run build                                                   # copies Cesium assets, builds dist/
cd src-tauri && cargo build --release --features tauri/custom-protocol
SPIKE_OUT=/tmp/spike SPIKE_EXIT=1 ./target/release/lavender-spike-tauri
```

`--features tauri/custom-protocol` matters: without it a plain `cargo build` is a
dev build that loads `build.devUrl` instead of the embedded `dist/`
(`cargo tauri build` adds the feature itself).

Results: `docs/architecture/spikes/2026-10-11-tauri-cesium-macos.md`.
