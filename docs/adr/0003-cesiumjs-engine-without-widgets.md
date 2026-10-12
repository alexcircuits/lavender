# ADR 0003 — Keep CesiumJS, use `@cesium/engine` only, own every control

- Status: Accepted
- Date: 2026-10-11
- Evidence: `docs/architecture/spikes/2026-10-11-tauri-cesium-macos.md`, `docs/audit/upstream-layers.md` §3

## Problem

GEV renders with the full `cesium` package and its `Viewer` (Knockout widgets).
Lavender needs a 3D globe with terrain, photorealistic 3D Tiles, time-dynamic
objects, picking and high-volume point rendering. It must run in browsers and
in three desktop WebViews under a strict CSP.

## Options

| Option | Strengths | Weaknesses |
|---|---|---|
| **CesiumJS** (Apache-2.0) | True 3D globe + terrain + 3D Tiles (incl. Google Photorealistic), time-dynamic primitives, mature picking, WebGL2. Already proven by GEV's renderers and maths. | Large bundle (~4.6 MB min, 1.3 MB gzip for the engine). `Viewer` widgets need `unsafe-eval`. Imperative API. |
| deck.gl + MapLibre globe | Excellent large-data layers, smaller footprint, good 2D | Globe/terrain/3D Tiles support is less complete. A rewrite of every renderer for no analytical gain. |
| iTowns / other | Niche | Smaller community; same rewrite cost. |
| Rust/WASM renderer | More Rust | Years of work; explicitly out of scope. |

## Decision

Keep CesiumJS, but depend on **`@cesium/engine` only**, never `@cesium/widgets`
or the `cesium` meta-package:

- Use `CesiumWidget` (canvas, scene, camera, clock, credits), not `Viewer`. Every
  control (layer list, timeline, search, compass, zoom, imagery picker) is a
  Lavender React component. This removes Knockout, so the CSP needs only
  `'wasm-unsafe-eval'` (measured in the spike).
- Copy Cesium's static half at build time from **two** locations
  (`Build/Workers`, `Build/ThirdParty`, `Source/Assets`), and fail the build if
  any is missing. Serve it under `/cesium/` with `CESIUM_BASE_URL` set before the
  engine loads.
- High-volume live objects use primitive collections
  (`PointPrimitiveCollection`, `BillboardCollection`) with keyed incremental
  updates, not the Entity API. Entities are allowed for small, rich, static sets
  (cyclone tracks, a selected object's trail).
- `requestRenderMode` is on. Live feeds request a frame when data changes, and
  continuous rendering is used only while something animates (camera flight,
  satellite propagation of tracked objects).
- A render-error handler replaces Cesium's default behaviour of silently stopping
  the render loop. It surfaces the error and attempts recovery.
- The engine is lazy-loaded behind the application shell, so the UI paints
  before the 1.3 MB gzip engine arrives.

Pin to a Cesium release at least 14 days old (monorepo dependency rule); the
spike validated `@cesium/engine` 26.3.0 (CesiumJS 1.145).

**Amended 2026-10-12:** the owner decided to use the latest versions without a
release-age gate. Lavender now pins `@cesium/engine` 26.4.0 (CesiumJS 1.146) plus
`@cesium/core` 0.1.0, which now holds Cesium's core types (see
`docs/architecture/frontend.md` §1). The engine-without-widgets decision is unchanged.

## Consequences

- No `unsafe-eval` in any deployment target.
- Lavender owns its controls, so it can fully redesign the interface (no Cesium
  widget styling to fight), at the cost of building the timeline and imagery
  picker itself.
- Credits: `CesiumWidget` still renders the provider credits that Cesium knows
  about. Lavender adds its own attribution panel driven by the core's source
  registry, so no mandatory attribution can be lost.
- Cesium remains a large dependency. Its upgrades are deliberate tasks with a
  rendering smoke test, never incidental bumps.
