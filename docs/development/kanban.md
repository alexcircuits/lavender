# Lavender kanban — orchestrator board

Working board for the orchestrated continuation. One line per card, moved by
hand as waves complete; Flokzy (`task-index.md`) remains the system of record
for the ENG-\* tasks this board references. Update together with
[PROGRESS](../PROGRESS.md). Card numbering: LB-1xx (this board), defined in
[upstream-delta-2026-10.md](upstream-delta-2026-10.md) and
[language-fit.md](language-fit.md).

## Done

- **LB-166 Route Flight Cinematics & Tour Director (Camera Verbs & Fly Route)**:
  - **Route Flight Engine (`apps/web/src/globe/routeCinematics.ts`)**: Ported `legacy/src/cameraVerbs.js` and `legacy/src/routeCinematics.test.mjs`. Full trajectory arc sampling (`arcPoint`, `parseRouteCoordinates`), trapezoid velocity profiling with acceleration ($6\text{ m/s}^2$) and deceleration ($8\text{ m/s}^2$), locked $-32^\circ$ pitch attitude, coordinated centrifugal bank angle kinematics ($\phi = \arctan(v^2 \cdot \kappa / g)$ clamped to $\pm 18^\circ$ with $4.0\text{ rad/s}$ damping), lookahead gaze turning ($150\text{ m}$ ahead), and altitude breathing + corridor floor caching ($60\text{ m}$ AGL clearance, terrain lookahead, non-descent guarantee).
  - **Globe Engine & Controller Wiring (`GlobeEngine.ts`, `globe/index.ts`, `test/fakes.ts`)**: Added `flyRoute(route, options)`, `stopRouteFlight()`, `isRouteFlightActive()` to `GlobeController`. Integrated into per-frame render loop; any camera intent (drag, wheel, click, flyTo, resetNorth) immediately aborts active route flight and levels camera roll.
  - **Directions Panel UI (`DirectionsPanel.tsx`, `directions.module.css`)**: Added "Fly Route" / "Stop Flight" toggle with speed selector pills (`Slow` $15\text{ m/s}$, `Normal` $35\text{ m/s}$, `Fast` $80\text{ m/s}$). Responsive real-time speed transitions and live status updates.
  - **Tests & Verification**: 26 unit tests in `routeCinematics.test.ts` and 10 unit tests in `DirectionsPanel.test.tsx` passing.

- **LB-167 Starfield Photometry & Celestial Sky Background (Yale Bright Star Catalogue & Milky Way Cube Map)**:
  - **Photometry Engine & Shaders (`apps/web/src/globe/renderers/starrySky.ts`, `cesium-internal.d.ts`)**: Ported `legacy/src/sky/starrySky.js` and `legacy/src/sky/stars.js`. Conforms to Cesium `Scene#skyBox` interface with custom WebGL draw commands. Starlight points primitive rendered with Gaussian point-spread function shader, Ballesteros B-V effective temperature, and Helland blackbody sRGB spectrum via Zig WASM micro-kernel `lavender_sky_build_star_vertices` (`wasmSkyKinematics.ts`). Milky Way cubemap with interleaved-gradient dither and variable intensity multiplier.
  - **Inertial Celestial Rotation**: ICRF-to-fixed transformation (`Transforms.computeIcrfToFixedMatrix`, fallback to TEME `computeTemeToPseudoFixedMatrix`) with IAU 2006 table preloading and 1-second caching epsilon.
  - **Globe Engine & Settings Integration (`GlobeEngine.ts`, `preferencesStore.ts`, `SettingsPanel.tsx`, `createRuntime.ts`)**: Dynamic toggle in Settings under "Display & Imagery" for "Starry Sky & Milky Way", synced via runtime preferences.
  - **Assets & Budget Governance**: 9,096 Yale Bright Stars (`bright_stars.json`) and Milky Way cubemap face textures served on demand from `public/assets/sky/`; initial JS bundle **250.0 KiB gzip <= 250.0 KiB budget**; 9 unit tests in `starrySky.test.ts` passing.

- **LB-120 Application Internationalization & Localization Engine**:
  - **i18n Core & Reactive Store (`apps/web/src/i18n/`)**: Minimal zero-dependency localization architecture (`locale.ts`, `index.ts`). Dynamic, on-demand code-splitting for language catalogs (`es.json`, `ru.json`, `zh-CN.json`) via `import('./locales/*.json')` ensures zero bytes of translation overhead for default English users. Features robust pluralization (`tn`), parameter interpolation (`msg`), browser locale auto-detection, persistent locale storage, and locale-aware number/datetime formatters (`formatNumber`, `formatDateTime`).
  - **Preferences & UI Wiring (`SettingsPanel.tsx`, `preferencesStore.ts`)**: Integrated language dropdown in Settings with native language names (English, Español, Русский, 简体中文), reactive runtime updates across the shell and panels.
  - **Bundle Budget & Test Verification**: Workspace JS bundle **249.9 KiB gzip <= 250.0 KiB budget**; 24/24 unit tests in `i18n.test.ts` passing.

- **LB-165 Camera Ground Guard & Surface Collision Prevention Kinematics**:
  - **Surface Collision Prevention (`apps/web/src/globe/cameraGroundGuard.ts`)**: Ported `legacy/src/cameraGroundGuard.js`. Multi-attempt asynchronous surface mesh height sampling (`scene.sampleHeight`), terrain clearance deficit calculation (`MIN_EYE_CLEARANCE_M = 120`), and cubic easing arrival lift (`easeOutCubic`). Immediately cancels ground lifts on user canvas interaction (`pointerdown`, `wheel`, `keydown`) or programmatic camera steering (`flyTo`, `lookAt`, `zoomIn`, `zoomOut`, `resetNorth`) to eliminate jitter and respect user intent.
  - **Camera Cinematics & Trajectory Profiling (`apps/web/src/globe/cameraCinematics.ts`)**: Dynamic turn-rate bank roll target calculation (`routeBankTargetDeg`), exponential arrival smoothing (`approachValue`), altitude breathing, and $C^1$ trapezoid speed profiling.
  - **Globe Engine Wiring (`GlobeEngine.ts`)**: Attached ground guard `#onArrival` hook to guard camera above ground on `flyTo` arrivals.
  - **Tests & Verification**: 26 unit tests in `cameraGroundGuard.test.ts` and 25 unit tests in `cameraCinematics.test.ts` passing.

- **LB-164 Local Receiver & SDR Ingest Daemon (WebUSB, AVR/Beast TCP Streaming, Mode-S Decoder)**:
  - **WebUSB RTL-SDR Controller & UI (`apps/web/src/features/sdr/`)**: Ported `legacy/src/sdr/` into strict TypeScript module (`usbDevices.ts`, `gain.ts`, `spectrum.ts`, `sdrController.ts`, `LocalSdrPanel.tsx`). Supports WebUSB device selection for RTL2832U devices (`0x0bda:0x2832`, `0x0bda:0x2838`), 29 R820T gain steps (0.0 to 49.6 dB) with persistent storage, FFT power spectrum analysis (`analyzeSpectrum`), and connects to Zig WASM `AdsbStreamDecoder` (`wasmSdr.ts`). Code-split with `SourcesPanel.tsx` integration; workspace JS bundle **249.5 KiB gzip <= 250.0 KiB budget**; 33 unit tests in `src/features/sdr/` passing.
  - **Backend Mode-S AVR & Beast Sockets (`crates/lavender-core` & `crates/lavender-server`)**: Mode-S ASCII AVR frame parser (`parse_avr_stream`), binary Beast frame parser (`parse_beast_stream`) handling 0x1A byte stuffing and resynchronization, DF 11/17/18/19 ICAO extraction, ADS-B extended squitter callsign, velocity, altitude, and CPR position decoding in `local_receivers.rs`. Background TCP stream client daemon (`local_receiver_daemon.rs`) connecting to `dump1090`/`readsb` ports 30002/30005 with SSRF safe IP validation, exponential reconnect backoff, and transparent stream decoding in `POST /api/v1/local-receivers/ingest`.

- **LB-162 Cesium Bloom & HDR Post-Process Normalization**:
  - **Bloom Normalization & Mapping (`apps/web/src/globe/bloom.ts`)**: Ported `legacy/src/bloom.js` and `legacy/src/ui/visualEffects.js` into strict TypeScript module. Versioned scale migration (`v1` 0–100 inverted to `v2` 0–200, unversioned tolerance), Hermite cubic smoothstep easing ($3t^2 - 2t^3$) mapped to Cesium bloom uniforms (`contrast`, `brightness`, `sigma`, `delta`, `stepSize`, `glowOnly`). Gated by perceptual threshold ($> 0.06$).
  - **Globe Engine Integration (`apps/web/src/globe/index.ts`, `GlobeEngine.ts`, `test/fakes.ts`)**: Added `setBloom(enabled, intensity)` to `GlobeController` and `GlobeEngine`.
  - **Preferences & UI Controls (`preferencesStore.ts`, `createRuntime.ts`, `SettingsPanel.tsx`)**: Bloom toggle and 0–200% intensity slider with live readout in Settings under "Display & Imagery". Persisted across sessions and restored on launch.
  - **Tests & Verification**: 23 unit tests in `bloom.test.ts`, verified against Cesium mock scene and legacy test vectors.

- **LB-163 EGM96 Geoid / MSL Altitude Datum Service (F-014)**:
  - **Geoid Undulation Module (`apps/web/src/globe/geoid.ts`)**: Ported `legacy/src/data/geoid.js` into strict TypeScript module. Lazy dynamic import of `egm96-universal` keeps the ~2.7 MB NGA 15-minute grid completely code-split from the eager bundle. Implemented $h = H + N$ and $H = h - N$ display conversion with graceful uncorrected fallback.
  - **Status Bar Datum Integration (`apps/web/src/shell/StatusBar.tsx`, `shell.module.css`)**: Interactive altitude datum toggle button in the status bar (`MSL` vs `HAE`). Computes local geoid undulation from camera center coordinates in real time.
  - **Bundle Optimization (`cameraLimits.ts`)**: Decoupled `state/workspaceDocument.ts` from eagerly importing camera limits, preserving initial JS bundle at **249.2 KiB gzip <= 250.0 KiB budget**.
  - **Tests & Verification**: 11 unit tests in `geoid.test.ts` verifying London (+46.0 m), Austin (-26.9 m), San Francisco (-32.2 m), Denver (-17.0 m), and SFO touchdown (-32.3 m, resolving the famous -15 m cockpit bug to +17 m MSL). 151/151 web test files passing (1,096/1,096 tests).

- **F-108 / F-109 Deep-Link Share Codec, URL Hash State Restoration & ShareModal**:
  - **URL Hash Share Codec (`apps/web/src/features/share/shareCodec.ts`)**: Minimal versioned deep link encoder & decoder (`v=2`). Encodes camera pose (lat, lon, alt, heading, pitch, roll), active layers via single-character base tokens (`f`, `a`, `s`, `e`, `y`, `w`, `2`, `x`, `b`, `j`), visual optical preset (`noir`, `nvg`, `flir`, `crt`, `normal`), HUD overlay, scope mask, cyber sonar, selected contact (`feed:id`), and tracking mode (`follow` | `cockpit`). Fully backward-compatible with legacy GEV share links while stripping `?lang` overrides to respect recipient preferences.
  - **Share Restoration Hook (`apps/web/src/features/share/useShareRestoration.ts`)**: On app startup, inspects `window.location.hash`, restores camera trajectory via `globe.flyTo` and `globe.restoreCamera`, reactivates persisted layers in `workspaceStore`, restores visual optical preset in `preferencesStore`, restores selection and tracking (follow or cockpit), and notifies the user with a toast.
  - **NoirSystems Share Modal (`apps/web/src/features/share/ShareModal.tsx`)**: Code-split modal with live read-only URL input, one-click copy button, summary metadata (coordinates, active layers, selection, preset), and customizable include toggles.
  - **Shell Wiring & Budget Governance**: Added "Share" button (`CopyIcon`) in `AppShell.tsx` top ribbon; code-split via `React.lazy()` + `<Suspense>`; workspace JS bundle **249.7 KiB gzip <= 250 KiB limit**; all 143 test files passing (**1,003/1,003 tests**); Biome 100% clean.

- **Cockpit View Mode, Primary Flight Display (PFD) Avionics HUD & Aircraft Camera Kinematics**:
  - **Entity Tracking Service & Kinematics (`apps/web/src/globe/tracking.ts`)**: Added `TrackingMode = 'follow' | 'cockpit'` to `EntityTrackingService`. Cockpit view integrates shortest-arc heading slewing via `slewHeading` (`kernels/zig/src/kinematics.zig`), locks camera 15m forward and 3m above the aircraft ellipsoid coordinate, applies -3° downward pitch attitude, and updates synchronously on frame renders.
  - **Avonix HUD Primary Flight Display (`apps/web/src/features/cockpit/`)**: Implemented `CockpitPfd.tsx`, `cockpit.module.css`, `CockpitPfd.test.tsx`, `index.ts`. Authentic aviation PFD displaying top compass tape with cardinal headings, left airspeed tape (knots IAS), right altitude tape (ft MSL), center artificial horizon ladder with +/-10° pitch rungs and aircraft boresight reticle, bottom telemetry strip (Callsign, Registration, ICAO Type, Vertical Speed FPM, Squawk code), and one-click "Exit Cockpit" button, themed dynamically across Noir, NVG, FLIR, and CRT visual presets.
  - **Globe Controller & Shell Wiring (`apps/web/src/globe/index.ts`, `GlobeEngine.ts`, `test/fakes.ts`, `shell/AppShell.tsx`, `shell/FollowingChip.tsx`, `features/inspector/Inspector.tsx`)**:
    - Added `enterCockpit(target)`, `exitCockpit()`, `isCockpitActive()` to `GlobeController` and `GlobeEngine`.
    - Added Quick Action "Cockpit" toggle button (`AirplaneIcon`) in `Inspector.tsx` whenever an aircraft contact is selected.
    - Updated `FollowingChip.tsx` with dynamic cockpit detection ("Cockpit [ID] · Esc to exit").
    - Lazy-loaded `CockpitPfd` in `AppShell.tsx` within `<Suspense fallback={null}>` without inflating startup bundle.
  - **Bundle Budget & Test Verification**: Workspace JS bundle **248.1 KiB gzip <= 250 KiB limit**; all 141 web test files passing (**997/997 tests**); full Rust workspace test suite passing; Biome check 100% clean; contracts up to date.

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

- **LB-122** AI enrichment (owner gate) · **LB-130** Zig WASM rebuild-provenance script + CI · **LB-131** vision sidecar lifecycle in lavender-server · **LB-132** Nim CEP retire-or-wire decision.
- Release track (Flokzy-owned): ENG-996–999 desktop, ENG-1000–1004
  measurement/CI, ENG-1005–1009 guides/deploy/licence/legacy-retirement.

## In Review

- (none — ENG-974/976/983/1003 remain in review in Flokzy from before this
  board; see `task-index.md`.)

## Fleet watchdog log
- 2026-10-25 17:40 run 1: 2 unpushed commits (97aac60b, 93174c75), dirty set alerts/briefing/audit + 11 modified; baseline fingerprint b7db47cd85b7c334 — no prior run, baseline recorded only.
- 2026-10-25 19:0x run 2: fleet FINISHED (last write 17:33). Full verification: Rust chain green (fmt, clippy -D warnings, tests, contracts); web green with load-flaky timeouts only (AnnotationsPanel Escape, launchesIsland — pass in isolation; 877/877 seen in a clean run). Fixed: startup budget 262→239.3 KiB by code-splitting alerts/directions/replay/briefing panels. Committed fleet work + fixes (95229e41), PUSHED through 95229e41, demo server restarted on the new build (divisions API live).
- 2026-10-25 run 3: no change since run 2 (clean tree, 0 unpushed, HEAD 3737afe7 already verified+pushed, demo 200) — no-op, full re-verification skipped as redundant.
