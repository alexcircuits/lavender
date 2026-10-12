# Lavender desktop — Tauri compatibility specification

- Status: v1, 2026-10-11
- Decisions: ADR 0002 (shared core), 0005 (platform/transport), 0006 (storage/secrets), 0008 (egress)
- Evidence: `spikes/2026-10-11-tauri-cesium-macos.md` (measured), `../research/2026-10-technology-research.md` §B–C (cited)
- Target: Tauri **2.x stable**. Tauri 3 (alpha, CEF runtime option) is tracked, not adopted.

## 1. Goal and non-goals

Lavender ships first as a web application, then as a desktop application for
Windows, macOS and Linux that:

- reuses `apps/web` **unchanged** (one frontend, one build with a `desktop` mode);
- runs `lavender-core` **in-process**, so local capabilities need no server;
- can optionally connect to a remote Lavender server for capabilities that
  require shared infrastructure;
- stores secrets in the OS keychain, and settings and data in the OS app
  directories.

Not in scope now: mobile (Tauri iOS/Android), auto-update infrastructure,
code-signing certificates. They are designed for below but not built.

## 2. Deployment modes

| | Web | Desktop (local core) | Desktop (remote-backed) |
|---|---|---|---|
| Frontend | `apps/web/dist` served by `lavender-server` | same `dist`, embedded by Tauri | same |
| Platform adapter | `WebPlatform` (HTTP + WS) | `TauriPlatform` (invoke + Channel) | `WebPlatform` with configured base URL |
| Core | in the server | in the Tauri process | in the remote server |
| Provider keys | operator's server env | user's OS keychain | remote operator |
| Offline | no | yes for bundled imagery and cached data | no |

Mode selection happens once at the composition root: `isTauri()` → desktop;
then the desktop settings decide local vs remote. Components never branch on
mode.

## 3. What runs where

| Capability | Local in desktop? | Notes |
|---|---|---|
| Feed ingestion from public APIs (USGS, adsb.lol, NASA, NOAA, LL2) | **Yes** | Same providers, same fetch policy; requests originate from the user's machine |
| Keyed sources (FIRMS, AISStream, OpenSky, TomTom) | **Yes**, with the user's keys | Keys in the OS keychain |
| Satellite propagation, ground tracks, passes | **Yes** | Pure `lavender-geo` |
| Geocoding | Yes (configured provider) or remote | Public Nominatim's usage policy forbids heavy use; desktop defaults to coordinate parsing plus the configured provider |
| Local ADS-B receivers (dump1090 on the LAN) | **Desktop-only** | The desktop is the natural home: LAN access without exposing a server |
| Shared workspaces, collaboration, alerts with server-side schedules | **Remote** | Inherently multi-user |
| Historical archives | Remote (large); local cache for recent windows | |
| Map tiles | WebView fetches tile hosts directly | Listed in the desktop CSP |

## 4. Rust structure

```
apps/desktop/
  package.json            scripts: dev (cargo tauri dev), build (cargo tauri build)
  src-tauri/
    Cargo.toml            depends on lavender-core, lavender-domain (workspace member, not default)
    tauri.conf.json       build.frontendDist = "../../web/dist", build.devUrl = "http://localhost:5410"
    capabilities/main.json
    src/main.rs           builder, plugins, state
    src/commands.rs       thin adapters: one fn per core method
    src/secrets.rs        KeyringSecretStore implements lavender_core::SecretStore
    src/settings.rs       file-backed SettingsStore in app_config_dir
```

Commands are thin. Each validates arguments into domain types, calls one
`LavenderCore` method, and maps errors to the serialisable `ApiError`:

| Command | Core call |
|---|---|
| `catalog()` | `core.catalog()` |
| `status()` | `core.status()` |
| `feed_snapshot(query)` | `core.snapshot(query)` |
| `feed_subscribe(query, channel: Channel<FeedUpdate>) -> SubscriptionId` | `core.subscribe(query)` → forward to channel |
| `feed_update(id, query)`, `feed_unsubscribe(id)` | subscription registry (per webview label) |
| `search(query)` | `core.search(query)` |
| `satellite_track(req)` | `core.satellite_track(req)` |
| `secret_set(key: SecretKey, value)`, `secret_clear(key)`, `secret_status()` | keychain; values are **never returned** to the WebView |

`LavenderCore` is created in `setup` with `app_config_dir`, `app_data_dir` and
`app_cache_dir`, and managed as Tauri state. Subscriptions are held in a map
keyed by (webview label, id) and dropped on window close, which cancels their
refresh tasks through the core.

## 5. Security model

### 5.1 Capabilities (least privilege)

- One capability file for the `main` window only. Permissions: `core:default`
  minus anything unused, plus Lavender's own generated command permissions
  (`allow-catalog`, `allow-feed-subscribe`, …). Commands are registered through
  an app manifest in `build.rs` so each needs an explicit grant.
- **No** `shell`, `fs`, `http` or `sql` plugins exposed to the WebView. The
  WebView never issues SQL, reads files or makes arbitrary HTTP requests; the
  core does that through typed commands.
- No remote URL is loaded into a window with IPC access. External links open
  in the system browser (`opener` plugin, scoped to `https:`).

### 5.2 Content Security Policy (desktop)

Verified in the spike, extended with catalog tile hosts at build time:

```text
default-src 'self';
script-src 'self' 'wasm-unsafe-eval';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: <tile hosts>;
connect-src 'self' ipc: http://ipc.localhost <tile hosts>;
worker-src 'self' blob:;
object-src 'none'; base-uri 'self'; frame-ancestors 'none'
```

- **No `'unsafe-eval'`.** Cesium needs only `'wasm-unsafe-eval'` when Widgets
  are not used (measured).
- **`style-src 'unsafe-inline'`** is needed for Cesium's `CreditDisplay`. Under
  CSP 3, a Tauri-injected hash would void it, so `index.html` must not contain
  inline styles (a build test enforces this). The hardening task moves to
  hashes.

### 5.3 Secrets

The `keyring` crate stores keys in the macOS Keychain, Windows Credential Manager
or Secret Service. If Secret Service is unavailable on Linux, keys are
session-only and the UI says so. They are never written to plain files. The
store plugin is not used for secrets (it is not encrypted).

### 5.4 Network

All data egress goes through `lavender-core`'s `Fetcher` (allowlists, https,
caps). The desktop app adds no general HTTP capability to the WebView.

## 6. WebView engines and Cesium

| Platform | Engine | WebGL2 / Cesium risk | Plan |
|---|---|---|---|
| macOS | WKWebView (WebKit tied to the OS) | **Verified** (72 fps, 20k points, workers, WASM; spike) | macOS 13+; WebKit fixes arrive with OS updates only |
| Windows | WebView2 (Chromium, evergreen) | Low | Origin is `http://tauri.localhost`; verify in CI on `windows-latest` |
| Linux | WebKitGTK 4.1 (distro-versioned) | **High**: DMA-BUF blank windows on NVIDIA, silent software rendering, WebGL context loss (tauri#6559), JSC WASM crash on non-AVX CPUs | Version- and GPU-aware environment defaults before the WebView starts (GeoLibre pattern: `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1` on WebKitGTK ≥ 2.52, `WEBKIT_DISABLE_DMABUF_RENDERER=1` on older), always overridable by the user; a WebGL context-loss handler that reports and offers reload; a diagnostics screen showing renderer/WebGL caps; an "open in browser" escape hatch pointing to a hosted instance; evaluate the CEF runtime when Tauri 3 is stable |

Cross-cutting rules from the spike:

- Cesium assets are copied from `@cesium/engine` `Build/Workers`,
  `Build/ThirdParty` and `Source/Assets`, and the build fails if any is missing,
  because under `tauri://` a missing asset returns `index.html`, not a 404.
- A Cesium render error must never freeze the globe silently. Lavender installs
  a render-error handler that reports and attempts recovery.
- `requestAnimationFrame` pauses for occluded windows; feeds and timers must not
  depend on the render loop.
- Asset URLs are relative to the app root (`/cesium/…`, `/assets/…`), with no
  absolute `http://localhost` anywhere. The same `dist` works at `https://host/`,
  `tauri://localhost/` and `http://tauri.localhost/`.

## 7. Build and packaging

- `apps/web`: `vite build --mode desktop` produces the same output, with the
  desktop CSP meta disabled (Tauri injects it).
- `apps/desktop`: `cargo tauri build` (which enables `tauri/custom-protocol`;
  a plain `cargo build` produces a dev binary that loads `devUrl`, which the
  spike demonstrated).
- CI matrix (when the remote exists): `macos-latest`, `windows-latest`,
  `ubuntu-24.04` (with `libwebkit2gtk-4.1-dev`). The job builds and runs the
  headless rendering smoke (the spike's self-measuring page, promoted to a test
  route) where a GPU or software GL is available.
- Signing and updater: the Tauri updater requires signed artifacts
  (`tauri signer generate`, `TAURI_SIGNING_PRIVATE_KEY` at build time). It needs
  an owner decision on key custody and an update endpoint, so it is a roadmap
  task, not part of the readiness milestone.
- Single-instance and deep links (`lavender://view?...` share links): plugins
  `single-instance` (registered first) and `deep-link`. Roadmap.

### 7.1 Automated desktop self-check

The frontend runs a self-check on desktop (`apps/web/src/runtime/selfCheck.ts`). Once
the default layer (earthquakes) has delivered live data over IPC and the globe is
attached, it captures a frame from inside the render loop (`GlobeController.captureFrame`,
lit-pixel ratio) and hands a report to the `smoke_report` command. The command writes
`desktop-report.json` only when `LAVENDER_SMOKE_OUT` is set, and exits the app when
`LAVENDER_SMOKE_EXIT=1`; in normal use it does nothing. The report includes the
platform, transport state, item count and freshness, errors, CSP violations, the
measured animation frame rate, the page visibility and the frame as a PNG data URL.

```bash
cargo build -p lavender-desktop --features custom-protocol   # after `pnpm run build` in apps/web
LAVENDER_SMOKE_OUT=/tmp/lavender-smoke LAVENDER_SMOKE_EXIT=1 ./target/debug/lavender-desktop
```

While `LAVENDER_SMOKE_OUT` is set the window is kept on top and on every Space, because
WKWebView stops rendering when its window is covered. It also stops when the screen is
locked: the report then shows `visibility: hidden`, `framesPerSecond: 0` and no frame,
and `ok` is false. Run it in an unlocked session (CI runners are).

Measured 2026-10-12 (macOS, debug build, live USGS):
- 02:15, screen locked: data path passed (desktop transport live, 45 earthquakes, live
  freshness, globe attached, no errors or CSP violations); the frame check could not run.
- 09:22, unlocked, after the dependency upgrade (Tauri 2.12.3, CesiumJS 1.146,
  satellite.js 7.1): `ok: true` — 53 live earthquakes, 36 fps, `visibility: visible`,
  lit-pixel ratio 0.245, no errors, no CSP violations. Report and frame:
  `docs/architecture/evidence/2026-10-12-desktop-self-check.{json,png}`.

## 8. Readiness acceptance criteria (tracked in Flokzy)

1. Frontend has no browser-only architecture: all I/O through `LavenderPlatform`
   (lint-enforced).
2. `lavender-core` has no dependency on Axum or Tauri (`cargo tree` check in CI).
3. `apps/desktop` builds and invokes the shared core for at least one
   end-to-end feature (earthquakes or aircraft, live), verified by an automated
   run like the spike's.
4. The desktop CSP has no `unsafe-eval`; the build test asserts it.
5. Secrets use the keychain; there is no plain-file fallback.
6. The Windows and Linux CI builds pass; the Linux run records WebKitGTK version
   and renderer.
7. This document is kept current with measured results per platform.
