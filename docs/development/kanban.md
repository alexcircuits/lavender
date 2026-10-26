# Lavender kanban — orchestrator board

Working board for the orchestrated continuation. One line per card, moved by
hand as waves complete; Flokzy (`task-index.md`) remains the system of record
for the ENG-\* tasks this board references. Update together with
[PROGRESS](../PROGRESS.md). Card numbering: LB-1xx (this board), defined in
[upstream-delta-2026-10.md](upstream-delta-2026-10.md) and
[language-fit.md](language-fit.md).

## Done

- **Reconnaissance Intelligence HUD, Tactical Scope Aperture Mask, Cyber Sonar Sweep, Solari Split-Flap & Zig Optics Kernel**:
  - **Zig Freestanding Optics Kernel (`kernels/zig/src/optics.zig`)**: Implemented zero-allocation micro-kernel for Ground Sample Distance (`lavender_sensor_calculate_gsd`), NIIRS General Image Quality Equation (`lavender_sensor_calculate_niirs`), Off-Nadir Angle (`lavender_sensor_calculate_ona`), high-precision solar elevation angle (`lavender_sensor_sun_elevation`), tactical Cyber Sonar radial sweep beam gain with hermite feathering (`lavender_sonar_calculate_sweep_gain`), quantized scope terminus alpha ramp (`lavender_scope_terminus_alpha`), and keyhole circular aperture geometry (`lavender_keyhole_geometry`). Rebuilt `lavender_kernels.wasm` (`70/70` Zig tests passing).
  - **Web Micro-Kernel Bridge (`apps/web/src/globe/renderers/wasmOptics.ts`)**: WASM bridge with aligned memory management and pure-TS reference fallbacks.
  - **NoirSystems Reconnaissance Intelligence HUD (`apps/web/src/features/hud/`)**: Satellite reconnaissance HUD overlay displaying real-time MGRS grid coordinates, DMS coordinates, MSL altitude, GSD, NIIRS, ONA, solar position, view band (Street, City, Metro, Regional, Global), nearest city landmark vector, and classification banner (Top Secret, Secret, Confidential, Unclassified), themed by optical preset (NVG, FLIR, CRT, Noir).
  - **Tactical Scope Aperture Mask (`apps/web/src/features/scope/`)**: Keyhole aperture vignette with altitude-adaptive terminus alpha (0.94 above 10 Mm to preserve background starfield, 1.0 below 7 Mm for crisp focus) and cardinal compass ticks.
  - **Cyber Sonar Acoustic Sweep (`apps/web/src/features/sonar/`)**: Radial sweep beam, 8 concentric distance rings, and cardinal spokes.
  - **NoirSystems Solari Split-Flap Component (`apps/web/src/design/primitives/SplitFlap.tsx`)**: Mechanical Solari departure-board character flip component with left-to-right staggered animation.
  - **Shell Wiring & Budget Governance**: Integrated into `AppShell.tsx` and `SettingsPanel.tsx` with dynamic code-splitting; initial workspace JS bundle **247.6 KiB gzip <= 250 KiB limit**; all 140 web test suites passing (**994/994 tests**).

- **LB-116 Street Level Remainder, LB-118 Evidence Tours & Director Moves, LB-119 Map Stacks Catalog, LB-123 Awareness Navigation, Zig Sky & Kinematics Kernels, Radio & ALPR Globe Renderers & Scenes Panel**:
  - **Zig Freestanding Kernels (`kernels/zig/src/sky.zig` & `kernels/zig/src/kinematics.zig`)**: Implemented starfield photometry with Ballesteros B-V effective temperature conversion, Helland piecewise sRGB approximation, peak normalization, and Yale Bright Star Catalogue vertex buffer generation (`lavender_sky_build_star_vertices`). Implemented kinematics and camera director with shortest-arc heading slewing (`lavender_kinematics_slew_heading`), bounded cockpit anchor correction (`lavender_kinematics_anchor_correction`), 6-DOF camera trajectory interpolation with antimeridian wrapping and cubic easing (`lavender_director_sample_camera_move`), and WMO weather code effect profile derivation (`lavender_weather_derive_effects`). All `64/64` Zig tests passing; rebuilt `lavender_kernels.wasm`.
  - **Rust Domain & Providers (`crates/lavender-domain` & `crates/lavender-core`)**:
    - `street_level.rs` & `providers/street_level.rs` (`LB-116`): KartaView segment chaining + Panoramax Ramer-Douglas-Peucker line simplification, plus capture timestamp sanitization (upstream #1341).
    - `scene_pack.rs` & `providers/scene_packs.rs` (`LB-118`): 17-beat Nepal Landslide & Flood tour, Maritime Chokepoints tour, Launch Corridors tour, and camera-move sampler with short-arc antimeridian interpolation.
    - `map_catalog.rs` & `providers/map_catalog.rs` (`LB-119`): 5 canonical map stacks (OSM Carto, OpenTopoMap, Satellite Composite, Dark Monochrome, Stamen Watercolor) with credential-aware availability diagnostics.
    - `awareness.rs` & `analytics/awareness.rs` (`LB-123`): focus, history, and navigation context model, hysteresis refresh decisions, and bounded proximity window contacts.
    - Added routes `GET /api/v1/scenes/packs` and `GET /api/v1/maps/catalog` in `crates/lavender-server/src/routes.rs`.
  - **Web Bridges, Cesium Renderers & NoirSystems Panels (`apps/web/src/`)**:
    - Added `wasmSkyKinematics.ts` (WASM micro-kernel bridges + pure-TS reference fallbacks with 8-byte aligned memory management).
    - Added `RadioRenderer` (`globe/renderers/radio.ts`) and `AlprRenderer` (`globe/renderers/alpr.ts`) with primitive collections, nearest-to-center capping, halo tracking, and click picking.
    - Added NoirSystems inspector sheets `RadioCard.tsx` and `AlprCard.tsx`.
    - Added `ScenesPanel` (`features/scenes/`) with multi-beat evidence tour playback, fly-to trajectory triggering, layer activation checks, and ADR 0005 platform integration.
    - Added `MapCatalogCard.tsx` (`features/imagery/`) for map stack inspection with availability indicators.
    - 977/977 web tests passing; startup bundle size 248.8 KiB gzip <= 250 KiB limit.

- **LB-115 GNSS Integrity, LB-117 Local Receivers & SDR Mode-S/IQ Kernels, Radio Catalog, ALPR Layer & LB-121 Analyst Query Packs**:
  - **Zig Freestanding Kernels (`LB-115` + `LB-117` — `kernels/zig/src/gnss.zig` & `kernels/zig/src/sdr.zig`)**: Implemented DO-260A/B navigation integrity anomaly classification (`lavender_gnss_classify_observation`), `gpsjam.org` 1-aircraft false-positive discount (`lavender_gnss_discount_percent`), severity banding (`lavender_gnss_classify_level`), zero-allocation equirectangular cell binning (`lavender_gnss_bin_cells`), 2 Msps I/Q Manchester envelope demodulation (`lavender_iq_extract_adsb_frames`), and Mode-S DF17/DF18 extended squitter decoding (`lavender_adsb_decode_frame`). Rebuilt `apps/web/public/wasm/lavender_kernels.wasm` (`51/51` Zig tests passing).
  - **Rust Domain Contracts & Backend Providers (`crates/lavender-domain` & `crates/lavender-core/src/providers/`)**:
    - `local_receiver.rs` & `providers/local_receivers.rs` (`LB-117`): Loopback/RFC1918/Tailscale CGNAT SSRF validation, `dump1090`/`readsb` `aircraft.json` normalization (`tisb`/`mlat` source tagging, Mach speed conversion, stale pruning), and multi-band receiver health aggregator (`adsb`, `ais`, `aprs`, `ism`).
    - `radio.rs` & `providers/radio.rs`: Radio Browser public mirror rotation, HTTPS enforcement, coordinate sanity checks (`|lat|<0.05 && |lon|<0.05` null-island rejection), category classification (`public_safety`, `utility`, `news`, `music`, `talk`), and spatial bounding-box search.
    - `alpr.rs` & `providers/alpr.rs`: OpenStreetMap Overpass `surveillance:type=ALPR` camera normalization, cardinal/numeric direction parsing (`0..360°`), and viewport zoom gating (`min_zoom >= 10`).
    - `query_pack.rs` & `analytics/query_packs.rs` (`LB-121`): 10 multi-domain analyst query packs (`aircraft_in_area`, `military_aircraft_watch`, `vessels_in_area`, `maritime_dark_targets`, `satellites_overhead`, `hazards_in_area`, `gnss_interference_watch`, `alpr_surveillance_scan`, `submarine_cables_in_area`, `theater_situation_brief`) with ICAO airline designator extraction, ITU-R M.1371 ship family normalization, category breakdowns, numeric summary statistics (`min`/`median`/`max`), and parameterized `DuckDB` Spatial SQL generation.
    - REST API routes in `crates/lavender-server/src/routes.rs`: `/api/v1/gnss`, `/api/v1/local-receivers`, `/api/v1/radio`, `/api/v1/alpr`, `/api/v1/analytics/query-packs`, and `/api/v1/analytics/query-packs/{id}/execute`.
  - **Web WASM Bridges, Cesium Renderer & NoirSystems Inspector (`apps/web/src/`)**: Added `wasmGnss.ts` and `wasmSdr.ts` (Zig WASM bridges + pure-TS fallbacks), `GnssRenderer` (`globe/renderers/gnss.ts`), and `GnssCard.tsx` (`features/inspector/GnssCard.tsx`) with mandatory unvalidated-interpretation provenance banner.
- **LB-140 Item 9 & Item 10 + Legacy Tactical Frontend Ports**:
  - **Search-Around & Co-Location Graph (`LB-140 Item 10` / `LB-123` / `legacy/src/layers/awareness/model.js`)**: `searchAround.ts` and `SearchAroundCard.tsx` inside `Inspector.tsx` compute great-circle distance and cardinal bearings across all active live feeds (`aircraft`, `vessels`, `earthquakes`, `cyclones`, `fires`, `launches`, `bikeshare`, `transit`) within configurable radii (`50 km`, `150 km`, `500 km`), rendering an interactive SVG radial proximity radar, cohort summary badges, and 1-click multi-hop entity pivoting (`Pivot to ...`).
  - **Monitoring Mission Snapshots & Saved Workspace Scenes (`LB-140 Item 9` / `LB-118` / `legacy/src/ui/sceneControls.js`)**: Extended `MissionsPanel.tsx` and `features/missions/model.ts` with validated composite workspace scene snapshots (`MissionScene`: active layers, base imagery, and target selection) persisted in device preferences, plus 1-click `Focus entity` from mission cards.
  - **Tactical Optics Presets (`legacy/src/ui/visualPresets.js`)**: Added GPU-composited optical modes (`True Color`, `Noir`, `FLIR`, `NVG`, `CRT`) in `preferencesStore.ts`, `AppShell.tsx`, `shell.module.css`, and `SettingsPanel.tsx` with zero WebGL post-process framebuffer overhead.
  - **Curated Tactical Cities & Landmarks (`legacy/src/cityPois.js`)**: Ported 9 global cities and 45 tactical landmarks into `lib/cityPois.ts` and wired the `Quick locations` menu in `MapControls.tsx`.
  - **RFC 7946 GeoJSON Import & Export (`legacy/src/ui/geoFileImport.js`)**: Added `features/annotations/geojson.ts` and wired `Import GeoJSON` / `Export GeoJSON` into `AnnotationsPanel.tsx`.
  - **Saved Searches UI & Layer Explorer Family Filter**: Added `SavedSearchesTab.tsx` inside `AlertsPanel.tsx` and added family filter tabs (`All families`, `Air`, `Sea`, `Space`, `Hazards`, `Context`) + `Turn all off` control to `LayerExplorer.tsx`.
- **Globe Performance & NoirSystems Flat Design Overhaul**:
  - Eliminated Cesium 3D globe rendering lag and stutter in `apps/web/src/globe/GlobeEngine.ts`: clamped `resolutionScale = 1.0` and enabled `useBrowserRecommendedResolution: true` (reducing fragment shading pixels by 84% on Retina screens, from 12.4M down to 2M pixels per frame), disabled expensive WebGL2 multisample resolve passes (`msaaSamples: 1`), set high-performance WebGL context options, tuned camera inertia (`0.6–0.65`) and bounce animation time (`0.05`), and bypassed depth buffer readback stalls.
  - Debunked huge clusters of text across Inspector, Sources, Search, and Alerts: introduced Hero KPI metrics cards (`extractHeroMetrics`) above the fold for Aircraft (altitude/speed/track), Vessels (speed/course/heading), Satellites (NORAD ID/inclination/mean motion), Cyclones (winds/pressure/category), Earthquakes (magnitude/depth/status), and Fires (FRP/brightness/confidence).
  - Reorganized Inspector with a pinned Quick Action Ribbon (`Fly to`, `Follow`/`Stop following`, `Copy coordinates`, `Open source record`) and Segmented focal tabs (`All`, `Telemetry`, `Identity`, `History`, `Provenance`) using persistent container visibility.
  - Transformed UI styling to canonical NoirSystems language (`libs/noir-design`): eliminated 1px dividing borders across cards, search legends, and alert items; applied `.sq` squircle smoothing masks, flat `--well` surfaces, `--control` pills, and 3px–6px card gaps.
- **LB-140 Item 5 Saved Searches as Persistent Feeds & Alerting**:
  - High-performance Rust core engine in `crates/lavender-core/src/search/saved.rs`: thread-safe `SavedSearchEngine` with bounded capacity (max 1,000 searches), substring matching across all 10 feed entity types (Aircraft, Vessels, Satellites, Earthquakes, Cyclones, Fires, Launches, Perimeters, Bikeshare, Transit), deduplicated match tracking with `BoundedLru` (max 500 matches per search), dynamic virtual `FeedSnapshot` generation, and alerting integration.
  - Domain contracts in `crates/lavender-domain/src/entities/saved_search.rs`: `SavedSearch` and `SavedSearchMatch` with `ts-rs` derives and `AlertKind::SearchMatch`.
  - REST endpoints in `crates/lavender-server/src/routes.rs`: `POST /api/v1/searches/saved`, `GET /api/v1/searches/saved`, `GET /api/v1/searches/saved/{id}`, `DELETE /api/v1/searches/saved/{id}`, `GET /api/v1/searches/saved/{id}/feed`, and `GET /api/v1/searches/saved/{id}/matches` with integration test coverage.
  - Web platform wiring in `WebPlatform.ts` and `fakes.ts`.

- **LB-140 Item 6 Entity Resolution Across Feeds**:
  - Deterministic Rust resolver in `crates/lavender-core/src/resolution/`: dual-index generational LRU
    canonical entity store (`max_entities = 50,000`) and alias index with automatic eviction,
    cross-feed ingest auto-linking (aircraft hex/callsign/registration, vessel MMSI/IMO/name,
    satellite catalog ID/designator/name), and manual alias linking/splitting and entity merging with
    provenance link tracking.
  - Domain contracts in `crates/lavender-domain/src/entities/resolution.rs`: `CanonicalEntity`,
    `EntityIdentifier`, `EntityKind`, `EntityLink` with `ts-rs` TypeScript bindings.
  - REST endpoints in `crates/lavender-server/src/routes.rs`:
    `GET /api/v1/entities/resolve?scheme=...&value=...`, `GET /api/v1/entities/{id}`,
    `GET /api/v1/entities/{id}/aliases` with full OpenAPI error envelopes and integration tests.
- **Globe Performance & LB-114 Air Quality UI**:
  - Eliminated Cesium globe stutter and render lag in `apps/web/src/globe/GlobeEngine.ts` by bypassing ocean
    specular reflection shaders (`showWaterEffect = false`), preloading ancestor parent tiles for instant zoom/pan
    display without blank patches (`preloadAncestors = true`), halting offscreen sibling speculative downloads
    (`preloadSiblings = false`), disabling celestial sun/moon ephemeris calculation passes (`scene.sun.show = false`,
    `scene.moon.show = false`), and optimizing camera bounce animation (`bounceAnimationTime = 0.1`).
  - Implemented `AirQualityRenderer` (`apps/web/src/globe/renderers/airQuality.ts`) using primitive collections,
    band-tinted badges, distance culling, halo tracking, and click selection.
  - Added `AirQualityCard` inspector sheet (`apps/web/src/features/inspector/AirQualityCard.tsx`) displaying AQHI
    health risk assessment, advice note, monitoring zone, station facts, and fly-to navigation.
- **LB-109 Provider verification audit**:
  - Full static and dynamic verification audit across all 28 Rust data providers under
    `crates/lavender-core/src/providers/` (`docs/audit/provider-verification.md`).
  - Evaluated against mandatory roadmap criteria: (a) parser tests on fixtures, (b) error-path
    and secret redaction tests, (c) rate-limit/budget/cache tests, (d) licence/attribution in `sources.rs`
    gated by `registry.rs`, (e) frontend renderer journeys and ADR 0008 privacy governance.
  - Formally gates completion of ENG-990 and ENG-991.
- **LB-150 Timeline replay (web)**:
  - New `features/replay` panel (`ReplayPanel.tsx`, `interpolate.ts`, `replay.module.css`, `index.ts`).
  - Added `'replay'` panel to `SidePanel` union, wired into `SidePanel.tsx` and `NavRail.tsx` with Phosphor `ClockCounterClockwiseIcon`.
  - Replays historical trajectory for active aircraft (`platform.aircraftTrack`) and vessels (`platform.vesselTrack`).
  - Interactive timeline scrubber with live UTC time readout, span formatting, and telemetry (lat, lon, altitude, heading).
  - Play/Pause transport control, 1×/2×/4× playback speed selector, Start/End seek shortcuts.
  - Camera follow via `globe.flyTo` at 500 ms throttled interval.
  - Mirrored globe track trail rendering (`globe.showAircraftTrack` / `globe.showVesselTrack`).
  - 100% test coverage (`interpolate.test.ts`, `ReplayPanel.test.tsx`), zero CSS token violations.
- **LB-152 Admin-1 divisions serving & Wave B layer integration**:
  - Rust core `providers/divisions.rs` serving offline Natural Earth 10m states and provinces
    (`crates/lavender-core/data/natural_earth/states_provinces.json`) with point-in-polygon
    spatial index, bbox prefetch, localized names (`name` and `name_en`), and routes
    `/api/v1/divisions/at` and `/api/v1/divisions/search`.
  - Web Admin-1 drill-down marked `served: true` in `boundaryHierarchy.ts` with `DivisionCard` inspector.
  - Directions & Routing (`ENG-963` / `ENG-991`): OSRM route model and drawing via
    `DirectionsRenderer` with Zig WASM RDP curve simplification (`simplifyRdpWasm`),
    `DirectionsPanel` with travel profile selector (`car`, `bike`, `foot`), Origin/Destination
    waypoint pickers, turn-by-turn maneuvers, camera step tracking, and `calculateRoute` on platform.
  - Wave B Inspector cards: CCTV with Mojo MLIR vision perception stream (`CctvCard`), Submarine
    Cables with TeleGeography CC BY-NC-SA licensing (`CableCard`), Public Transit GTFS-RT with
    Zig WASM RDP trails (`TransitCard`), GBFS Bikeshare docks (`BikeshareCard`), and Military
    Installations with defense surveys and MGRS coordinates (`MilitaryInstallationCard`).
- **LB-110 Borders, divisions & region drawing** (commit 12129766):
  geofence draw tool (polygon/box, naming, persistence, share links),
  per-class boundary styling, honest division drill-down, zoom-aware
  class defaults. Follow-up: admin-1/admin-2 need a richer source
  (upstream `states_provinces.json`/`us_census_counties` are in legacy).
- **LB-111/112 Simulation stack** (commit f2c49257): motion predictor,
  deterministic synthetic aircraft (Aircraft contract), 2-DOF spherical
  ascent integrator with insertion physics + Ascent* TS contract. Hub/UI
  wiring behind a config flag is the next slice.
- **LB-113 Mission boards** (commit 387f01f7): analyst kanban, entity
  links, device persistence, validation.
- **LB-133 NoirSystems token alignment** (commit 06c74058).
- **LB-100 Upstream import.** 1095 commits merged under `legacy/` (`30047df2`);
  `HEAD..upstream/main` empty; legacy byte-identical to `9d3548c2`.
- **LB-101 WIP checkpoint.** Second provider wave + polyglot kernels committed
  (`0eaebbb3`) after fixing contract drift (Freshness union, bigint CCTV
  timestamps, `Located | undefined` + `symbolPx`); web 772/772, `cargo check`
  clean.
- **LB-102 Boundary data relocation.** Derived `boundaries.json` moved to
  `crates/lavender-core/data/natural_earth/` so `legacy/` stays pristine.

## In Progress

- **LB-151 Simulation hub wiring — DONE** (`b67bb1c4`): SimulationProvider
  swaps in for adsb.lol at registration behind `providers.simulation`
  (default OFF); deterministic fleet, 'simulation' source stamp,
  synthetic-data notice. Remaining from the original brief: admin-1
  division serving (see LB-152) and the optional ascent enrichment seam.
- **LB-130 Zig kernels + WASM provenance** (wrappers + script done): the Zig
  kernels already exported point-in-polygon (batch/multi-polygon/wrapped) and
  heatmap binning; `wasmHeatmap.ts` adds the TS wrapper with a pure-TS
  fallback, `scripts/build-zig-kernels.sh` pins the toolchain and rebuilds
  the artifact. BLOCKED: this 0.16-dev toolchain cannot link on macOS —
  bump the pin and rerun (see language-fit.md provenance note).
- **LB-140 Gotham-inspired build list** (from `docs/research/gotham-feature-set.md`
  top-10): geo-search (1), geofence alerting + inbox (2), timeline replay (3),
  provenance view (4), saved searches (5), entity resolution (6), briefing
  export (7), heatmaps (8 — pairs with LB-130), monitoring missions (9 —
  extends LB-113), graph view (10). Pick one per review.

## Backlog (ranked)

- **LB-120** i18n decision · **LB-122** AI enrichment (owner gate) · **LB-130** Zig WASM rebuild-provenance script + CI · **LB-131** vision sidecar lifecycle in lavender-server · **LB-132** Nim CEP retire-or-wire decision.
- Release track (Flokzy-owned): ENG-996–999 desktop, ENG-1000–1004
  measurement/CI, ENG-1005–1009 guides/deploy/licence/legacy-retirement.

## In Review

- (none — ENG-974/976/983/1003 remain in review in Flokzy from before this
  board; see `task-index.md`.)

## Fleet watchdog log
- 2026-10-25 17:40 run 1: 2 unpushed commits (97aac60b, 93174c75), dirty set alerts/briefing/audit + 11 modified; baseline fingerprint b7db47cd85b7c334 — no prior run, baseline recorded only.
- 2026-10-25 19:0x run 2: fleet FINISHED (last write 17:33). Full verification: Rust chain green (fmt, clippy -D warnings, tests, contracts); web green with load-flaky timeouts only (AnnotationsPanel Escape, launchesIsland — pass in isolation; 877/877 seen in a clean run). Fixed: startup budget 262→239.3 KiB by code-splitting alerts/directions/replay/briefing panels. Committed fleet work + fixes (95229e41), PUSHED through 95229e41, demo server restarted on the new build (divisions API live).
- 2026-10-25 run 3: no change since run 2 (clean tree, 0 unpushed, HEAD 3737afe7 already verified+pushed, demo 200) — no-op, full re-verification skipped as redundant.
