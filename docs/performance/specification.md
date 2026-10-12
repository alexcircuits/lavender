# Lavender — performance specification

- Status: v1, 2026-10-11
- Baseline: GEV's recorded capture (`legacy/docs/PERFORMANCE.md`, Apple M5, Chrome 150, 1440×900 @1×, 22 Aug 2026), and the Tauri spike (`docs/architecture/spikes/2026-10-11-tauri-cesium-macos.md`, Apple M3 Pro, WKWebView)

## 1. Baselines carried over (GEV)

| Workflow | GEV result |
|---|---|
| Startup: app ready / initial settle (median of 3) | 605 ms / 1 856 ms; JS heap 105 MiB |
| Flights cold activation (247 contacts) | 668 ms, 118 MiB |
| CCTV city cold activation | 19 608 ms |
| Datacenters (4 362 points) | 818 ms, 328 MiB heap |
| Submarine cables (2 629) | 615 ms, **412 MiB heap** |
| NASA FIRMS (100 430 detections) | 30 s activation, 32 fps in motion |
| AISStream (12 000 vessels) | 6.4 s activation, **22 fps in motion** |
| Combined static (11 575 objects) | 57.6 fps, **872 MiB heap** |

GEV's own controls for a capture are adopted unchanged (`legacy/docs/PERFORMANCE.md`
"Controls for a future capture"): record the GPU renderer string, 1440×900 at
DPR 1, focused page, medians of three, 5 s motion and 5 s rest, and live object
counts recorded alongside.

## 2. Targets

### Frontend (Chrome, reference hardware: Apple M-series, 1440×900 @1×)

| Metric | Target | Rationale |
|---|---|---|
| Shell first paint | ≤ 400 ms | Shell is light; engine is lazy |
| Globe first full tile load (GIBS imagery) | ≤ 2 500 ms | Spike: 1.8 s offline imagery |
| Initial JS excluding Cesium engine | ≤ 250 KiB gzip | Code-split engine (1.3 MiB gzip) |
| Idle frame cost | 0 frames rendered when nothing changes | `requestRenderMode` (GEV's animated styles forced 60 fps) |
| Motion with 20 000 point contacts | ≥ 55 fps | Spike: 72 fps in WKWebView |
| AIS 12 000 vessels, motion | ≥ 45 fps (GEV: 22) | Server bbox filtering + billboard diffing |
| FIRMS activation (global) | ≤ 3 s to first render (GEV: 30 s) | Server-side aggregation per zoom |
| Feed update apply (10 000 diffs) | ≤ 8 ms main-thread | Keyed in-place updates |
| JS heap, five P2 layers on | ≤ 300 MiB (GEV combined: 872 MiB) | No Entity API for bulk layers; summaries in stores |
| Panel interaction (toggle, select) | ≤ 100 ms to visual response (INP) | React re-renders limited to rows |

### Backend (`lavender-server`, release build, local)

| Metric | Target |
|---|---|
| Cached snapshot `GET /api/v1/feeds/{feed}` p95 | ≤ 5 ms server time (excluding network) |
| Bbox filter of 50 000 points | ≤ 2 ms (R-tree / linear scan bench decides) |
| Normalise a 10 MiB USGS/adsb.lol payload | ≤ 50 ms |
| Upstream calls per (feed, region) per cadence | exactly 1, regardless of subscriber count |
| Memory, idle with P2 feeds active | ≤ 150 MiB RSS |
| WebSocket fan-out, 100 subscribers same key | ≤ 10 ms per update cycle |

### Desktop (WebViews)

Same frontend targets on macOS (WKWebView) and Windows (WebView2). Linux
(WebKitGTK) targets are recorded per GPU/driver; a software-rendered
configuration is flagged in diagnostics instead of tuned for.

## 3. Measurement methods

| What | How |
|---|---|
| Rust hot paths | `criterion` benches in `crates/*/benches` (normalisation, bbox filter, region keys, SGP4 batch) |
| Server latency | integration test harness with `wiremock` upstreams + a load script (`oha`) against release build |
| Frontend startup | Playwright with CDP: `performance.mark` at shell paint, engine ready, first tiles loaded; three runs, median |
| Frame rate | the spike's self-measuring approach promoted to a dev-only route: wall-clock-bounded postRender counting during scripted camera motion |
| Heap | CDP `Performance.getMetrics` (`JSHeapUsedSize`) |
| Bundle | `vite build` report; CI asserts the initial chunk budget |

## 4. Optimisation priorities (in order)

1. Server-side spatial filtering and aggregation (largest win; removes
   whole-world payloads).
2. Primitive collections with keyed diffing instead of the Entity API.
3. `requestRenderMode` and render requests only on change.
4. Lazy engine and layer code; virtualised lists.
5. Satellite propagation in a Worker above ~3 000 propagated objects.
6. Deltas instead of snapshots on the stream (only if measured payloads
   demand it).
7. Binary encoding for high-volume feeds (only if JSON parse time shows up in
   profiles).

Do not optimise without a measurement that shows the problem.
