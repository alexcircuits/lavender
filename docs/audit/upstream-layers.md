# Upstream audit: frontend data and layer subsystems (God's Eye View → Lavender)

Audited tree: `legacy/` (God's Eye View, MIT, vanilla JS + CesiumJS, served by a Vite dev server whose Node
middleware in `legacy/server/providers/**` provides every `/api/*` route).
Scope: `legacy/src/{app,data,layers,sources,maps,search,overlays,services,view}` plus `legacy/README.md` and
`legacy/docs/{INFRASTRUCTURE-LAYERS,PERFORMANCE,KNOWN-ISSUES,CURRENT-STATE,APPLICATION,CODE-BOUNDARIES}.md`.
Method: static reading only (no `node_modules`, nothing executed). LOC = `wc -l`. Backend routes were opened only to
identify the upstream service and caching.

---

## 0. Headline numbers

| Area | Source LOC | Test LOC |
| --- | ---: | ---: |
| `legacy/src/layers/**` (25 families) | ~80,000 | ~31,000 |
| `legacy/src/data/**` (excluding datasets) | 35,580 | 63,996 |
| `legacy/src/overlays/**` | 5,559 | 3,313 |
| `legacy/src/sources/**` | 4,722 | 2,120 |
| `legacy/src/app/**` | 3,260 | 1,864 |
| `legacy/src/services/**` | 1,857 | 226 |
| `legacy/src/maps/**` | 1,192 | 1,385 |
| `legacy/src/search/**` | 869 | 860 |
| `legacy/src/view/**` | 385 | 128 |

* **30 registered layers** in the application catalog (`legacy/src/app/constructCatalog.js:130-216`): 29 shareable
  (`legacy/src/data/layerStateTokenReservations.json`) plus `local-adsb` (local-only, never serialised).
* Six more context services behave like data layers but are not registered (regions/boundaries, regional brief,
  detection overlay, analyst engine, data credits, height-datum services). See 2.7.
* Many layer tests live in `legacy/src/data/*.test.mjs`, not next to the layer. A good number of them exercise **server**
  code, and many are source-text regex assertions rather than behaviour tests.

---

## 1. Application bootstrap and architecture

### 1.1 Startup chain

1. `legacy/src/main.js` reads `import.meta.env.GOOGLE_MAPS_API_KEY` and `import.meta.env.CESIUM_ION_TOKEN`
   and calls `createStandaloneApplication()`. `legacy/server/standalone/vite.config.js` → `legacy/build/vite.js` **injects
   the Google Maps key, the Cesium ion token and `MAPILLARY_CLIENT_TOKEN` into the browser bundle** (`build/vite.js:110-112`
   for Mapillary).
2. `legacy/src/standalone/application.js` composes `createApplication()` from `legacy/src/app/application.js` (152 LOC)
   with four phase constructors. A module-level `constructed` flag allows **one application per page**.
3. `legacy/src/app/application.js` is a clean, framework-free lifecycle controller:
   * start order `scene → controls → data → tools`, teardown `tools → controls → data → scene`;
   * each constructor receives `{...earlierComponents, signal, defer}`; `defer(cleanup)` is legal only during construction;
   * one shared `AbortController`; failures run cleanup and throw `AggregateError`;
   * observable frozen `{status, phase}` (`created|starting|ready|destroying|destroyed|failed`).
   This is one of the best-designed modules in the repo and maps 1:1 onto a TS app shell.

### 1.2 Phase constructors

| Phase | File | What it does |
| --- | --- | --- |
| scene | `legacy/src/app/scene.js` (125) + `legacy/src/app/viewer.js` (142) | Creates `Cesium.Viewer` with all widgets off, `baseLayer:false`, `msaaSamples:4`, `preserveDrawingBuffer:true`, `targetFrameRate=60`, plus the Apple-Metal atmosphere workaround (`legacy/src/app/atmosphereCompat.js`). Loads Google Photorealistic 3D Tiles (`legacy/src/maps/google3d.js`) and hides the globe if they load. Constructs `MapStackController` and rebroadcasts stack changes as the `window` event `gev:map-stack-changed`. Writes the Google key to `window.__GOOGLE_MAPS_API_KEY__`. Builds `operations` (request services, surface services, annotation resolver) via `legacy/src/app/operations.js`. |
| catalog (built inside the scene phase) | `legacy/src/standalone/catalog.js` → `legacy/src/app/constructCatalog.js` (189) | Resolves source contracts (`legacy/src/app/sourceComposition.js`), creates the shared military classification registry and weather clock, then constructs all 30 layer instances in a fixed order. |
| controls | `legacy/src/app/controls.js` (51) | Constructs `StyleManager` (`legacy/src/ui/composition.js` → `legacy/src/ui/applicationShell.js`, 1,625 LOC) and **hands it 14 live layer instances directly** via `catalogControlServices()` (`legacy/src/app/catalog.js:1-16`). Flies to Austin unless a share link restores state. |
| data | `legacy/src/app/data.js` (63) | Creates `LayerLifecycle` and `LayerPresentation` (the toggle panel). Registers every layer, calls `attachDataManager()` / `attachMapStackController()`, and seals registration with `finalizeRegistrations(metadata)`. Mounts the panel into `#data-toggles`, installs `window` QA hooks in dev, starts the Cyber Sonar scene. |
| tools | `legacy/src/standalone/tools.js` | Scenes/director, annotations, voice, page listeners (out of scope). |

**Duplicate wiring.** `legacy/src/data/{flights,militaryFlights,traffic,bikeshare,transit}.js` still build module-level
singleton "compatibility" instances when imported. `legacy/src/app/sources.js` imports them, while
`constructCatalog.js` builds the real instances separately. Drop the singleton path in the rewrite.

### 1.3 Layer module contract

There is no TypeScript interface. The contract is implicit in `legacy/src/data/lifecycle.js` and
`legacy/docs/INFRASTRUCTURE-LAYERS.md`. A layer is a plain object with:

* identity: `id`, `name`, `icon`, `source` label;
* lifecycle: `init(viewer)`, `enable(viewer)`, `disable(viewer)`, `update(viewer, {signal})`, `destroy(viewer)`;
* cadence: `refreshInterval` or `updateInterval` in ms (`0` = self-driven or camera-driven, with a stats-only ticker), optional `statsRefreshInterval`;
* params: `getParams()`, `setParams()` (share-link options);
* telemetry: `getStats()`, `getLodDiagnostics()`, `getAnalystRecords()`, `getDetectableObjects()` (for the detection overlay);
* optional: `getRowControls()`, `setRowControlsListener()`, `setLifecyclePresentation()`, `resolveTrackingRestoreTarget()`,
  `attachDataManager()`, `attachMapStackController()`, `getSourceAvailability()`, plus a `testing` surface that **ships in production**
  (`layers/*/testing.js`; for example flights 216, transit 340, vessels 180).

Every family follows the same **component-bag pattern**, for example `legacy/src/layers/firms/index.js`.
`createXLayer({services, feed, ...config})` creates a mutable `layerState` object full of underscore-prefixed fields.
Components such as `model`, `ingestion`, `rendering`, `cards`, `selection`, `viewport`, `lifecycle` and `queries` all close
over that state and over each other. Their `methods` are then merged into the layer object with `Object.assign`.

Two examples of the scale this reaches:
* Flights' `flightState` has about 101 fields and is referenced about 964 times (`legacy/src/layers/flights/state.js`).
* Traffic keeps about 300 lines of mutable state in `legacy/src/layers/traffic/state.js`.

Scene services (picking, sprites, context, render governor, overlay host, ground floor, geoid, trails, …) are injected by
small wiring modules in `legacy/src/app/layers/*.js` (6–49 LOC each).

### 1.4 Lifecycle manager (`legacy/src/data/lifecycle.js`, 2,350 LOC)

`LayerLifecycle` owns registrations, visibility intent, refresh transactions, params and teardown.

* **Registration** is sealed by `finalizeRegistrations()`, which proves every layer has exactly one share disposition
  (`enabled-only`, `enabled+options`, `enabled+mirrored-options`, `local-only`).
* **Per-layer entry state** (`lifecycle.js:121-181`):
  * `enabled` (the settled state) and `lifecycleState` (`disabled|enabling|enabled|disabling`), plus `lifecycleUncertain`;
  * monotonic `visibilityIntentEpoch`, a params epoch and `refreshEpoch`;
  * `destroying`;
  * `toggleChain`, a promise chain that serialises toggles. It was added after double intervals caused OpenSky 429s.
* **Periodic refresh:** one `setInterval` per enabled layer (`_armUpdateLoop`, `lifecycle.js:531-550`). `_runPeriodicUpdate`
  guards against re-entrancy and stale epochs, then publishes `data-updated` activity.
* **`_doToggle`** alone spans `lifecycle.js:646-1463` (~800 lines) of intent and supersession bookkeeping.
* **Events:** `subscribe`, `subscribeVisibilityRequests`, `addVisibilityGuard`, `subscribeBeforeDestroy`, `subscribeActivity`.

`legacy/src/data/manager.js` (43) is a compatibility facade. `legacy/src/app/layerPresentation.js` (94) turns activity into
`governorRequestRender()` and `markDetectionSourcesChanged()` calls, and owns the `LayerPanel`.

### 1.5 Catalog, sources and availability

* `legacy/src/app/sourceComposition.js` declares `CATALOG_SOURCE_CONTRACTS`: for each named source, which layer ids consume
  it and which methods it must have. Examples: `flights: getSnapshot`; `cctv: getCatalog/getHealth/getFrameUrl/getMediaUrl`;
  `traffic: requestRoads/getStatus/fetchFlowForBounds/...`.
  * A missing source becomes a stub that throws `SourceUnavailableError`, and the row shows "data source not configured".
  * This is a good, portable seam.
* `legacy/src/standalone/layerSources.js` picks the concrete adapters. Most are same-origin `/api/*` clients in each
  `legacy/src/layers/<family>/source.js`, plus `legacy/src/sources/openFreeMap.js`, `legacy/src/sources/reference.js`
  (earthquakes, perimeters, cables) and `legacy/src/sources/live/standalone.js` (flights, military, vessels).
* `legacy/src/sources/live/contract.js` (125) is the typed-in-spirit live-feed contract: `finite`, `epoch`, `coordinates`,
  `admitRecords` and `httpError`, with separate coverage, completeness and freshness fields.

### 1.6 State tokens and share links

* **`legacy/src/data/layerState.js` (1,077)** is a versioned (v2) codec.
  * Each layer has a 1–2 character base-36 token (`legacy/src/data/layerStateTokenReservations.json`), e.g. `flights=f`,
    `military=m`, `cctv=c`, `wind=k`, `recent-imagery=1`, `street-level=0`.
  * Options carry a `defaultValue` and a frozen historical `absentValue`, so old links keep their meaning.
  * Grammars are strict (`TRACKING_ID_GRAMMAR`), payloads have ceilings (4,096 / 512 characters), and an unknown token
    rejects the whole payload.
  * Storage key: `gev:layer-state:v2`.
* **`legacy/src/data/layerStateCoordinator.js` (756)** applies share-link and local restores after registration, persists explicit
  changes to `localStorage`, and restores tracked flights, military aircraft or satellites with a 1 s pending poll.
* **`legacy/src/view/index.js` (385)** defines a "view" and serialises it into the share-link format (`viewToParams`, `viewFromParams`, `viewUrl`):
  * camera and layers;
  * style (`normal|crt|nvg|flir|anime|noir|snow`);
  * map (`photoreal|bing-aerial|bing-labels|esri-imagery|osm`);
  * follow target (`aircraft|military_aircraft|satellite`);
  * annotations.

  It is pure and portable.

### 1.7 Render governor (`legacy/src/renderGovernor.js`, 147)

A module-global singleton with two modes, driven by an identity-keyed set of holds:
* Any hold switches Cesium to continuous rendering.
* With zero holds it sets `scene.requestRenderMode = true` and `maximumRenderTimeChange = Infinity`. Every discrete change must then call `governorRequestRender(reason)`.
* Upstream credits it with cutting idle GPU use from about 60%.
* Holds are leaky in places:
  * vessels hold continuous rendering even though nothing animates between polls (`legacy/src/layers/vessels/lifecycle.js:170`);
  * flights and traffic hold it for as long as they are enabled.

Port the concept as an instance-owned service.

### 1.8 Request scheduling and services

* **`legacy/src/services/requests.js` (192)** is a same-origin JSON transport.
  * Cancellation uses `AbortSignal.any([lifetime, caller])`, and redirects are refused (`redirect:'error'`).
  * Endpoints: `/api/overpass`, `/api/terrain/heights`, `/api/regional-brief`, `/api/weather-effects`, `/api/openai/hud-summary`.
  * It probes once for Overpass capability, parses `Retry-After`, and classifies rate-limit responses.
* **Height-datum stack.** Read this before porting any 3D placement.
  * `legacy/src/services/terrainHeights.js` (361): batched, cached ellipsoidal ground heights from `/api/terrain/heights` (Re:Earth), with an EGM96 fallback through `legacy/src/data/geoid.js` (lazy ~2.7 MB grid).
  * `legacy/src/services/groundFloor.js` (666): a coarse floor grid of about 111 m cells.
  * `legacy/src/services/meshFloorSampler.js` (203): `scene.sampleHeight` on the photogrammetric mesh, within an idle-time budget.
  * `legacy/src/services/groundSnap.js` (388): a one-shot model snap per aircraft, with backoff.
* **There is no central scheduler.** Each layer owns its own polling interval (through the manager), `camera.moveEnd`
  listeners, debounce timers, `AbortController`s, tile queues and caches:
  * shared tile queue/cache: `legacy/src/sources/vectorTiles.js`;
  * other helpers: `legacy/src/sources/requestDeadline.js`, `legacy/src/sources/rateLimit.js`, `legacy/src/data/trafficQueue.js`.
* **No Web Workers are used for layer data or rendering.**
  * The only browser worker is the WebUSB SDR Mode-S decoder (`legacy/src/sdr/controller.js:391`, `sdr.worker.js`), which feeds Local ADS-B.
  * `legacy/src/overlays/worldOverlayAllocation.worker.mjs` is not a Web Worker (see section 5).
* **Everything runs on the main thread:** MVT decoding, flow matching, the FIRMS cell binning, the 12.5 Hz aircraft
  dead-reckoning loop and the IR alpha pass.
* **Backend shape.** Every backend is a Vite dev-server plugin with process-local state. The server also **imports
  browser modules**, for example:
  * `src/data/adsbLolFallback.js`, `src/sources/adsbRecords.js`, `src/data/firmsCsv.js`;
  * `src/layers/perimeters/records.js`, `src/sources/radioBrowser.js`, `src/data/naturalEarthRegions.js`;
  * `src/data/transitProxy.js`, `src/sources/transitService.js`, `src/sources/transitHistoryStore.js`;
  * `src/data/terrainHeightsProxy.js`, `src/data/gtfsRealtime.js`, `src/data/aisStreamAdapter.js`, `src/data/aisWatchdog.js`.

  These are natural Rust ports.

### 1.9 How UI state and Cesium scene state are coupled today

* **Layers combine scene objects, input and DOM.**
  * Each layer creates its own `CustomDataSource` or primitive collections.
  * Each installs its **own `ScreenSpaceEventHandler`** (17 sites, across vessels, launches, directions, radio, cyclones,
    localAdsb, cctv, flights, satellites, transit, military, streetLevel, installations, alpr, bikeshare,
    `data/cctvGizmo.js` and `data/localGeojsonCore.js`), so one click runs many picks.
  * Launches and Local ADS-B use `drillPick`, and launches calls it on every `MOUSE_MOVE`.
  * Several families build DOM themselves: awareness, cctv, launches `panel.js` via `innerHTML`, flights, military,
    weather, wind, firms, vessels.
* **Click arbitration is a workaround.** `legacy/src/data/pickRegistry.js` keeps module-global ownership predicates so
  sibling handlers do not clear each other's tracking.
* **Selection is a `window` global.** `legacy/src/data/contextStore.js` stores entities on `window.__gevContextStore` and
  stamps `entity.__gevContextId`.
* **Cross-module messaging uses `window` CustomEvents:** `gev:map-stack-changed` (18 sites), `gev:cockpit-mode-changed`
  (9), `gev:awareness-subject-selected` (9), `gev:entity-selection-cleared` (7), `gev:entity-selected` (6),
  `gev:cctv-request-focus`, `gev:view`, `gev:style-change`. Layers also read the `cockpit-mode` class on `body`.
* **Other globals:**
  * `window` properties: `__GOOGLE_MAPS_API_KEY__`, `__godsEyeView`, `__gevWorldOverlay`, `__gevRecentImagery`, `__gevQaRegisterLayer`;
  * module singletons: render governor, world overlay, pick registry, and `detection.js` (about 38 module-level `let` variables).
* **Layers reach into each other and into UI ownership.**
  * Controls call layer instances directly (`catalogControlServices`).
  * Launches toggles the satellites layer into dense mode through `dataManager`.
  * Awareness reads `dataManager.layers.get(id).module.getStats()` (`legacy/src/layers/awareness/queries.js:9-20`).
* **The manager's activity stream is the only formal UI↔scene channel.** `legacy/src/app/stateChannel.js`, an immutable
  snapshot pub/sub, exists but is used only for product state.

---

## 2. Per-layer inventory

**How to read the tables:**
* **Src:** `API /x` is a same-origin backend route. **DIRECT** means the browser fetches an external host.
* **Rendering abbreviations:** CDS = `CustomDataSource` (Entity API), BBC = `BillboardCollection`, PPC = `PointPrimitiveCollection`,
  PLC = `PolylineCollection`, GPP = `GroundPolylinePrimitive`, WO = the world-overlay canvas
  (`legacy/src/overlays/worldOverlay.js`).
* **Labels:** Cesium labels are forbidden (`legacy/src/noCesiumLabels.test.mjs`). Every label goes through WO.

### 2.1 Aircraft family

| Layer (id, token) | Purpose | Data source | Format | Cadence | Client normalization | Cesium rendering | Interaction | Perf techniques | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Flights `flights` `f` | Worldwide civil fleet: click-to-track, follow camera, trails, 3D models, type/route enrichment | API `/api/flights?lat&lon` (OpenSky `states/all?extended=1`, **global, no bbox**; OAuth/basic/anon; adaptive TTL 9→300 s; adsb.lol `/v2/lat/lon/dist/250` fallback when stale), `/api/flights/track` (OpenSky `tracks/all`), `/api/adsbdb/{type,route}/:id` (24 h disk cache). Client: `legacy/src/sources/live/standalone.js:63-118`. | OpenSky state-vector JSON; provider state in `X-Flight-*` / `X-OpenSky-*` headers | 30 s poll; backoff 20 s, or 45 s on 429/401/403; dropped after 3 missed polls | Live contract (`legacy/src/sources/live/aircraft.js` `normalizeOpenSkyAircraft`); `FlightRecords` sticky fields, per-icao geoid, ground fallbacks (`layers/flights/records.js` 283); `classifyAircraft` (`data/aircraftClass.js` 518); **30 s display delay** with interpolation/arc extrapolation (`layers/flights/motion.js` 1,108, `data/motionModel.js` 502); enrichment rate limiter (`policy.js:222-271`) | One BBC of SVG data-URI icons (`data/aircraftIcons.js`); glTF `Model.fromGltfAsync` in a `PrimitiveCollection` (`rendering.js:482`); unlit IR `CustomShader` (`state.js:442`); tracked aircraft is an Entity with `CallbackProperty` position/rotation + `viewer.trackedEntity` (`tracking.js:916-1037`); trails via `data/trailRenderer.js` + `CallbackProperty` head | Own SSEH `scene.pick` (`tracking.js:1154`), Esc release, cockpit handoff, `gev:awareness-*` events, share-link tracking restore | 12.5 Hz preRender DR loop (`FLEET_DR_INTERVAL_MS=80`), <1 m write skip, camera-pose-gated rotation, horizon occluder, model caps 150/350/60 (cockpit) via `selectModelEligible`, continuous-render hold | `layers/flights/` 7,230 (tracking 1,265, rendering 1,162, motion 1,108, queries 952, state 449, policy 439) | `data/flights.test.mjs` 2,298, `trackedModelRegime.test.mjs` 963, `iconOrientation.test.mjs` 747, `motionModel.test.mjs` 343, `sources/live/contract.test.mjs` 315, in-dir 319 |
| Military `military` `m` | adsb.lol military feed in amber, same features as flights | API `/api/military` (adsb.lol `/v2/mil`, 12 s cache, 429/5xx cooldown), `/api/military/track?hex` (adsb.lol tar1090 `trace_full_<hex>.json`, undocumented path, fragile). Client `standalone.js:122-175`. | readsb JSON `{ac:[…]}` | 15 s poll, 15 s display delay | `normalizeReadsbAircraft` (ft→m, kt→m/s); military records convert **back to feet** (`layers/military/records.js:32-38`), so units drift from flights | Same as flights (amber BBC, `jet.glb`) | Same as flights; known-military aircraft hidden from Flights when Military is on (`flights/snapshotRenderer.js:50-61`) | Same | `layers/military/` 5,417. **A fork of flights**: about 2,800 identical unique lines (rendering 620, tracking 594, queries 396, motion 354, lifecycle 177, state 165, snapshotRenderer 156) | `data/militaryFlights.test.mjs` 830, `tr3bRegistry.test.mjs` 540, in-dir 381 |
| Aircraft classification (`legacy/src/layers/aircraft/`) | Shared military-hex registry for both aircraft layers | Polls `/api/military` every **60 s even when Military is off** (`classification.js:4`) | JSON | 60 s | ICAO hex tagging | — | — | Shared instance | 185 | 141 |
| Local ADS-B `local-adsb` (local-only) | Aircraft heard by the user's own receivers, magenta, real time | API `/api/local-receivers/aircraft` (`LOCAL_RECEIVER_FEEDS`; loopback/RFC1918/`.local` only, DNS pinning, ≤8 feeds, 2 s timeout, 2 MB cap); **also the in-browser WebUSB RTL-SDR** decoded in `legacy/src/sdr/sdr.worker.js` | dump1090/readsb/tar1090/skyaware978 `aircraft.json` | 1 s poll (5 s deadline), 200 ms coalesced scene sync | `legacy/src/sources/adsbRecords.js` (`normalizeDump1090Aircraft`, `mergeLocalAdsbRecords`, `localAdsbFixIsPlausible` jump rejection), 60 s retention, clock rebase; enrichment reuses flights' adsbdb | Entity per aircraft in CDS with `CallbackProperty` position/rotation/show (`index.js:442-466`, `:702`), `DistanceDisplayCondition` 2,000 km, glTF in `PrimitiveCollection` (`models.js:46`), Entity trail | `drillPick` (`index.js:609-628`), card (`card.js`), no camera follow | Never serialised | `layers/localAdsb/` 2,334 (index 880, motion 427, models 282, feeds 253); imports `flights/policy.js` and `flights/modelSpec.js` | `index.test.mjs` 1,167, `data/localReceiversProxy.test.mjs` 459, `feeds.test.mjs` 256, `motion.test.mjs` 209 |

### 2.2 Maritime and space

| Layer | Purpose | Data source | Format | Cadence | Client normalization | Rendering | Interaction | Perf | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Vessels `ais-live-vessels` `a` | Live worldwide AIS ships, chevrons, cards, selected trail | API `/api/vessels?maxRows=&lat&lon&radius_km` and `/api/vessels/track?mmsi=`. **The AISStream websocket is on the server** (`legacy/server/providers/vessels/ais-live.js:17`, `AISSTREAM_API_KEY`). The server's in-memory store (`ais-store.js`, 50k vessels, 64-sample Float32 ring buffers) **ignores lat/lon/radius**, so there is no spatial filtering anywhere. | JSON `{rows, status, silentForMs, reconnectAttempt, …}` | 60 s poll; refetch on `moveEnd` when the view moves ≥25%; 10 s timeout. Server watchdog: 15 s tick, stale after 120 s, socket recycled at 300 s | `normalizeVesselObservation` (kn→m/s, separate heading/course, MMSI dedup) in `legacy/src/sources/live/vessels.js`; diff reconcile with 5 min retention (`layers/vessels/records.js:43-121`); AIS type colours (`data/vesselLabels.js`). No dead reckoning. | One BBC of SVG chevrons (`rendering.js:95-133`), `disableDepthTestDistance:∞`, 3 m above the geoid; trail is an Entity polyline with `depthFailMaterial` | Own SSEH `scene.pick`, `requestWorldFocus` camera move (no follow), Esc | 800 ms `EllipsoidalOccluder` horizon cull, rotation only on camera change, 12k rows / 900 labels, WO 118 px grid + 150 px separation; unnecessary continuous-render hold | `layers/vessels/` 3,000; `data/aisStreamAdapter.js` 580 and `data/aisWatchdog.js` 511 are **server-only** pure state machines | `data/aisLiveVessels.test.mjs` 1,693, `aisWatchdog.test.mjs` 878, `aisStreamAdapter.test.mjs` 757, `ownership.test.mjs` 407, others ~600 |
| Satellites `satellites` `s` | ~840 core sats (stations, visual, GNSS, GEO), optional dense Starlink mode, ISS hero label, orbit rings, click-to-track | API `/api/celestrak/<group>` (CelesTrak `gp.php?GROUP=…&FORMAT=tle`; 6 h memory+disk cache, stale-on-error, single-flight). No key. | 3-line TLE text (`legacy/src/sources/tle.js` `parseTleText`) | **Full re-fetch and rebuild every 5 min** (`controls.js:101`) against a 6 h server cache. Propagation every 1 s (200 ms while tracking), the tracked sat every frame | satellite.js v6 SGP4 (`layers/satellites/orbits.js` 389: `propagatePosition`, `computeOrbitPath`); NORAD dedup; `data/satelliteClass.js`. Dense mode round-robins propagation over ~300 frames in 1,500-sat chunks (`catalog.js:28-55`). `legacy/src/orbit.js` is a camera orbit controller, not orbital math. | PPC with `NearFarScalar`; orbit rings are one `Primitive` + `PolylineGeometry` re-aligned by a GMST `modelMatrix` instead of rebuilt (`orbitFrameModelMatrix`, `rendering.js:38-82`); tracked sat is an Entity with an SGP4 `CallbackProperty` + `viewer.trackedEntity` (`tracking.js:139-200`); ISS label on WO | Pick by NORAD id, track/untrack, share restore. **No pass-prediction UI**: `data/satellitePass.js` (458) is used only headlessly by `legacy/src/tools/queries/space.js` | Round-robin propagation, GMST rotation trick, render hold while animating, analyst cap 2,000 | `layers/satellites/` 3,391 (controls 562, tracking 479) | `satelliteClass` 467, `satellitesTrackedRefresh` 340, `satellitePass` 282, `source.test` 166, analyst 154, others ~400 |
| Space missions `rocket-launches` `x` | Launches from the last 30 days: pad markers, ascent/orbit/recovery/re-entry paths, mission panel, animated replay with chase camera | API `/api/launches` (Launch Library 2 `2.3.0/launches` detailed; optional `LL2_API_TOKEN` server-side; 15 min cache, 12 MB cap). **Also downloads `/api/celestrak/active` — the whole active catalog (10k+ TLEs, several MB) — into the browser** to name-match payloads, and forces the satellites layer into dense mode (`policyHelpers.js:20-28`). | JSON + TLE | 300 s, dirty-flag serialised rebuilds | `normalizeRocketLaunches` (`model.js:168`); paths reconstructed because LL2 rarely has a trajectory (`paths.js` 588) | CDS entities (pad point, reticle billboard, stage polylines, dashed re-entry); `CallbackProperty` for the live payload and replay vehicle; custom GLSL `Material` `GevMissionOrbitTactical` (`orbitRendering.js:20-55`); PLC for TLE-matched orbits; `EllipseGeometry` pad `Primitive`; WO labels | `drillPick(…,12)` on click **and on mouse move** (`lifecycle.js:26-50`); DOM panel via `innerHTML` (`panel.js:455`); replay 0.25–4× (`replay.js`, `camera.js`) | Overlay caps | `layers/launches/` 5,275 (overlays 640, paths 588, rendering 573 with a ~470-line `addLaunchEntity`, replay 557, panel 543) | `data/rocketLaunches.test.mjs` 1,421, in-dir 87 |

### 2.3 Hazards and environment

| Layer | Purpose | Data source | Format | Cadence | Client normalization | Rendering | Interaction | Perf | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Earthquakes `earthquakes` `e` | USGS M2.5+ events from the last 24 h | **DIRECT** `https://earthquake.usgs.gov/.../all_day.geojson` (`legacy/src/layers/earthquakes/source.js:3`). No server route exists. | GeoJSON | 60 s | `normalizeEarthquakeSnapshot` (`records.js`) rejects the whole snapshot on any bad row and drops rows below M2.5 | One Entity per quake in a CDS: `ellipse` `CLAMP_TO_GROUND` of radius `2^mag*1000`, coloured by depth (`model.js:12`, `index.js:96-122`); WO magnitude labels (top 96, collision capacity 48) | **None** (labels are not interactive, no pick handler); `getAnalystRecords` | Horizon cull on labels | 404 (index 233, model 102, records 49, source 20) | `data/earthquakes.test.mjs` 404, `ownership.test.mjs` 90 |
| Weather radar / satellite / lightning (`weather-radar` `v`, `weather-satellite` `o`, `weather-lightning` `l`) | MRMS CONUS reflectivity, GOES-19/18 B14 regional IR + NESDIS global longwave mosaic, NLDN/GLD360 lightning density | API `/api/weather/manifest\|tile\|image`, proxying **NOAA nowCOAST GeoServer WMS** (GetCapabilities → manifest; GetMap EPSG:4326 PNG; 24 h immutable cache) (`legacy/server/providers/weather.js`) | Strict JSON manifest (≤13 ISO times, bounds, maxLevel 6) (`layers/weather/source.js:11-45`) + PNG | Radar and IR 120 s, lightning 600 s (`index.js:259`) | Shared observed-history clock `createWeatherClock` (`clock.js`, pure, injectable timers); IR "filtered" alpha computed on the CPU via canvas `getImageData` per tile (`infraredAlpha.js`, `infraredImage.js`) | **Globe host:** `UrlTemplateImageryProvider` on `/api/weather/tile` (`rendering.js:355`); the global IR mosaic (2048×1024) is sliced on the client by a custom provider (`rasterTiles.js`, LRU 6); double buffering plus next-frame prefetch; `requestImage` is monkey-patched (`rendering.js:367-400`). **3D-tiles host:** a raised `Primitive(RectangleGeometry)` "shell" at 5–6.6 km with a custom GLSL `Material` blending coarse and detail textures (`shellRendering.js:6-53`), texture cache capped at 128 MB | Timeline/history playback; only a "centre outside coverage" check (no value picking) | Imagery ordering (`imageryOrder.js`), detail window fit to the footprint | `layers/weather/` 2,882 (shellRendering 892, index 727, rendering 673, clock 218) | `weather.test` 2,255, `shellRendering.test` 1,504, `data/weatherProxy.test` 1,161, `ui/weatherPanel.test` 656, others ~860 |
| Wind `wind` `k` | GFS / ECMWF IFS 10 m forecast wind with an optional temperature/MSLP field | API `/api/wind/manifest`, `/api/wind/grid/{id}.bin`. The server range-reads GRIB from NOAA S3 `noaa-gfs-bdp-pds` (`.idx`) and `data.ecmwf.int` (`.index`), decodes with **ecCodes WASM**, and resamples to a 1° grid (1 h TTL) | Planar Float32 `u[]`, `v[]`, optional scalar (implicitly host-endian, `source.js:77-98`) | 3,600 s; does not follow the weather clock | `sampleWind`, `bakeWindStreamlines` (golden-angle seeds, RK2, 7,200 paths or 1,200 on narrow screens, `streamlines.js:14-60`) | **Not GPU particles:** CPU-baked streamlines grouped into 30° cells, one `Primitive` of `PolylineGeometry` per cell, with a custom `PolylineMaterialAppearance` animated through a `phaseTime` uniform and a **regex-patched vertex shader** (`gpuRendering.js:13-185`). Speed/scalar field as a `SingleTileImageryProvider` (`fields.js`, `rendering.js:372`). Canvas `requestAnimationFrame` particle fallback. `relief.js` replaces `globe.material` (global side effect) | `inspectWindAtCenter` + DOM marker (`inspection.js`) | Per-cell frustum and occluder cull, height fade | `layers/wind/` 2,367 (rendering 801, index 452, gpuRendering 277) | 2,758 in-dir + `windProxy` 657, `windGfs` 211, `windGrid` 150, `windIfs` 130 |
| Cyclones `weather-cyclones` `y` | NHC/CPHC storms, tracks, forecast cones | API `/api/cyclones` (NHC `CurrentStorms.json` + ArcGIS `NHC_tropical_weather_summary` `f=geojson`; 5 min cache). `nhc.noaa.gov` in `source.js:48` is a **link** only. | Normalised storm JSON, geometry budgets, 4 MB cap | 300 s | `validateCycloneSnapshot` | CDS entities: ground-clamped points, geodesic track polylines and cone polygons with `ClassificationType.BOTH`; preRender horizon cull toggling `entity.show` (`rendering.js:67-84`); WO cards/lead-hour labels (cohort 64) | `scene.pick` → `pickStorm`; card click selects | Horizon cull, label cohort | 1,215 (index 476, rendering 314, source 245, labels 180) | 1,774 + `cyclonesProxy` 454 |
| FIRMS fires `local-firms` `w` | NASA FIRMS active fires (VIIRS NOAA-20/21/SNPP NRT + MODIS NRT, trailing 24 h) | API `/api/firms` (**`FIRMS_MAP_KEY` server-side**, area CSV `world/2`, parsed by `legacy/src/data/firmsCsv.js`, 30 min single-flight cache; 503 `no_key` otherwise) | JSON `{fires:[…]}` for the **whole world** (tens of thousands of records) | 600 s | `adaptFirmsRecords` (`data/firmsAdapt.js`: confidence, time, sensor); **client-side 2°/1° cell binning** for the global/regional LOD bands | Global/regional: one **Entity rectangle `CLAMP_TO_GROUND` per cell** (≤1,800/3,600) (`rendering.js:72-160`). Local/close: BBC glow sprites (≤2,500/3,000, canvas data URLs). Manual horizon cull (`applyHorizonCull`, `model.js:288-302`) over DEM-lifted anchors (`anchors.js`). WO labels (max 18) | `LEFT_CLICK` via the pick registry plus WO hit test, `requestWorldFocus`, fire/cell cards | 650 ms throttled LOD check with hysteresis, horizon cull, per-grid cache | `layers/firms/` 2,290 (model 534, rendering 355) + data helpers ~390 | ~1,792 in `src/data` (`firmsHorizonCull` 587, `firmsInteraction` 370, `firmsCards` 214, `firmsCsv` 183, `fireAnchors` 161), in-dir 45 |
| Fire perimeters `fire-perimeters` `2` | NIFC WFIGS current perimeters with verified InciWeb links | API `/api/fire-perimeters` (WFIGS FeatureServer paged past the transfer limit, `maxAllowableOffset=0.001`, normalised server-side by the shared `layers/perimeters/records.js`, 5 min cache), `/api/fire-perimeters/inciweb/index` and `/publication/:id`. `inciweb.wildfire.gov` is opened with `window.open` only. | JSON `{rows:[{stableId, polygons, facts}]}`; the client accepts up to **80 MB** (`source.js:13`) | 300 s | InciWeb name/state matching (`inciweb.js:31`) | Entity polygons (containment fill α 0.25) + `clampToGround` fire-line polylines (`index.js:310-340`); no LOD or culling | Pick → WO incident card (`cards.js`); picks deliberately left out of the pick registry | Unchanged snapshots keep their geometry | 895 (index 448, inciweb 173, cards 157) | 1,443 + `firePerimetersProxy` 410 |
| Recent imagery `recent-imagery` `1` | Recent HLS S30 (Sentinel-2) / L30 (Landsat 8/9) granules plus daily VIIRS NOAA-21 true colour, before/after swipe | **DIRECT** `https://cmr.earthdata.nasa.gov/search` (CMR UMM with `CMR-Search-After` paging, ≤2,000 records, 30 days, `catalog.js:16-60`); **DIRECT** Worldview Snapshot API `https://wvs.earthdata.nasa.gov/api/v1/snapshot` (its `Data-Present` header proves a day has pixels; `thumbnails.js`); **DIRECT** GIBS WMTS `https://gibs-{a,b,c}.earthdata.nasa.gov/wmts/epsg3857/best/...` (`model.js:539`). Keyless, CORS `*`. Not STAC, not Sentinel Hub. | CMR UMM JSON + PNG/JPEG tiles | Event-driven (user draws or pins a box ≤1,000 km) | `model.js` box math, `groupGranulesByDay`, `rankLatest` by coverage and cloud | ≤2 `UrlTemplateImageryProvider` layers clipped to the box with its own request semaphore (`rendering.js:105-122`); swipe via `layer.splitDirection` + `scene.splitPosition` (`legacy/src/ui/imagerySplit.js`); leases an Esri basemap through `legacy/src/maps/imageryComparison.js` | Readout panel `legacy/src/ui/recentImagery.js` (1,083), box tool | Thumbnail LRU 16, 3 in flight | `layers/recentImagery/` 2,861 — **`index.js` 1,453 is one closure with ~40 inner functions** | 3,099 in-dir + UI 1,307 |

### 2.4 Mobility

| Layer | Purpose | Data source | Format | Cadence | Client normalization | Rendering | Interaction | Perf | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Traffic `traffic` `t` | Simulated vehicle dots on motor roads (never real vehicles) plus congestion heat lines; road modes `tomtom` / `osm` / `hybrid` | **DIRECT** OpenFreeMap `https://tiles.openfreemap.org/planet` TileJSON + MVT (`legacy/src/sources/openFreeMap.js:228`, keyless). API `/api/tomtom/status`, `/api/tomtom/flow/{z}/{x}/{y}.pbf` (TomTom `traffic/map/4/tile/flow/relative`, key server-side, 120 s memory+disk cache, **6,000 tiles/day budget**). Overpass is no longer used (`overpassRoads.js` is dead code; `ingestion.js:172` still reads `x-overpass-cache`). | MVT/PBF decoded **in the browser** (`pbf` + `@mapbox/vector-tile`; `flowDecode.js`, `openFreeMap.js`; `legacy/src/sources/vectorTiles.js` 424 is the shared fetch/cache) | Camera-driven: 320 ms debounce, skip if overlap >0.6 or the centre moved <0.35 km; active below 8 km altitude, major roads only above 4.5 km; flow cache 120 s; retries 1.5→30 s | `isDrivableOpenMapRoad`/`openMapRoad`/`clipTileLine`; flow→road matching (`legacy/src/data/flowMatch.js` 294: ~100 m hash grid, 35 m snap, 30° bearing); hybrid de-duplication (`roadModes.js`); convex camera-footprint tile selection (`footprint.js`); platoons (`data/trafficQueue.js`); road heights from `pickPosition` (`surface.js:146`) | One PPC for all dots, animated in `scene.preRender` from precomputed waypoints (`animation.js:208-295`); two batched GPP heat-line batches (glow pulse via a shared uniform; `CESIUM_3D_TILE` classification) (`rendering.js:126-141`). No entities. | Controls only (`controls.js` 350); no picking | `MAX_DOTS` 6,000 scaled by altitude, fair per-road budget, `HEAT_LINE_CAP` 400, 768 dot changes per pump, road-identity retention (`retention.js`); continuous-render hold while enabled | `layers/traffic/` 5,799 (ingestion 646, flow 559, surface 435, **timing 420 dev instrumentation**, model 381) | 3,331 in-dir + ~1,512 in `src/data` + `vectorTiles` 247, `openFreeMap` 360 |
| Transit `transit` `j` | Live GTFS-RT vehicles, interpolated motion, selected trail, card | API `/api/transit/feeds`, `/vehicles/:feedId`, `/trail/:feedId/:vehicleId`. **The server decodes GTFS-RT** with a hand-written `pbf` decoder (`legacy/src/data/gtfsRealtime.js` 274, used server-side). Seven keyless feeds: MBTA, CapMetro, Metro Transit, HSL, OVapi, Entur, TransLink SEQ (`data/transitFeeds.js`). History is kept for MBTA only, in DataView ring buffers (`sources/transitHistoryStore.js` 493). | JSON snapshots, **whole feed with no bbox** (Entur is ~1.4 MB) | 15 s poll per feed in range; the server fetches only when a browser asks (15 s TTL, ETag, backoff) | ~25 s playback lag with interpolation (`movement.js`, `data/contactPlayback.js` 691), jump rejection, screen-projected heading, proximity-based feed activation | Two BBCs, static and moving (`lifecycle.js:129-134`, `rendering.js:121`); preRender with ≤64 height updates per frame; trail via `data/contactTrailRenderer.js` (GPP with a `Primitive` fallback) | Own SSEH click, Esc, WO card refreshed every 250 ms | 250 ms maintenance timer off the frame path, `MAX_VEHICLES_TOTAL` 15,000, hold only while moving | `layers/transit/` 4,549 (rendering 613, policy 565, ingestion 509, **qaMetrics 403 used only by scripts**, testing 340) | `lifecycle.test` 3,985 + ~1,517 in `src/data` + `transitHistory.test` 179 |
| Bikeshare `bikeshare` `b` | Docking stations coloured by availability, sized by capacity | API `/api/gbfs/<encoded upstream URL>`. **The browser sends the full upstream URL**; the server checks a host allowlist, https and `station_(information\|status).json` paths, and refuses redirects. **The server has no cache.** 32 cities hard-coded in `layers/bikeshare/registry.js` (394). | GBFS 2.x JSON (`legacy/src/sources/gbfsStations.js`; no v3 localised names) | Status 60 s; station info once per city per session; active below 50 km altitude | Station merge | PPC (≤8,000 points) + selection highlight Entity; **synchronous `scene.sampleHeight` per station on load** (`rendering.js:103`) | Own SSEH click, WO card, Esc | City load radius ~100 km | `layers/bikeshare/` 2,038 | `proxyRoute.test` 300, `gbfsProxy.test` 270, `bikeshare.test` 79, `source.test` 44 |
| Directions `directions` `n` | Pick A and B; OSRM drive/walk/bike route with step dots, step card and a camera FLY | API `/api/route?profile&coords&steps=1` → FOSSGIS OSRM `routing.openstreetmap.de/routed-{profile}` (10 min cache, 1 request/s with a queue of 8, distance caps); steps normalised **server-side** by `data/routeSteps.normalizeOsrmSteps` | OSRM JSON / GeoJSON | On demand (15 s client timeout) | `routeSteps.js` (`instructionFor`, `formatRouteDistance`, `formatRouteDuration`), `stepIndexAtDistance`. Note: `data/routePlausible.js` belongs to aircraft and `data/directionText.js` to CCTV. | Route Entity polyline `clampToGround` + `FlowMaterialProperty` (`index.js:769`); A/B Entities **with Cesium labels** (one of two files exempted in `noCesiumLabels.test.mjs`); step dots PPC (`index.js:1256`) | Own SSEH arming, step selection, swap/clear, FLY with progress `setInterval` (`index.js:1033`) | — | **`layers/directions/index.js` is a 1,427-line monolith** | `data/directions.test` 775, `routeSteps.test` 226, `source.test` 43 |

### 2.5 Sensors, media and street level

| Layer | Purpose | Data source | Format | Cadence | Client normalization | Rendering | Interaction | Perf | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| CCTV `cctv` `c` | Public traffic/weather cameras as 3D frustums with the live frame or video projected on the far plane, ambient thumbnails, pose calibration | **Fully proxied; no camera host is called from the browser.** API `/api/cctv/sources` (catalog, read once), `/api/cctv/health`, `/api/cctv/frame/:id?...` (still, falling back to Google Street View Static with a **server** key, then a synthetic SVG), `/api/cctv/media/:id` (HLS playlist rewritten to proxied segments with leases; **2 upstream sessions, 8 leases each**). 13+ agencies (Austin, TxDOT, Caltrans, Ontario 511, DriveBC, Calgary, DelDOT, TfL, Fintraffic, Vegvesen, Tallinn/Tarktee, Warendorf, NSW) in `legacy/server/providers/cctv/sources.js` (1,865). | JSON catalog (up to ~4,000 records at once, no bbox), JPEG, HLS/mp4/webm/mjpeg | Health 7 s, active frame 10 s, idle 60 s, projection swap ≤1 Hz, ambient stills 3–10 min | `buildCatalogFromSources` (`catalog.js:104-210`); `normalizeFeedType` exists twice (`model.js:68-95`, `sources/cctvTypes.js`) | BBC icons + `EllipsoidalOccluder` (`rendering.js:138`); frustum wireframes as 5 Entity polylines per camera (`geometry.js:671-728`); viewshed as a hand-built `Primitive` with `PerInstanceColorAppearance` (`data/cctvViewshed.js:68-128`); monitor plane Entity with `ImageMaterialProperty` bound to the `<video>` element or a double-buffered 1920×1080 canvas with a 64×36 change hash (`projection.js`, `frames.js:27-120`); gizmo entities (`data/cctvGizmo.js`); WO thumbnail cards | Hover pick (120 ms), click select, `flyToBoundingSphere`, auto-hop, one `pickFromRay` obstruction probe, calibration in `localStorage`, `gev:cctv-request-focus` | Staggered geometry queue (4 per 120 ms), lazy coverage for ≤14 neighbours within 1.8 km, card budget 20/28/40 by altitude (`data/cctvLod.js`), frame cache 96. Largest cold start: **19.6 s** (`legacy/docs/PERFORMANCE.md`) | `layers/cctv/` 7,101 (geometry 842, model 566, policy 489, lifecycle 479, projection 438) + data ~2,140 | ~6,600 in `src/data` (`cctv.test` 1,802, `cctvMediaRange` 703, `cctvLod` 647; many test the server), in-dir 328 |
| ALPR `alpr-cameras` `p` | OSM-mapped licence-plate-reader cameras (`surveillance:type=ALPR`), US and Canada, with facing wedges | **DIRECT** `https://tiles.dontgetflocked.com/cameras-{us,ca}-hourly.json` TileJSON + MVT (`layers/alpr/source.js:72-80`), keyless, a community-hosted hourly extract. **The user's viewport leaks to a third party.** An Overpass source (`/api/overpass`) exists but is not the default. | MVT (`tileRecords.js`, 40k feature cap) | `moveEnd` with 300 ms debounce; snapped box reused 10 min; tile cache 1 h | `normalizeAlprNode`, precision cache across zooms, detail zoom z9–12 within 16 tiles, 6,000 nearest | CDS billboards `CLAMP_TO_GROUND` with PNG markers (`presentation.js:375-405`); wedges on WO (≤64; native wedges cost ~40 ms/frame) | Pick → tactical card, `flyToBoundingSphere`, throttled `sampleHeight` | 1,500 marker cap | `layers/alpr/` 2,419 (presentation 655, index 492, overlay 444) | `data/alprCameras.test.mjs` 1,495, in-dir 555 |
| Radio `radio` `r` | Geotagged internet radio stations, category filters, tuner sweep, playback | API `/api/radio/stations` (server-side Radio Browser mirror discovery, curated to 750 stations, 45 min cache, stale up to 7 days), `POST /api/radio/click/:uuid`. **Audio plays directly from each broadcaster's stream URL** (`new Audio()`, `playback.js:36,166-169`), restricted to public HTTPS by `publicRadioHttpsUrl` (`legacy/src/sources/radioBrowser.js:14-60`). | JSON + audio streams | Directory once per enable (rejected if >7 days old); horizon tick 250 ms | `normalizeRadioBrowserStation`, categories, country (`data/radioCountry.js`), viewport ranking | CDS points with Cesium `EntityCluster` (pixelRange 42); **reads the private `clustering._clusterPointCollection`** (`rendering.js:84`); WO cluster/selected labels (cohort 64) | Pick with offset retry, fly-to, WebAudio tuning static (`tuningNoise.js`), ducking while voice speaks | Clustering | `layers/radio/` 4,209 | `data/radio.test` 1,801, `radioProxy` 471, `radioMarkup` 388, in-dir 108 |
| Street level `street-level` `0` | Mapillary coverage, image cones, MapillaryJS viewer, FOLLOW camera on Google 3D | API `/api/mapillary/status`, `/api/mapillary/tiles/:layer/:z/:x/:y` (token server-side, layers stripped, memory LRU + 1 GB disk cache 24 h, 600/min per IP). **DIRECT** `https://graph.mapillary.com/images` with `access_token` in the query string (`providers/mapillary/source.js:61-71,117-146`), plus **MapillaryJS loaded on demand** fetching images and meshes from the Mapillary/`*.xx.fbcdn.net` CDN. `MAPILLARY_CLIENT_TOKEN` is baked into the bundle. | MVT (`decode.js`) + Graph JSON + JPEG | `moveEnd` 320 ms debounce, ≤9 coverage / 16 overview tiles, redraw ≤ every 1.5 s | `tileMath.js` (`tilesForBbox`, zoom-for-height), `groundCast.js` (bare-earth casting, `refineHeights`, `densifyLine`, `nextSurfaceMode` at 1,400/1,800 m), `view.js` `visibleBbox` | Draped GPP (`ClassificationType.BOTH`); on Google 3D at street zoom a cast `Primitive` + `PolylineGeometry` at bare-earth height (`coverage.js:150-215`); overview PPC; cones and marker in BBCs with a horizon cull | Pick routing by the `mly:` prefix (`pickRouter.js`), viewer host, camera follow via the shared navigation authority | `meshSampler.js` heap queue: 11 m cells within 900 m, 6 ms idle budget | `layers/streetLevel/` 4,988 (coverage 917, index 552, meshSampler 371) | **5,547 (20 files) — the best-tested layer** |

### 2.6 Military context, infrastructure, reference and events

| Layer | Purpose | Data source | Format | Cadence | Client normalization | Rendering | Interaction | Perf | Main files (LOC) | Tests (LOC) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Military installations `military-installations` `i` | OSM military areas: outline on select, named markers, nearby-site search | Three tiers in `layers/installations/source.js`: (a) API `/api/military-installations?bbox` (hard-coded Overpass QL, 700-element cap, 0.05° snap, 5 min / 1 h / 30 day caches; needs `OVERPASS_UPSTREAMS`); (b) **DIRECT OpenFreeMap MVT** (the default when Overpass isn't configured; `landuse class=military`, z9–12, ≤16 tiles); (c) bundled `local_data/osm_military_names/names.json` (3.7 MB, ODbL/Overture) for views wider than 10° or crossing the dateline. Search via `/api/google/text-search`. | JSON / MVT / compact arrays | `moveEnd` 180 ms debounce; unavailable retry 30→240 s | `normalizeMilitaryInstallations` (`data/militaryInstallationData.js`); `mergeMilitaryFragments`/`clipTileRing`/`militaryOutlineLines` (`legacy/src/sources/militaryTileGeometry.js` 352) | CDS ground-clamped points + `ClassificationType.BOTH` outline polylines (`rendering.js:236-262`), classified footprint polygon, named-marker PPC (`namedMarkers.js:108`) + WO labels | Pick (`selection.js:53`), context store, Contacts NEXT cohort | `MAX_RENDERED` 700, point cap 512, label cap 24, in-place entity diffs, tile LRU 192 / 64 MB | `layers/installations/` 2,016 + helpers ~1,130 | 1,094 in-dir + `militaryInstallations.test` 891, `militaryTileGeometry` 383, `openFreeMap` 360 |
| Military awareness ("Contacts") `military-awareness` `g` | For a selected flight or vessel: nearby flights, military, vessels and installations within 250 km, with NEXT/history navigation | None of its own. Reads sibling layers' in-memory state (`constructCatalog.js:185-190`) and reaches into `dataManager` internals | in-memory | 750 ms parked, 175 ms while the camera moves | `legacy/src/data/militaryAwarenessEngine.js` (171, pure: `findByDoublingRadius`, `summarizeAwarenessCohort`, `getAwarenessNavigationTargets`) | DOM panel via `innerHTML` + `escapeHtml` (`panel.js`), DOM direction arrows, one range-ring Entity | `gev:awareness-subject-*` events | — | `layers/awareness/` 3,046 (subject 504) | `data/militaryAwareness.test` 3,165, others 240 |
| Datacenters `local-datacenters` `d` / Dams `local-dams` `q` | Bundled infrastructure with point stems and cards | Bundled `local_data/datacenters/datacenters.geojsonl` (2.4 MB, 4,351 features, ODbL) and `dams/dams.geojsonl` (713 KB, 704 features). `dams/dams.geojson` is an **unused duplicate**. The analyst/voice tools fetch and parse the same files a second time (`legacy/src/tools/services.js:77`). | GeoJSONL | Static | `parseGeojsonLines`, `featurePoint` | `GeoJsonDataSource.load` (`data/localGeojsonCore.js:610`), entities mutated into outline + stem polyline + point (`disableDepthTestDistance:∞`), `scene.sampleHeight` within 75 km; WO cards (`data/infrastructureOverlayEntry.js`) | Own SSEH (`:314`), `registerEntityContext`, `flyTo` | `legacy/src/data/localGeojsonLod.js` (416, pure): stem budget 80/200/420, incumbency, eviction grace; overlay cohort 160, collision capacity 96 | `data/localGeojsonCore.js` 1,292 (one ~790-line closure, `:301-1092`) | localGeojson 1,350, Lod 379, infrastructure 215, analyst 222 |
| Submarine cables `telegeography-submarine-cables` `u` | TeleGeography cables and landing points | Bundled `cable-geo.json` (711 KB, 712 features) + `landing-point-geo.json` (351 KB, 1,917 points), snapshot 2026-05-24. **Licence CC BY-NC-SA 3.0 (non-commercial).** | GeoJSON | Static | Trivial id fill (`geometry.js`) | Two `GeoJsonDataSource.load` with `clampToGround` (`ingestion.js:50,62`), classification switched per map stack (`surface.js:22`), CDS of ~2,629 stem polylines (`rendering.js:166`), WO labels (160 winners) | Click → `flyTo` | Dirty-sweep stems (moveEnd + 2 s / 250 m probe). **Highest heap: 412 MiB** (`PERFORMANCE.md`) | `layers/submarineCables/` 1,548 | `data/telegeographySubmarineCables.test` 1,756, in-dir 34 |
| Bhote Koshi event `bhote-koshi-2026` `h` | Curated 2026 Nepal flood: before/after imagery, flood path, witness media; driven by a scene recipe (`legacy/src/scenes/recipes.js:9`) | `public/events/bhote-koshi-2026/event.json` (41 KB), `pre.webp` 1.3 MB, `post.webp` 981 KB; flood path hard-coded in `data/bhoteKoshiFloodPath.js` (**GeoPera, CC BY-NC 4.0**). **DIRECT third-party script injection:** YouTube `iframe_api`, `platform.twitter.com/widgets.js`, `connect.facebook.net` SDK; iframes from `youtube-nocookie` and `facebook.com/plugins/video.php`; CORS poster fetches from `i.ytimg.com` (`data/bhoteKoshiEmbeddedMedia.js:27-79,355,371`; `data/bhoteKoshiEvent.js:140`). | JSON + WebP | Static (`no-store`) | `buildEvidenceTimeline`, `sampleCinematicPose`, `buildCinematicKeyframes` (pure) | CDS flood polylines with `CallbackProperty` (`:2490-2513`), `SingleTileImageryProvider` before/after with imagery split (`:2556`), WO witness cards, DOM evidence frames, render holds | Event panel and cinematic scene | — | `data/bhoteKoshiEvent.js` 3,746 (one ~2,800-line factory from `:950`), `bhoteKoshiEmbeddedMedia.js` 1,097 | Event 2,913, Locator 676, Media 396 |
| Bhote Koshi locator `bhote-koshi-locator` `z` | Nepal locator: border, flood path, quakes | **DIRECT** `earthquake.usgs.gov`; Nepal border through the annotation resolver (Overpass) | GeoJSON | On enable | Ring resolution | Entities + WO callouts | — | — | `data/bhoteKoshiLocator.js` 1,218 | 676 |

### 2.7 Non-layer context services (not registered in the catalog)

| Feature | Files | Source | Role |
| --- | --- | --- | --- |
| Natural Earth regions, admin boundaries, neighbourhoods | `legacy/src/data/naturalEarthRegions.js` (355), `adminBoundaries.js` (803; delta-encoded rings, `decodeRing`), `neighborhoodPolygons.js` (195) | Lazy bundled JSON: Natural Earth countries 924 KB, states/provinces 3.5 MB, regions 1.9 MB, marine 618 KB; US census counties 1.5 MB; SF neighbourhoods 217 KB. **About 8.7 MB downloaded by the browser.** | On-demand lookups for the "outline X" annotation resolver, the analyst engine's `pointInRing`, the Alps geocoder, and the server's regional brief (which imports this browser module). Not a toggleable layer. |
| Regional briefing | `legacy/src/data/regionalBrief.js` (8), `regionalModel.js` (154, pure, shared with the server) | API `/api/regional-brief` fans out server-side: offline Natural Earth place, `api.open-meteo.com` weather, `news.google.com/rss` (regex-parsed), `api.gdeltproject.org` fallback; 0.1° cache key | Cockpit briefing DOM (`legacy/src/ui/cockpitBriefing.js`), refreshed every 5 min or 25 km |
| Detection overlay | `legacy/src/data/detection.js` (1,663; ~38 module-level singletons), `detectionDraw.js`, `detectionPolicy.js`, `detectionRenderDemand.js`, `detectionCohort.js` | `getDetectableObjects()` of flights, military, vessels, satellites, traffic, cctv, transit, bikeshare, firms, localAdsb | Screen-space brackets and labels drawn in WO's `detection` lane with `LabelArbiter`; 34–41 FPS at 25–100% density (`PERFORMANCE.md`) |
| Analyst engine | `legacy/src/data/analystEngine.js` (1,106) | `getAnalystRecords()` snapshots | Pure query engine for voice/MCP questions (filter, scope, rank, follow-up) |
| Data credits | `legacy/src/data/dataCredits.js` (475) | Static + dynamic Cesium credits (HTML strings) | ~45 provider keys (OpenStreetMap, OpenFreeMap, OpenSky, adsb.lol, adsbdb, AISStream, CelesTrak, LL2, USGS, NASA GIBS, WFIGS, Photon, Open-Meteo, Google News, GDELT, 13 CCTV agencies, GBFS, OSRM, GTFS-RT, Radio Browser, Re:Earth, NOAA, ECMWF, FIRMS, TeleGeography, TomTom, Natural Earth, Census, …) |

---

## 3. Maps, imagery and terrain catalog

The stacks are declared in `legacy/src/maps/catalog.js` (`MAP_STACKS`) and selected by `legacy/src/maps/defaultSources.js`. They are
coordinated by `legacy/src/maps/controller.js` (`MapSourceController`, 430), which provides:
* generation-guarded `setStack`;
* provider caches;
* construction and tile-failure fallback;
* static credits via `legacy/src/maps/credits.js`.

| Stack id | Kind | Provider (file) | Host | Credential |
| --- | --- | --- | --- | --- |
| `photoreal` (default when available) | 3D Tiles | Google Photorealistic 3D Tiles (`legacy/src/maps/google3d.js`), tried in order: (1) `createGooglePhotorealistic3DTileset({key})`; (2) bearer token from `/api/google/tiles-token` (`legacy/src/maps/googleTokens.js`; **no route in `legacy/server/` implements it**, so this path is dormant); (3) Cesium ion asset `2275207`. Cache 1.5 GB + 1 GB overflow. | `tile.googleapis.com`, or ion | Google Maps browser key (`GOOGLE_MAPS_API_KEY`, client-visible) or `CESIUM_ION_TOKEN` (client-visible) |
| `bing-aerial` | Globe imagery | `IonImageryProvider` `IonWorldImageryStyle.AERIAL` (`legacy/src/maps/imagery.js`) | `api.cesium.com`, `dev.virtualearth.net`, `ecn.t0-3.tiles.virtualearth.net` | ion token |
| `bing-labels` | Globe imagery | `IonImageryProvider` `AERIAL_WITH_LABELS` | Same | ion token |
| `esri-imagery` (default without 3D) | Globe imagery | `ArcGisMapServerImageryProvider` World_Imagery. Falls back to OSM on construction failure or after 2 tile failures. | `services.arcgisonline.com` | None |
| `osm` | Globe imagery | `OpenStreetMapImageryProvider` | `tile.openstreetmap.org` | None (the OSMF tile usage policy applies) |

**Terrain** (globe stacks only; `legacy/src/maps/terrain.js`):
* With an ion token: Cesium World Terrain (`IonResource.fromAssetId(1)`, vertex normals).
* Without one: the keyless Re:Earth/Mapterhorn quantized mesh `https://terrain.reearth.land/cesium-mesh/ellipsoid`
  (CC BY 4.0). It uses a shared-cooldown 429/5xx retry policy (`legacy/src/maps/terrainRetry.js`) and falls back to
  `EllipsoidTerrainProvider`.
* With Google 3D the globe is hidden and terrain is unused. `legacy/src/app/scene.js` notes that World Terrain conflicts with the 3D tiles.
* Terrain **heights** (as opposed to the mesh) come from `/api/terrain/heights`, a server proxy of Re:Earth `heights.json`
  (`legacy/src/data/terrainHeightsProxy.js`, Node-side).

**Draping:**
* `legacy/src/maps/imageryHost.js` decides where imagery goes: `viewer.imageryLayers` on the globe, or the tileset's
  `imageryLayers` (Cesium 1.138 tileset draping).
* Weather, wind, recent imagery and Bhote Koshi use it. Weather and wind switch to their "shell" primitives when the host is a tileset.
* `legacy/src/maps/imageryComparison.js` leases the map for before/after comparisons.

**Cesium ion usage:**
* Assets: Google 3D (2275207), World Terrain (1) and Bing imagery.
* `Cesium.Ion.defaultAccessToken` is never set; tokens are passed per resource.

**Tokens:**
* `GOOGLE_MAPS_API_KEY`: browser-exposed. Used for 3D Tiles and Geocoding.
* `GOOGLE_MAPS_SERVER_API_KEY`: server-only. Used for Places and the Street View fallback.
* `CESIUM_ION_TOKEN`: browser-exposed.

---

## 4. Search

Composition: `legacy/src/search/defaults.js` builds two services:
* `createPlaceSearch` (`legacy/src/search/placeSearch.js`): ordered providers, 12 s timeout, 64-entry cache, 5 min
  positive and 30 s negative TTL.
* `createGeospatialServices` (`legacy/src/search/geospatial.js`): the other operations, with capability flags.

Forward geocoding order:

1. Decimal coordinates: `legacy/src/search/coordinateGeocoder.js` with the strict parser `coordinateParser.js` (pure).
2. Bundled city/POI presets: `legacy/src/search/presetGeocoder.js` (data from `legacy/src/locations.js` `CITY_POIS`).
3. The Alps special case: `legacy/src/search/alpsGeocoder.js` (Natural Earth polygon).
4. Google Geocoding: **DIRECT** `https://maps.googleapis.com/maps/api/geocode/json?key=<browser key>`.
5. Photon: **DIRECT** `https://photon.komoot.io/api/` (`legacy/src/keylessGeocoder.js`, 419), with a 6 s timeout and an OSM-tag → Google-type mapping.
6. Nominatim: API `/api/geocode` (server, `legacy/server/providers/regional/place.js`). An optional configured provider lives in
   `legacy/src/search/nominatim.js` / `legacy/src/sources/nominatim.js`.

Other operations (`legacy/src/search/http.js`):
* Reverse geocoding: **DIRECT** to Google Geocoding with the browser key.
* Text search: `/api/google/text-search`.
* Nearby: `/api/google/nearby-places`.
* Route: `/api/route`.

The server keeps its own key for the last three.

**Entity search:** there is no entity search box. Entity lookup happens two ways, both client-side over already-loaded records:
* voice/MCP tools (`legacy/src/tools/queries/aviation.js` `findAircraft`, `aircraftInArea`; `tools/queries/space.js`);
* `legacy/src/data/analystEngine.js` over `getAnalystRecords()` snapshots.

---

## 5. Overlays and "workers"

* **`legacy/src/overlays/worldOverlay.js` (2,855)** is a module-singleton 2D canvas host the size of the viewport, drawn in
  `scene.postRender` (`worldOverlay.js:2696`).
  * Sources publish with `setOverlayEntries(sourceId, entries, {cohortLimit, collisionCapacity, maxVisible})` (`:948`).
  * The host projects entries, culls them (`EllipsoidalOccluder`), solves placement around UI occluder rectangles,
    arbitrates labels, paints by lane (`detection, ambient-label, ambient-track, ambient-card, thumbnail, selected, tracked`)
    and hit-tests.
  * Budgets: default cohort 256, per-source maximum 900, ambient card collision capacity 1,150.
  * It mirrors entries into an accessibility action list.
  * The canvas is allocated lazily on the first frame with paint work.
  * It is the reason Cesium labels are banned.
* **`legacy/src/overlays/worldOverlayDraw.js` (1,153)** holds the painters and pure curves (`distanceFade`, `altitudeFade`,
  `placementVariants`, the text-measure cache).
* **`legacy/src/overlays/worldOverlayTokens.js` (228)** holds the style tokens.
* **`legacy/src/data/labelArbiter.js` (1,271)** arbitrates labels on a 32 px grid with per-layer quotas (elastic or weighted),
  150/300 ms fades, a 2.5 s minimum lifetime and a 1.2 s cooldown.
* **`legacy/src/overlays/worldOverlayAllocation.worker.mjs` (1,323) is not a browser worker.** It is a Node
  `--expose-gc` child process spawned by `worldOverlayAllocation.test.mjs` to assert a bytes-per-frame allocation budget over a
  deterministic workload. Port it as a benchmark if at all.
* **Other overlay-like code:**
  * ALPR wedges and installation labels reuse the host.
  * Awareness direction arrows, the wind inspection marker and launch panels are separate DOM.
  * The only browser worker in `src/` is the SDR decoder.

---

## 6. Direct browser → external requests (full list)

The CSP is effectively open: `legacy/build/vite.js:20-30` allows `connect-src`, `img-src` and `media-src` from any `https:`
host. Upstream's own allowlist for its MCP panel (`legacy/src/tools/globePanel.js:21-42`, `PROVIDER_ORIGINS`) is incomplete.

| Host | Purpose | File(s) | Key |
| --- | --- | --- | --- |
| `tile.googleapis.com` | Google Photorealistic 3D Tiles | `legacy/src/maps/google3d.js` | Google Maps browser key in the bundle (or a bearer token) |
| `maps.googleapis.com` | Forward and reverse geocoding | `legacy/src/search/defaults.js`, `legacy/src/search/http.js` | The same browser key, in the query string |
| `api.cesium.com`, `assets.ion.cesium.com`, `assets.cesium.com` | ion assets (Google 3D via ion, World Terrain, Bing) | `legacy/src/maps/google3d.js`, `terrain.js`, `imagery.js` | `CESIUM_ION_TOKEN` in the bundle |
| `dev.virtualearth.net`, `ecn.t0-3.tiles.virtualearth.net` | Bing imagery (via ion) | Cesium `IonImageryProvider` | ion-issued |
| `services.arcgisonline.com` (`server.arcgisonline.com`) | Esri World Imagery | `legacy/src/maps/imagery.js` | None |
| `tile.openstreetmap.org` | OSM raster | `legacy/src/maps/imagery.js` | None |
| `terrain.reearth.land` | Keyless quantized-mesh terrain | `legacy/src/maps/terrain.js` | None |
| `tiles.openfreemap.org` | Vector tiles for traffic roads and military installations | `legacy/src/sources/openFreeMap.js:228` | None |
| `tiles.dontgetflocked.com` | ALPR camera vector tiles (viewport leak) | `legacy/src/layers/alpr/source.js:72-80` | None |
| `earthquake.usgs.gov` | Earthquake GeoJSON | `legacy/src/layers/earthquakes/source.js:3`, `legacy/src/data/bhoteKoshiLocator.js` | None |
| `cmr.earthdata.nasa.gov` | Recent-imagery granule search | `legacy/src/layers/recentImagery/catalog.js:16` | None |
| `wvs.earthdata.nasa.gov` | Worldview Snapshot thumbnails | `legacy/src/layers/recentImagery/model.js:18`, `thumbnails.js` | None |
| `gibs-{a,b,c}.earthdata.nasa.gov` | GIBS WMTS imagery tiles | `legacy/src/layers/recentImagery/model.js:539` | None |
| `photon.komoot.io` | Keyless geocoder | `legacy/src/keylessGeocoder.js` | None |
| `graph.mapillary.com` + Mapillary CDN (`*.xx.fbcdn.net`) and the MapillaryJS runtime | Graph API, photos, meshes | `legacy/src/layers/streetLevel/providers/mapillary/source.js:61-146`, `viewer.js:38,114` | `MAPILLARY_CLIENT_TOKEN` in the bundle and in the query string |
| Broadcaster stream hosts (hundreds) | Radio audio | `legacy/src/layers/radio/playback.js:36,166-169` | None (exposes the listener's IP) |
| `www.youtube.com` (iframe_api), `www.youtube-nocookie.com`, `i.ytimg.com`, `www.facebook.com`, `connect.facebook.net`, `platform.twitter.com` | Event media; **injected third-party scripts** | `legacy/src/data/bhoteKoshiEmbeddedMedia.js:27-79,355,371`, `legacy/src/data/bhoteKoshiEvent.js:140` | None |
| `fonts.googleapis.com`, `fonts.gstatic.com` | Web fonts | Page | None |

**Not direct fetches:**
* **Navigation targets only:** `www.nhc.noaa.gov` (`cyclones/source.js:48`), `inciweb.wildfire.gov` (`window.open`, `perimeters/inciweb.js:66`), and the links in `dataCredits.js`.
* **CCTV** makes no direct calls; everything is proxied.
* **Bikeshare** is proxied, but the browser supplies the upstream URL to `/api/gbfs/`.
* **Node-side only, though they live under `src/`:** `transitFeeds.js`, `transitProxy.js`, `terrainHeightsProxy.js`,
  `firmsCsv.js`, `spaceProviderRequests.js`, `aisStreamAdapter.js`.

---

## 7. Quality assessment

### 7.1 Worth porting nearly as-is to TypeScript (pure, tested, framework-free)

* **App shell and state:**
  * `legacy/src/app/application.js` (lifecycle);
  * `legacy/src/app/stateChannel.js` (immutable pub/sub);
  * `legacy/src/app/sourceComposition.js` (source contracts and unavailable stubs);
  * `legacy/src/sources/live/contract.js` (live-feed admission).
* **Share state:** `legacy/src/data/layerState.js` codec rules and `legacy/src/view/index.js`. Re-key the tokens for Lavender.
* **Map plumbing:**
  * `legacy/src/maps/controller.js`, `registry.js`, `terrainRetry.js`, `imageryComparison.js`, `imageryHost.js`;
  * the render-governor concept.
* **Search:**
  * `legacy/src/search/placeSearch.js`, `coordinateParser.js`, `presetGeocoder.js`;
  * `normalizeGooglePlace`, `normalizeGoogleReverse`;
  * the Photon type mapping.
* **Aircraft:**
  * `legacy/src/sources/live/aircraft.js` normalisers and `legacy/src/sources/adsbRecords.js`;
  * `classifyAircraft` (`data/aircraftClass.js`);
  * `data/motionModel.js` (`lerpAngleDeg`, `arcOffsetEnu`, `staleCoastLimitSeconds`, …);
  * `selectModelEligible`, `stabilizeScreenRotation`;
  * the `data/aircraftIcons.js` SVGs.
* **Space:**
  * `legacy/src/data/satellitePass.js`: bisected rise/set, parabolic peak, sun position, shadow;
  * `parseTleText`, `orbitFrameModelMatrix`, `computeOrbitPath`;
  * `data/satelliteClass.js`;
  * launch `paths.js` math (`approximateOrbitPath`, `blendAscentIntoOrbitTangent`, `buildMissionPaths`), `replayState`, camera pose math.
* **Vessels:**
  * `VesselStore.reconcile`, `normalizeVesselObservation`, `viewArea.js`, the vesselLabels type tables;
  * `data/aisWatchdog.js`, an excellent I/O-free state machine. It belongs in Rust.
* **Weather and hazards:**
  * `createWeatherClock`, `validateWeatherSnapshot`, `detailWindow`/`shellLift`, `createRasterTileProvider`, colour stops;
  * wind: `sampleWind`, `bakeWindStreamlines`, `groupWindPaths`, palettes;
  * cyclones: `validateCycloneSnapshot`, `labels.js`;
  * FIRMS: `parseFirmsCsv`, `normalizeConfidence`, the `LOD_LEVELS` table, `applyHorizonCull`;
  * earthquakes: `normalizeEarthquakeSnapshot`;
  * perimeters: InciWeb matching (`normalizeName`, `resolveInciwebNodeLink`).
* **Recent imagery:** `model.js` (box math, `groupGranulesByDay`, `rankLatest`, URL builders) and `parseCmrUmm`.
* **Mobility:**
  * `legacy/src/data/tomtomTiles.js`, `flowMatch.js`, `trafficBounds.js`, `trafficQueue.js`, `trafficFlowStyle.js`;
  * `footprint.convexFootprint`, `isDrivableOpenMapRoad`/`openMapRoad`/`clipTileLine`/`polygonCentroid`;
  * `legacy/src/sources/vectorTiles.js` (generic tile cache);
  * transit `policy.js` helpers and movement plausibility;
  * `gbfsStations` parsers;
  * `routeSteps.js`.
* **CCTV:**
  * `computeFrustumGeometry`/`frustumFrameEcef` (extract from the closure);
  * `data/cctvFootprint.js`, `cctvViewshed` geometry, `cctvGizmo` ray maths;
  * `data/cctvLod.js` selection and eviction grace.
* **ALPR:** `alprDetailZoom`, the precision cache, `normalizeAlprNode`.
* **Radio:** URL safety checks, `normalizeRadioBrowserStation`, category matching.
* **Street level:** `tileMath.js`, `groundCast.js`, the `meshSampler` heap and cell cache, `view.js` `visibleBbox`, `decode.js`,
  and the provider contract in `registry.js` (the cleanest design of the sensor layers).
* **Context:**
  * `legacy/src/data/militaryAwarenessEngine.js`, `localGeojsonLod.js`;
  * admin and Natural Earth lookup logic;
  * `regionalModel.js`;
  * `detectionPolicy`/`detectionDraw`/`detectionCohort`, `labelArbiter.js`, `analystEngine.js`;
  * the `worldOverlayDraw.js` curves.
* **Height datum:** port the rules in `services/groundFloor.js`, `groundSnap.js`, `meshFloorSampler.js` and `data/geoid.js`, not the code structure.

### 7.2 Poorly designed or over-coupled

* **Lifecycle complexity.** `legacy/src/data/lifecycle.js` (2,350 lines; `_doToggle` alone is ~800) has epoch and intent
  machinery that compensates for unstructured async. Replace it with a small typed state machine plus a store.
* **The flights/military fork.** About 2,800 identical unique lines, constants repeated in both `policy.js` files, and units
  that have drifted apart. Local ADS-B imports flights internals.
* **The component-bag pattern.** One shared mutable `layerState` (flights ~101 fields, ~964 references; traffic ~300 lines)
  is impossible to type cleanly and gives no encapsulation.
* **Globals and singletons:**
  * the render governor, world overlay, pick registry, context store and `detection.js`;
  * `window.__GOOGLE_MAPS_API_KEY__`;
  * `window` CustomEvents and the `body.cockpit-mode` class.

  These block multiple viewers and make tests depend on page state.
* **Input handling.** Seventeen per-layer `ScreenSpaceEventHandler`s plus pick-registry predicates, with `drillPick` on mouse
  move in launches. There should be one pick dispatcher.
* **DOM inside layers.** Launches, cctv, awareness, flights, military, wind and weather build DOM. Controls hold layer
  instances directly.
* **Monolithic closures:**
  * `data/bhoteKoshiEvent.js` (~2,800-line factory);
  * `overlays/worldOverlay.js` 2,855;
  * `data/detection.js` 1,663;
  * `layers/recentImagery/index.js` 1,453;
  * `layers/directions/index.js` 1,427;
  * `data/localGeojsonCore.js` (~790-line closure);
  * launches `addLaunchEntity` (~470 lines).
* **Shipped test and QA code.** `layers/*/testing.js` seams are part of the public layer API, `transit/qaMetrics.js` is used
  only by scripts, and there are 420 lines of traffic timing instrumentation.
* **Fragile Cesium internals:**
  * radio reads `clustering._clusterPointCollection`;
  * weather monkey-patches `requestImage`;
  * the wind shader is patched with a regex;
  * wind relief replaces `globe.material` globally.
* **Whole-world payloads with no spatial filtering.** Flights (OpenSky global), vessels (the server ignores bbox), FIRMS
  (global), transit (whole feed), CCTV catalog (~4,000 records), launches (the entire CelesTrak active catalog), regions
  (~8.7 MB of JSON).
* **Wasteful refresh.** Satellites re-fetch and rebuild every 5 min against a 6 h cache. The military registry polls even
  when Military is off.
* **Main-thread hot spots.** MVT decoding and flow matching, FIRMS binning, the IR alpha pass, `sampleHeight` per
  bikeshare station, and the 12.5 Hz aircraft loop.
* **Client/server boundary.** The server imports browser modules, and all backends are dev-server plugins holding state in
  process memory.
* **Secrets and privacy:**
  * the Google, ion and Mapillary tokens are in the bundle;
  * ALPR leaks the viewport to a third party;
  * radio exposes listener IPs to broadcasters;
  * event media injects Facebook, Twitter and YouTube SDKs.
* **Licensing.** TeleGeography is CC BY-NC-SA and the GeoPera flood path is CC BY-NC, both non-commercial. Many sources
  are ODbL and need attribution.
* **Uneven tests.** Launches has 87 test lines in its folder, radio 108, FIRMS 45, cables 34. A lot of coverage is
  regex-on-source assertions, or lives in `src/data` and actually tests server code.

---

## 8. Migration recommendations per layer

**Legend:**
* **Preserve:** port the behaviour and most of the logic.
* **Improve:** port with a redesign.
* **Replace:** new implementation or provider.
* **Remove:** drop it.
* **Defer:** a later phase.

"Rust" means the Lavender backend; "TS" means the new frontend.

| Layer | Rec. | Reason | Rust backend | TS frontend |
| --- | --- | --- | --- | --- |
| Flights | Improve | Core value, but the payload is the whole world every 30 s | OpenSky OAuth/adaptive cache, adsb.lol fallback, contract normalisation, classification, adsbdb joins, track backfill, **bbox filtering + delta push (SSE/WS)** | One aircraft engine: instanced billboards, model pool, interpolation (consider a Worker), tracking camera, overlay labels |
| Military aircraft | Replace (merge into Flights) | A forked module tree; keep the behaviour | Merge `/v2/mil` into one aircraft stream with an `isMilitary` flag (drops the client registry poll and suppression handoff) | Style variant of the shared engine |
| Local ADS-B | Preserve (later phase) | Well designed and tested; a natural Tauri sidecar | Feed polling with SSRF/DNS guards, merge, push instead of a 1 s poll | Keep the WebUSB SDR + decoder worker; reuse the aircraft renderer |
| Vessels (AIS) | Improve | Already server-brokered; add spatial queries | AISStream ingest, watchdog state machine, ring-buffer store, **R-tree bbox query**, delta push | Billboards, occlusion/rotation, card declutter; drop the continuous-render hold |
| Satellites | Improve | Good maths, wasteful refresh | CelesTrak OMM JSON ingest and cache, dedup/classification, pass prediction (`sgp4` crate), name → NORAD matching | Per-frame SGP4 for the visible/tracked subset (Worker for dense mode), points, GMST-rotated rings; rebuild only on epoch change |
| Space missions | Defer | 5.3k LOC, weakly tested, presentation-heavy | LL2 ingest + payload → NORAD matching against the server TLE store (removes the multi-MB browser download) | Path/replay maths and the custom material later; throttled single `pick` |
| Earthquakes | Improve | Small; currently browser-direct | `/api/earthquakes`: USGS fetch, validation, 60 s cache, bbox/magnitude filter | Batched primitive instead of N clamped ellipses; add a pick card |
| Weather radar/satellite/lightning | Preserve features, Improve internals | Well tested, already server-routed | nowCOAST WMS → manifest, GetMap proxy with a byte-bounded LRU, server-side slicing of the global IR mosaic, optional IR alpha | Imagery providers, shell primitive + material, shared clock |
| Wind | Preserve (streamlines) | Good renderer; drop the hacks | GRIB range fetch + decode (eccodes binding or pure-Rust GRIB2), resampling, explicit little-endian grids, optional pre-baked streamlines | Primitive + material renderer; remove the canvas fallback and the `globe.material` relief override |
| Cyclones | Preserve | Small and well tested | NHC/ArcGIS aggregation and caching; serde schema = `validateCycloneSnapshot` | Entities are fine at this scale; port `labels.js` |
| FIRMS fires | Improve | Whole-world dump plus client binning | Ingest (key), CSV parse, **per-LOD cell aggregation + bbox/top-N** | Batched cell primitive or heat texture, point sprites, horizon cull, label budget |
| Fire perimeters | Preserve | Clean and well tested | Normalisation, per-zoom simplification (vector tiles if large), InciWeb join so cards arrive pre-linked | Batched GroundPrimitive/GPP, cards |
| Recent imagery | Improve (or Defer) | Useful, but browser-direct and a monolith | CMR search, caching, day grouping, thumbnail probe proxy, optional GIBS tile proxy | Split `index.js` into a store, a catalog client and a renderer adapter; swipe UI |
| Traffic | Improve | Heavy browser decoding; known starvation issues (`legacy/docs/KNOWN-ISSUES.md`) | TomTom proxy + daily budget (keep), OpenFreeMap road proxy/cache, MVT decode, flow → road matching, hybrid de-duplication → compact roads-with-flow per tile | Dot simulation (GPU/instanced), heat lines, footprint tiling, height sampling |
| Transit | Preserve (behaviour), Replace backend | Good client, request-driven server | Background GTFS-RT polling (prost + official `.proto`), history ring buffers, backoff/admission, **viewport filtering**, push | Playback interpolation, billboards, trails, card; leave QA code out of the bundle |
| Bikeshare | Improve | Client-supplied upstream URLs, no cache | Registry-id API, cache, `gbfs.json` discovery with TTL, v2/v3 normalisation, bbox filter | Points, heights batched through the ground service |
| Directions | Preserve (behaviour), Improve (structure) | Monolith using Cesium labels | OSRM proxy with throttling/cache/step normalisation (exists); optional self-hosted OSRM/Valhalla | Split into route model, renderer and controller; primitives + overlay labels |
| CCTV | Improve | High value; 19.6 s cold start; complex geometry | Catalog ingest for 13+ agencies, health, frame proxy with fallback, HLS relay with leases/caps, ground-height precompute, **bbox-served catalog**; keep per-source licence and opt-out | Frustum maths/rendering, cards, calibration, video |
| ALPR | Defer (off by default) / Remove | Surveillance-infrastructure map, third-party host, viewport leak, ODbL | If kept: ingest the hourly extract or OSM and serve from our own origin | Markers + wedges only |
| Radio | Preserve (later phase) | Self-contained and fun, but weak in-dir tests | Mirror discovery, curation, caching; optional stream proxy for privacy, mixed content and CORS | Playback state machine, a custom screen-space clusterer instead of private Cesium internals |
| Street level (Mapillary) | Preserve frontend, Improve backend | Best-tested layer; client-exposed token | Tile proxy/cache (exists), Graph API proxy so the token stays server-side (MapillaryJS still needs a client token) | Coverage rendering, ground casting, viewer host, follow |
| Military installations | Improve | Browser decodes vector tiles; three hidden tiers | OpenFreeMap MVT fetch/decode, Overpass proxy, `names.json` R-tree with bbox queries | Rendering and selection only |
| Military awareness (Contacts) | Improve | Derived view reaching into manager internals | Later: a "contacts near X" spatial query once feeds are server-side | Port the engine; rebuild the panel as components |
| Datacenters / dams | Improve | Static data, generic need | Serve datasets with a centroid/bbox index (one copy shared by the layer and the analyst); drop `dams.geojson` | Generic point-stem infra layer on primitives + overlay cards |
| Submarine cables | Preserve feature, Improve rendering (licence gate) | 412 MiB heap from entities; NC licence | Serve pre-simplified GeoJSON (gzip/ETag) | Batched GPP/PLC instead of thousands of entities; NC toggle |
| Regions / boundaries | Replace delivery, Preserve logic | The browser downloads ~8.7 MB | `/api/regions/at`, `/api/regions/lookup` (point-in-polygon, name search) | Optional boundary rendering only |
| Regional briefing | Preserve contract, Replace implementation | Already server-side | Open-Meteo, news via a real XML parser, GDELT fallback, caching | Cockpit/HUD consumer |
| Detection overlay | Defer | Expensive (34–41 FPS) cosmetic effect | — | Instance-scoped service later; keep policy/draw/cohort maths |
| Analyst engine | Preserve | Pure | Optional server-side queries over server stores | Port as-is |
| Bhote Koshi event / locator | Remove from core (Defer as a generic event pack) | One-off, 6k+ LOC, third-party SDK injection, NC data | Generic declarative event-pack schema + serving | Generic event-pack/scene player |
| World overlay host | Improve | Good concept, singleton implementation | — | Instance-owned overlay renderer (canvas or WebGL text) keeping arbitration and budgets |
| Map stacks | Preserve | Clean controller; fix secrets | Implement `/api/google/tiles-token`; optional ion/tile proxying and caching | `MapSourceController` port |
| Search | Improve | Keep the composition, remove the browser key | Proxy Google geocode/reverse and Photon; Nominatim | Provider chain + coordinate/preset parsing |
| Data credits | Preserve content, Replace format | HTML strings today | Structured provider registry `{key, name, url, license, nonCommercial}` | Render from the registry |

### Top recommendations

1. **Make the Rust backend the only network egress.**
   * Proxy every host in section 6, or allow only tile hosts in a strict Tauri CSP; drop the open `https:` CSP.
   * Stop injecting `GOOGLE_MAPS_API_KEY`, `CESIUM_ION_TOKEN` and `MAPILLARY_CLIENT_TOKEN` into the bundle, and
     implement `/api/google/tiles-token` server-side (the client already supports it).
2. **Move ingestion, normalisation and spatial filtering to Rust.** Today flights, vessels, FIRMS, transit, CCTV, launches
   and regions ship whole-world or whole-feed payloads. Serve already-normalised records filtered by bbox and zoom under
   one typed contract (`legacy/src/sources/live/contract.js` is a good starting point), and push deltas over SSE or
   WebSocket. Move MVT decoding (traffic, installations, ALPR), FIRMS aggregation, GTFS-RT, GRIB and TLE handling to the
   server too.
3. **Replace the lifecycle manager and globals with typed, instance-owned services.** That means a small typed layer
   registry and store, one pick dispatcher, one scheduler (poll, viewport and debounce), and instance-owned render
   governor, overlay host and selection in place of module singletons and `window` events. Merge flights and military
   into one aircraft engine.
4. **Port the good pure pieces in 7.1 almost verbatim.** These include the app lifecycle, source contracts, share codec,
   map controller, search chain, and the geometry, orbit, route, datum, LOD and arbitration maths.
5. **Phase the layer port:**
   * **Phase 1:** aircraft (merged), vessels, satellites, earthquakes, weather/wind/cyclones, FIRMS, perimeters, datacenters/dams.
   * **Phase 2:** traffic, transit, bikeshare, directions, CCTV, street level, installations, cables (licence gate).
   * **Deferred:** launches, radio, local ADS-B, recent imagery, detection.
   * **Legal review:** ALPR, which stays off by default.
   * **Removed from core:** Bhote Koshi.
