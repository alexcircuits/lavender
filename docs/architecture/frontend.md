# Lavender frontend specification

- Status: v1, 2026-10-11
- Decisions: ADR 0003 (Cesium engine-only), 0004 (stack), 0005 (platform), 0007 (contracts)
- Design: `docs/design/design-system.md`

## 1. Stack (latest versions at adoption, exact pins)

React 19.3, TypeScript 7.0 (strict + `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, `verbatimModuleSyntax`), Vite 8.3, `@cesium/engine`
26.4 with `@cesium/core` 0.1 (CesiumJS 1.146), Zustand 5, `@base-ui/react` 1.9,
`cmdk` 1.1, `@phosphor-icons/react` 2.1, `@tanstack/react-virtual`, `satellite.js` 7.1,
Geist/Geist Mono via Fontsource, Vitest 5 + Testing Library + happy-dom 21, Playwright
1.64, Biome 2.5. Package manager: pnpm 12, with the lockfile owned by `apps/web`
(monorepo ADR 0001).

Dependency rule (owner decision 2026-10-12): adopt the latest published version of
everything, pinned exactly; there is no release-age gate (`minimumReleaseAge: 0`).
Upgrades are reviewed through the lockfile diff, licences and the full test gate.

Upgrade notes that changed code:
- CesiumJS 1.146 moved its core maths and geometry types (`Color`, `Cartesian2/3`,
  `Rectangle`, … 149 names) to `@cesium/core`; `@cesium/engine` re-exports them only
  as deprecated value shims without class types, removed in 1.150. `src/globe` imports
  them from `@cesium/core` (pinned to the version the engine depends on, so there is
  one copy and one class identity). The Cesium import guards cover both packages.
- satellite.js 7's root entry re-exports an Emscripten/pthreads WASM bulk propagator
  whose worker uses top-level await, which Vite's `iife` worker bundling rejects.
  Lavender uses only the JavaScript SGP4 functions, so `satellite-js-without-wasm.ts`
  (a Vite plugin, also used by Vitest) resolves that one sub-module to an empty
  module; no WASM or worker is shipped. Propagation uses satellite.js 7's community
  decay check.
- pnpm 12 ships an ES-module entry and a native binary; corepack (≤ 0.36) cannot start
  it (it looks for `bin/pnpm.cjs`). Install pnpm 12 standalone
  (https://pnpm.io/installation), not through corepack.

## 2. Source layout and dependency rules

```
apps/web/src/
  main.tsx                 composition root: createPlatform → createRuntime → <App/>
  app/                     App, Shell (mounts the globe), test harness
  runtime/                 createRuntime (stores + FeedController + catalog/status/persistence),
                           RuntimeProvider and narrow-selector hooks, desktop self-check
  platform/                types.ts, web/, tauri/, preferences  ← only place for fetch/WS/@tauri-apps
  contracts/               generated/ (ts-rs) + index.ts re-exports
  globe/                   GlobeController contract (index.ts), GlobeEngine, imagery, camera,
                           interaction, capture, renderers/      ← only place for @cesium/engine
  feeds/                   FeedController: subscriptions ↔ globe ↔ feed summaries
  state/                   stores.ts: workspace, feeds, system, ui (Zustand vanilla)
  features/
    commands/              command registry, global shortcuts, shortcut sheet
    layers/                LayerExplorer, LayerRow, options, announcer
    inspector/             Inspector, per-kind fact sheets, provenance
    search/                SearchBar, CommandPalette, result builders
    sources/               SourcesPanel (attribution, licence, health)
  shell/                   AppShell, NavRail, SidePanel, StatusBar, MapControls, feed chip
  lib/                     pure helpers (coordinates, mgrs, orbit, time, safeUrl)
  design/                  tokens.css, fonts.css, primitives/*
  test/                    fakes (FakePlatform, fakeGlobe, fixtures)
```

Import rules (Biome `noRestrictedImports` plus a boundary test):

| Module | May import |
|---|---|
| `features/*`, `shell/*` | `design`, `state`, `contracts`, `lib`, `runtime` (context hooks: platform interface, stores, the mounted `GlobeController`) |
| `globe/*` | `@cesium/engine`, `contracts`, `lib`, `state` (subscribe only) |
| `platform/*` | `contracts`, `@tauri-apps/api` (tauri adapter only) |
| `design/*` | nothing app-specific |

## 3. State

Current workspace sharing uses `WorkspaceDocument v1` in
`state/workspaceDocument.ts` and `runtime/workspaceShare.ts`. A deliberate copy
action creates `#view=<base64url UTF-8 JSON>`; it does not continuously rewrite the
URL. Limits are 5,500 decoded bytes, 7,500 payload characters and 8,192 URL
characters. Unsupported versions/fields or malformed values produce a toast.
Restoration respects independent user revisions for layers, imagery, selection
and camera, filters against the current catalog and handles hash/back/forward.
Annotations are reserved as an empty array; native `lavender://view?view=…` is a
reserved format rather than an installed OS handler.

Place search is an explicit selectable action, with retry for unchanged text.
Debounced keystroke requests set `places: false`; coordinates and loaded entities
remain local to Lavender. Only `places: true` may contact an operator-configured
Nominatim/Photon service. This follows the
[public Nominatim policy](https://operations.osmfoundation.org/policies/nominatim/),
which forbids autocomplete. No public service is automatically selected.

The inspector requests aircraft history and satellite passes on demand through
`LavenderPlatform`, identically on HTTP and Tauri. Pass observers use the map
viewport's centre ground intersection at sea level, including tilted views. UTC dates,
geometric elevations and clipped-window flags describe the returned calculation;
no optical visibility or camera-follow behavior is implied.

| Store | Holds | Writers |
|---|---|---|
| `workspace` | enabled layers + options, selection `{ kind, id }`, tracked ids, camera bookmarks, imagery choice | UI actions, share-link restore |
| `feeds` | per feed: freshness, count, source, last error, subscription status | `FeedController` only |
| `ui` | panel open/width, palette open, toasts | UI |

Rules: entity *data* (thousands of positions) is never in a store; it lives in
renderers keyed by id. Stores hold summaries and selection. Selectors are
narrow (`useFeedSummary('aircraft')`), so a feed update re-renders one row.
`workspace` is persisted per device through `platform.preferences`
(debounced) and encoded into share links.

## 4. Globe engine

`GlobeEngine` (a class, not a component):

- `mount(host: HTMLElement)` / `destroy()`. React's `GlobeViewport` mounts it
  once.
- Creates `CesiumWidget` with `requestRenderMode: true`,
  `maximumRenderTimeChange: Infinity`, a WebGL2 context, an explicit base layer
  from the imagery catalog, and ellipsoid or configured terrain.
- Render-error handler: logs, reports to `ui` toasts, attempts
  `scene.requestRender()` after reset; never silently frozen (spike finding 1).
- Picking: one `ScreenSpaceEventHandler`; `pick` → `{ layer, id }` from
  primitive `id` payloads → `workspace.select`.
- Camera service: fly-to (respects reduced motion), home, north-up, follow
  (smoothed), cursor position → status bar (rAF-throttled).
- Imagery service: switches base layer from catalog descriptors; attributes
  credits from the registry.
- Renderer registry: `register(feed, renderer)`. A renderer implements
  `apply(snapshot)`, `setVisible`, `setSelected(id)`, `dispose()`.

Renderers (P2): `AircraftRenderer` (BillboardCollection with rotation +
LabelCollection budgeted), `PointHazardRenderer` (earthquakes, fires:
PointPrimitiveCollection with magnitude ramp), `SatelliteRenderer` (elements →
per-frame SGP4 for visible subset; PointPrimitiveCollection; orbit polyline
for selected), `CycloneRenderer` (small entity set: track lines, cone polygons).

Diff strategy: maintain `Map<id, primitive>`; on snapshot, update existing,
add new, remove missing. Never clear and rebuild a collection.

## 5. Feed controller

Bridges workspace ↔ platform ↔ renderers:

1. Watches `workspace.layers`; for each enabled feed opens
   `platform.subscribe({ feed, bbox })`, closes on disable.
2. Camera move end → debounce 300 ms → `subscription.update({ bbox })` for
   regional feeds (global feeds ignore bbox changes below a zoom threshold).
3. On update: `renderer.apply(snapshot)` then `feeds.setSummary(...)`.
4. Errors map to freshness states; the last good snapshot stays rendered.

## 6. Platform adapters

`WebPlatform`:
- Base URL from `import.meta.env.VITE_LAVENDER_API_BASE` (default same origin).
- `snapshot` → `fetch('/api/v1/feeds/…')` with `AbortSignal` and ETag reuse.
- One shared WebSocket; subscription ids; reconnect with jittered backoff
  (0.5 s → 30 s); resubscribe on reconnect; polling fallback.
- `preferences` → `localStorage` (namespaced `lavender.v1.*`, try/catch).

`TauriPlatform`:
- `invoke` wrappers per command; `Channel<FeedUpdate>` per subscription.
- `preferences` → a desktop command backed by the app config dir.

Both are tested against the same contract test suite with a fake transport.

## 7. Build

- `pnpm dev`: Vite at `:5410`, proxy `/api` (HTTP + WS) → `:7410`.
- `pnpm build`: `scripts/copy-cesium-assets.mjs` (fails on a missing dir), then
  `vite build`. Output `dist/` is served by `lavender-server` and embedded by
  Tauri.
- Code splitting: the engine (`@cesium/engine`) is a separate lazy chunk; the
  shell renders first. Budget: initial JS (excluding the engine) ≤ 250 KiB gzip.
- No inline scripts or styles in `index.html` (CSP; test-enforced).

## 8. Testing

| Level | Tool | Covers |
|---|---|---|
| Unit | Vitest | coordinate parsing (decimal/DMS/MGRS), formatters, stores, feed controller with fake platform, diff logic of renderers (Cesium mocked at the collection boundary) |
| Component | Vitest + Testing Library (happy-dom) | LayerRow states, Inspector, SearchField/palette keyboard behaviour, StatusBar |
| Contract | Vitest | WebPlatform and TauriPlatform against the shared adapter contract suite |
| E2E | Playwright (Chromium; WebKit where available) | boot, toggle earthquakes with the Rust server on fixtures, select, inspect, search coordinates |
| Rendering smoke | Playwright | globe canvas lit-pixel ratio > threshold, no console errors, no CSP violations |

## 9. Porting from GEV

Port almost verbatim (as TS with tests): coordinate parsing and formatting,
place framing, orbit maths not covered by Rust, label arbitration, share codec
concepts, route maths. Do not port: the lifecycle manager, `window` event bus,
the shell facade, style/effect modules, the HUD (see the parity matrix).
