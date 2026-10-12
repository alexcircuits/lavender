# Upstream audit: UI shell, visual effects and auxiliary systems

**Subject:** God's Eye View (MIT; vanilla JS, CesiumJS and Vite). The upstream copy is vendored at `legacy/`.
**Purpose:** to decide what the Lavender rewrite (TypeScript frontend, Rust backend) keeps, rebuilds or drops.
**Date:** 2026-10-11.
**Method:** a read-only source audit. `node_modules` is not installed, and nothing was executed.

All paths are relative to `products/lavender/`. Line numbers refer to the file as vendored.

Layers and data sources are covered by sibling audits (`docs/audit/upstream-layers.md` and `docs/audit/data-sources-and-licensing.md`). This document covers the shell, the visual effects, voice, Director/scenes, annotations, tools/MCP, SDR, camera and locations. It touches layer code only where those systems reach into it.

**Verdict vocabulary**

| Applies to | Verdict | Meaning |
|---|---|---|
| Visual effects (section 2) | **Remove** | Do not carry over |
| | **Redesign** | Keep the analytic intent, rebuild the treatment |
| | **Keep** | Carry over essentially as-is |
| Features (section 6) | **Preserve** | Port the behaviour largely as it is |
| | **Improve** | Keep the capability, rework the design |
| | **Replace** | Keep the need, build it differently |
| | **Remove** | Drop it |
| | **Defer** | Not for v1 |

Lavender's guiding philosophy is **information over spectacle**. The upstream identifies itself as the opposite:

- "A spy-satellite simulator in your browser" (`legacy/README.md:7`).
- "the visual grammar of a classified ops room" (`legacy/README.md:546`).
- "Apple meets Blade Runner" (`legacy/src/ui/styles/foundation.css:3`).

The engineering underneath, however, is often careful and worth mining.

---

## 0. Executive summary

### What to throw away

- **None of the six "sensor" styles uses sensor data.** CRT, NVG, FLIR, Anime, Noir and Snow are full-screen GLSL post-process passes over the rendered RGB frame.
  - FLIR prints a fabricated temperature: `float tempC = 20.0 + centerLuma * 30.0;` (`legacy/src/styles/thermal.js:263`).
  - FLIR also prints a fake frame counter, `int(mod(time * 30.0, 10000.0))` (`thermal.js:301`).
- **The HUD fabricates telemetry:**
  - a `TOP SECRET // SI-TK // NOFORN` banner (`legacy/src/hud.js:192,200`);
  - random `KH11-xxxx`, `OPS-xxxx`, orbit and pass numbers drawn from `Math.random()` (`hud.js:169-172`);
  - a static `BAND: PAN / BITS: 11 / LVL: 1A` (`hud.js:241`);
  - a "GSD/NIIRS" value derived only from camera altitude (`hud.js:373-376`).
- **Spectacle is on by default** (`legacy/src/ui/visualPresets.js:44-66`):
  - the circular scope mask (`legacy/src/scopeMask.js:104`, `_enabled = true`; share default `sc` = on at `legacy/src/sharelink.js:217`);
  - the tactical HUD, visible;
  - the "detection" bracket overlay at DENSE 75%;
  - an unsharp-mask "sharpen" at 49.
- **The animated styles defeat the idle renderer.** They hold continuous rendering (`legacy/src/ui/visualEffects.js:205`, `holdRender('style-anim')`), which undoes the purpose of `legacy/src/renderGovernor.js`.
- **The visual styles leak into data code.** Several layer modules compensate for them, and NVG's barrel distortion breaks picking on most layers. See 2.3.

### What to mine

These patterns carry over:

- Lifecycle and ownership discipline.
- Navigation "generations", which arbitrate camera authority.
- The feed-state honesty vocabulary (LIVE / STALE / DEGRADED / FALLBACK / PARTIAL / UNAVAILABLE).
- The loading-feedback reducer.
- The key-setup admission gates.
- Scene-document validation and migration.
- The data-pack provenance model (licence, attribution, sha256).
- The split between command schemas and the descriptions the model sees.
- Deixis sentinels ("pointer", "referent n").
- Conservative voice cost metering.
- The pure `drawMode` geometry.
- The recent-imagery and A/B split workflow.
- The place-framing algorithms.

### Biggest gaps against Lavender's roadmap

1. **There is no data-time model.** The Director runs on wall-clock "authored seconds" and never touches `viewer.clock`. Replaying a scene shows today's aircraft, not the ones present when it was authored.
2. **Annotations are not persisted.** They live in memory only.
3. **There is no server-side state.** It lives in `localStorage` and in an uncompressed URL hash dominated by shader settings.
4. **There are two overlapping command registries:**
   - 30 voice "app actions" in `legacy/src/voice/actionSchemas.js`, run by the 5,019-LOC `gevActions.js`;
   - 40 catalog tools in `legacy/src/tools/index.js:86-127`.
5. **The MCP server has no authentication.** Only a loopback gate protects it.

### Size of what was audited

Non-test lines of code:

| Area | LOC | Notes |
|---|---:|---|
| `legacy/src/ui/**/*.js` | 23,927 | Shell, panels, cockpit UI, context, radio, CCTV, imagery |
| `legacy/src/ui/styles/*.css` | 17,218 | 27 files; `cyber.css` 2,837, `cockpit.css` 2,189 |
| Top-level `legacy/src/*.js` | ~14,000 | HUD, camera verbs, locations, sonar, scope, split-flap, celestial, first-run, key setup… |
| `legacy/src/voice/**` | 15,554 | `gevActions.js` 5,019 |
| `legacy/src/scenes/**` + `legacy/src/director/**` | 5,663 + 1,897 | Plus ~7,300 LOC of Nepal-specific data code under `src/data/bhoteKoshi*` |
| `legacy/src/annotations/**` | 6,202 | `resolver.js` 2,117, `annotationEngine.js` 1,597 |
| `legacy/src/tools/**` | 5,994 | Plus `legacy/server/mcp/*` 526 |
| `legacy/src/sdr/**` | 2,620 | Plus 443 LOC of local-SDR UI |
| `legacy/src/styles/*.js` (GLSL styles) | 1,017 | |

---

## 1. UI shell architecture

### 1.1 Composition and lifecycle

**Page and styles**

- `legacy/index.html` (39 lines) contains only nine `<!-- gev:template X -->` markers. They are spliced from `legacy/src/ui/templates/*.html`:

  | Template | Lines | Notes |
  |---|---:|---|
  | `cockpit.html` | 328 | Mostly SVG visor and instrument frames |
  | `context.html` | 246 | |
  | `display-controls.html` | 182 | |
  | `layer-panels.html` | 180 | |
  | `command-dock.html` | 97 | |
  | `scene-chrome.html` | 61 | |
  | `welcome.html` | 52 | |
  | `provider-settings.html` | 25 | |
  | `hud-loading.html` | 11 | |

- The page loads Google Fonts (JetBrains Mono 300–700, Inter 300–600) and a 34-glyph Material Symbols Outlined subset through `icon_names=` (`index.html:15`). The subset is guarded by `legacy/src/materialSymbolsSubset.test.mjs`. Nothing is self-hosted.
- `legacy/style.css` is only `@import`s of the 27 component stylesheets, in a fixed order. `cyber.css` comes last so its overrides win.

**Bootstrap**

- The chain is `legacy/src/main.js` → `legacy/src/standalone/application.js` (`createStandaloneApplication`) → `legacy/src/app/application.js` (`createApplication`).
- Phases start in the order scene → controls → data → tools and stop in the order tools → controls → data → scene.
- Every constructor receives `{signal, defer}`. Cleanup failures are aggregated.
- The app state is a frozen `{status, phase}` object with `subscribe`. Statuses are `created | starting | ready | destroying | destroyed | failed`.
- A startup crash writes a red "Error: …" into the loader (`main.js:11-15`).
- **Standalone** (`legacy/src/standalone/*`, 266 LOC) is simply the default self-hosted composition: the browser app plus the local key-setup server. It is a page singleton ("reload to start again", `legacy/docs/APPLICATION.md:92-95`).

**Shell**

- `legacy/src/ui/applicationShell.js` (1,625) declares `class StyleManager extends ShellFacade`. The name is historical.
- Its constructor wires about ten "owners" with explicit `elements`, `operations` and `read*` closures, then makes about 20 `_init*()` calls.
- `legacy/src/ui/shellFacade.js` (1,540) exists only for back-compatibility. It has about 143 getters and setters and about 115 forwarding methods, used by voice, scenes and layers.
- `legacy/src/ui/composition.js` (118) injects about 45 service functions (detection, scope mask, render governor, HUD class and so on).

**Ownership rules** (`legacy/docs/UI-OWNERSHIP.md`)

| Owner (`legacy/src/ui/…`) | Owns |
|---|---|
| `navigationController.js` | Navigation generation, tracking release, camera authority |
| `locationNavigation.js` | Destinations, orbit, world jumps, globe-reset completion |
| `cockpitCoordinator.js` | Cockpit entry and rollback, readouts, the Display portal |
| `visualSettings.js` | Effects, detection overrides, visual restoration, render-loop lifetime |
| `panelChrome.js` | Disclosure, docking, layout, cockpit panel restoration |
| `aircraftDisplay.js` | 3D aircraft display preferences |
| `layerBindings.js` | Data-manager attachment, selected-tracker adoption, camera-entry listeners |
| `displayBindings.js` | Keyboard and display subscriptions, frame-rate monitor |
| `shareRestoration.js` | Initial share and layer restoration, cancellation |
| `shellFeedback.js` | Loading feedback, notices, toast |

Owners never receive the shell itself. They receive named services, operations and readers, and readers resolve a collaborator at the moment of use.

**Shutdown** runs in two phases:

1. Revoke input, deferred navigation, camera-entry listeners, lookups, timers and rendering.
2. Await Context restoration, release sensor overrides, detach layers and destroy scene resources.

Late callbacks cannot acquire the camera (`applicationShell.js:1567-1624`). This discipline is sound and worth porting as a pattern. The facade and the god-object shell are not.

### 1.2 Layout: rails, panels, HUD, chips, trays, dock

**Glossary**

| Term | What it is |
|---|---|
| **Rail** | An absolutely positioned vertical stack over the globe, either `#left-panel-stack` or `aside#right-context-rail` |
| **Panel** | `.panel-collapsible[data-panel-id]` with a `.panel-collapse-btn[data-collapse-target]` |
| **Card** | A keyed `<article>` built by `legacy/src/ui/railCards.js` (128) from ordered "blocks" in `railCardBlocks.js` (295): list, actions, chips. Used by weather, recent imagery and context |
| **Chip** | Keyed pill buttons (`legacy/src/ui/chipGroup.js` 53, `legacy/src/mapStackChips.js` 165), plus `aria-live` status chips |
| **Tray** | A popover in the command dock (location bar, visual presets). Hover-open at 140 ms, close at 420 ms (`panelChrome.js:139-146`), with pin buttons |
| **HUD** | The screen-corner telemetry overlay `#intel-hud` (`legacy/src/hud.js`) |
| **Command dock** | `#command-dock`, bottom centre: a grid of `'locations voice presets'`, `width: min(94vw, 1240px)` |

**Regions** (from the templates)

| Region | DOM | Content |
|---|---|---|
| Globe | `#cesiumContainer`, `#world-overlay-root`, `#world-overlay-actions` | Cesium, plus a 2D canvas for labels, plus a screen-reader-only focusable mirror of the canvas cards. Cesium labels are banned by `legacy/docs/CODE-BOUNDARIES.md` |
| Title | `#title-bar`, `#style-indicator` | Logo with "gaze", tagline "NO PLACE LEFT BEHIND", `.title-glow`, "ACTIVE STYLE". Decorative |
| Top-centre actions | `nav#top-center-actions` | Clear layers, share link, tilt, north-up, reset globe. Round 36 px glass buttons |
| Status | `#global-loading-status`, `#traffic-sync-chip`, `#cctv-sync-chip`, `#toast` | Feedback (see 1.8) |
| Left rail | `#data-panel` (DATA LAYERS), `#cctv-panel`, `#street-level-panel` (moved right at runtime), `#scene-panel` | Accordion |
| Right rail | `#weather-panel`, `#recent-imagery-panel`, `#global-context-panel` | The context panel has CONTACTS / SPACE MISSIONS tabs and a nested `#radio-panel` (internet radio plus local RTL-SDR) |
| Display panel | `#pp-toggles`, `#param-slider-panel` | HUD and layout, cyber sonar, detection, 3D models, scope, draw, celestial, clean UI, bloom, sharpen; plus generated shader sliders |
| Dock | `#command-dock` | Location tray (city pills, POI row, search); voice control; "VISUAL PRESETS" tray (styles 1–7 plus MAP SOURCE chips) |
| Cockpit | `#cockpit-hud`, `#view-switcher` | First-person aircraft view (section 2.10) |
| Overlays | `#loading-screen`, `#first-run-launcher`, `#key-setup-chip` / `#key-setup`, `#safe-frame-overlay`, `#clean-view-exit` | |

**Panel-surface contract** (`legacy/docs/panel-surfaces.md`, `legacy/src/ui/styles/panel-surfaces.css` 230)

This is an opt-in set of attributes: `[data-panel-surface]`, `[data-panel-header]`, `[data-panel-body][data-rail-scroller]` and `[data-panel-popup]`. It is CSS only. There is no registry, no focus trap and no collision solver.

**Layout engine** (about 1,460 LOC)

Files:

| File | LOC |
|---|---:|
| `legacy/src/ui/panelLayoutController.js` | 500 |
| `leftPanelRail.js` | 306 |
| `rightPanelRail.js` | 338 |
| `panelRailGeometry.js` | 78 |
| `panelMeasurement.js` | 44 |
| `legacy/src/panelStackLayout.js` | 158 |
| `legacy/src/rightRailPolicy.js` | 37 |

How it works:

- Each pass synchronously measures about 25 obstacle selectors: HUD corners, title bar, dock and credits (`panelLayoutController.js:11-62`).
- `resolveHudRailLayout` centres the rails between those obstacles.
- `allocatePanelStackHeights` keeps natural heights when they fit. Otherwise it gives each panel a 96 px floor and shares the rest in proportion.
- Panels are auto-collapsed as a presentation-only state (`layout-auto-collapsed`).
- Layout reruns on ResizeObserver, MutationObserver, window resize and HUD transitions.
- `settledLeftRailTop` reads CSS transition keyframes to predict where the rail will settle (`rightPanelRail.js:18-35`). This is fragile.

Theme policies:

- Under the tactical HUD, collapsed right-rail siblings are hidden while one panel is open.
- The Cyber theme enforces a one-open-panel accordion (`panelChrome.js:416-440`).

**Docking, floating and resizing** (`legacy/src/ui/panelPositionControls.js`, 842)

- `#pp-toggles` can be dragged.
- `cctv-panel` (minimum 300×160) and `street-level-panel` (minimum 320×280) lift out of the rail when the header is dragged past a 4 px threshold.
- Floating panels resize from eight edges and corners. The resize maths is the pure `resizeBox` in `panelResize.js` (47).
- They are clamped to the viewport with a 6 px margin and pushed clear of the dock.
- A double-press on the header within 400 ms snaps the panel back.
- Floating panels occupy z 100–139.
- Positions are persisted only for floating panels.

**Responsive behaviour**

- The main breakpoint is `max-width: 720px`. Both rails become full-width scroll containers and the left-rail engine switches to `layoutMode = 'mobile'` (`leftPanelRail.js:50-60`). Touch pointers never drag panels.
- Other width breakpoints: 520, 575, 620, 760, 900, 960, 980. Height breakpoints: 620, 640, 649, 700, 820.
- The dock CSS is four stacked override layers (`command-dock.css` → `-compact` → `-trays` → `-sliding`, about 1,950 LOC). This is accretion.

**Z-index ladder** (documented only in comments)

| z | Elements |
|---|---|
| 1 | Scope mask |
| 2 | Celestial ring, HUD |
| 5 | Detection surface |
| 6 | World overlay |
| 7 | Cockpit clouds |
| 60 | Key chip |
| 90 | Credits |
| 95 | Parameter panel, safe frame |
| 100 | Rails, title, dock trays |
| 101 | CCTV chip |
| 102 | Global loading chip |
| 110 | Right rail, `#pp-toggles` |
| 111+ | Floating panels |
| 145 | Dock, cockpit HUD |
| 150 | Voice |
| 175 | First-run launcher |
| 190 | Key setup |
| 200 | Toast, credit lightbox |
| 300 | Clean-view exit |
| 1000 | Loading screen |

**Exclusive modes** are body classes:

| Class | Effect |
|---|---|
| `ui-clean-view` | V key; hides the chrome |
| `recording-mode` | Adds 16:9 or 9:16 safe frames and hides panels (`legacy/src/ui/recordingControls.js`, 97). Nothing is actually captured |
| `cockpit-mode` | Collapses map panels |
| `scene-playback-mode` | Set during Director playback |
| `ui-embed` | Embed mode (below) |

**Embed mode** (`legacy/src/app/embed.js`, 305)

- Activated by `?embed=1` or `GEV_EMBED_INLINE`. It hides all chrome.
- It accepts `postMessage({type: 'gev:view', id, view})` and replies with `gev:view-applied` and `gev:ready`.
- **There is no origin allowlist** (`embed.js:259-296`): any parent frame can drive the camera and layers. Replies go to `event.origin`, or to `*` when the origin is null.

### 1.3 State management

There is no central store. State is spread across:

1. **DOM classes and attributes.** For example `collapsed`, `active`, `dock-pinned`, `panel-floating`, `:root[data-ui-theme=cyber]` and the body mode classes. Other code reads them directly; cockpit Escape, for instance, checks `#context-radio-dock.disclosure-open` (`legacy/src/ui/cockpitInput.js:8-15`).
2. **Instance fields** on about 15 owner classes.
3. **`createStateChannel`** (`legacy/src/app/stateChannel.js`, 111). A good primitive: deep-frozen snapshots, revision ordering, reentrancy safety, isolated listener errors. It is used in only five places: share state, location search, location navigation, camera hand-offs and the Director.
4. **An untyped window `CustomEvent` bus**, including:
   - `gev:map-stack-changed`, `gev:style-change`, `gev:vision-change`;
   - `gev:entity-selected` and `-cleared`;
   - `gev:cockpit-mode-changed`, `gev:cockpit-weather-toggle`;
   - `gev:radio-selected`, `gev:initial-share-restore-settled`, `gev:navigation-authority-taken`;
   - `gev:world-request-focus`, `gev:cctv-request-focus`, `gev:awareness-subject-*`.
5. **Generation counters** that guard async staleness: navigation, context mode and map-stack select.
6. **Restore lanes.** User actions call `shareLinkManager.claimRestoreLane('visual' | 'map' | 'panel', id)`, so a delayed share restore cannot overwrite something the user just did.
7. **`UiLifetime`** (`legacy/src/ui/uiLifetime.js`, 54). It tracks listeners, rAF callbacks and timeouts, and drops them all on `destroy()`.
8. **The debug handle** `window.__godsEyeView` (`legacy/src/app/tools.js:145`).

**Persisted keys.** Every access is wrapped in try/catch. Naming is inconsistent: two prefixes and mixed versioning.

| Key | Purpose |
|---|---|
| `godsEyeView.v6.panelCollapsed.<id>` | Panel open/closed; ignored when a share link is loaded |
| `godsEyeView.v8.panelPos.<id>`, `.v8.panelFloatHintShown`, `.v8.layoutResetNotified` | Floating positions and one-time hints |
| `gev:layer-state:v2` | Durable layer enables (origin user, voice or tool only) |
| `gev:detection-allocation:v1` | Detection allocation |
| `gev:first-run-mission:v1` (local), `gev:first-run-mission-session:v1` (session) | First-run suppression |
| `gev:street-level:render-mode` | Street-level fit/fill |
| `gev:sdr:device:v1`, `gev:sdr:gain:v1` | RTL-SDR device and gain |
| `godsEyeView.cctv.calibration.v2` | CCTV pose calibration (v1 is migrated) |
| `godsEyeView.cockpitWeatherEffects.enabled` | Cockpit clouds |
| `godsEyeView.sceneProject.v2`, `.checkpoint.v1` | Director project |
| `godsEyeView.voice.cloudAuth`, `godsEyeView.voiceCost.tier`, `godsEyeView.voiceCost.limits`, `gev-realtime-errors` | Voice settings and error log |

**Context modes** (`legacy/src/contextModePolicy.js` 437, plus `legacy/src/ui/context*.js` about 1,400 LOC) are transactional layer bundles:

- `contacts` enables military-awareness, flights, military, AIS and installations, with radio as a companion.
- `space-missions` enables launches and satellites, and blocks other layers.

Entering a mode snapshots the user's layers. Exiting restores them, replaying any changes the user made during the session. Disabling a dependency exits the mode. Modes also gate cockpit entry and drive panel auto-expansion (`rightRailPolicy.js`). The implementation is correct but very complex, and it hides coupling (section 5.9).

### 1.4 Keyboard and surface handling

**Shortcuts** (consolidated from the whole codebase)

| Key | Action | Source |
|---|---|---|
| `1`–`7` | Visual style: normal, retro/CRT, surveillance/NVG, thermal/FLIR, anime, noir, snow | `legacy/src/ui/applicationShortcuts.js:1-9` |
| `H` | Toggle HUD (leaves auto mode) | `applicationShortcuts.js` |
| `O` | Toggle auto-orbit around the last POI | same; `legacy/src/orbit.js` |
| `V` | Clean view | same |
| `F` | Toggle Data Layers panel | same |
| `D` | Cycle the "detection" overlay | same |
| `C` | Toggle the CCTV layer | same |
| `C` (when an aircraft is tracked) | Enter or exit cockpit. A capture-phase handler calls `stopImmediatePropagation`, so it takes C from CCTV and swallows the key even when entry is refused | `legacy/src/ui/cockpitInput.js:56-69` |
| `Esc` | Dismiss search (works inside fields too) | `applicationShortcuts.js:33` |
| `Esc` (layered) | Collapse the nearest panel and refocus its toggle; release a tracked flight, military aircraft, satellite, transit vehicle or bikeshare; close the street-level viewer; stop scene playback; cancel a radio drag | `panelDisclosure.js:43-70`, `layers/*/tracking.js`, `sceneControls.js:217`, … |
| `Esc` (cockpit) | Credit lightbox, then radio disclosure, then utility popover, then context panel, then signal panel, then exit | `cockpitInput.js:6` |
| `Q W E R T` | Fly to POI 1–5 of the expanded city | `legacy/src/ui/locationControls.js:2,45-57` |
| `Enter` (search) | Geocode | `locationControls.js:63` |
| `` ` `` | FPS readout in the title bar | `legacy/src/ui/frameRateMonitor.js:48` |
| Hold `Space` (≥500 ms) | Voice push-to-talk; a short tap keeps native behaviour | `legacy/src/voice/realtimeInputPolicy.js:11-16` |
| Arrows / Home / End / PgUp / PgDn | Roving focus in context tabs, the recent-imagery day strip, street-level radio groups, the imagery-split handle and the radio tuner | `contextBindings.js:5-20`, `imagerySplit.js:130-133`, `radioBindings.js:749-757` |
| `Enter` / `Esc` / `Backspace` (draw) | Finish the shape, cancel, or undo the last vertex | `legacy/src/annotations/drawTool.js:296-310` |
| `Enter` / `Space` / `Esc` | Attribution lightbox | `legacy/src/creditKeyboard.js` |
| Any key, pointer down, wheel | Cancels the ground guard; pointer down or wheel interrupts camera verbs | `legacy/src/cameraGroundGuard.js`, `legacy/src/cameraVerbs.js:1221` |

Single-key bindings are ignored inside `select`, `input` and `textarea`. There is **no shortcut registry, help overlay or command palette**. Shortcuts are discoverable only through `title` text and `.btn-key` badges.

**Escape arbitration.** About 15 independent `keydown` listeners mix capture and bubble phases with `defaultPrevented` and `stopImmediatePropagation`. Behaviour therefore depends on registration order.

**Focus and accessibility that work**

- A global `:focus-visible` 2 px outline (`foundation.css:41-70`, enforced by tests).
- `createSurfaceKeyboard` (`legacy/src/ui/surfaceKeyboard.js`, 90) wraps Tab and restores focus to the opener, without being modal.
- The street-level expanded viewer is a real modal: it sets `inert` on body siblings.
- Busy controls use `aria-busy` and `aria-disabled`, so they keep focus.
- Canvas labels are mirrored into a focusable list.
- 12 `prefers-reduced-motion` CSS blocks.
- Split-flap keeps `textContent` stable, so live regions are not announced twice.

**Gaps**

- First-run and key setup are `aside role=dialog`, with no `aria-modal` and no inert background.
- HUD, bloom and sharpen toggles carry state only in a class, with no `aria-pressed`.
- Emoji are used as icons.
- `--text-dim` (30% alpha) is about 2.3:1 on glass, which fails WCAG AA.

### 1.5 Settings and visual settings

**Classes**

- `VisualSettings` (`legacy/src/ui/visualSettings.js`, 1,707) holds the visual state in memory.
- `VisualEffects` (`legacy/src/ui/visualEffects.js`, 235) owns the Cesium post-process stages.
- Each shader's `uniforms` block (default, min, max, label) generates the PARAMETERS sliders (`legacy/src/ui/styleParameters.js`, 65).
- The Display panel is wired by `displayControls.js` (59) and `displayBindings.js` (230).

**First-load defaults** (`GLOBAL_POST_DEFAULTS`, `visualPresets.js:44-66`, applied before share restore)

| Setting | Default | Comment |
|---|---|---|
| Style | `normal` | |
| Bloom | off | |
| Sharpen | **on, 49** | Alters imagery every frame |
| HUD | `tactical`, **visible** | Fake fields; see 2.9 |
| Detection | **DENSE 75%, ELASTIC** | "owner directive 2026-08-22" comment at `visualPresets.js:50-53` |
| Label fade / outside opacity | 7% / 1% | Labels outside the scope circle all but vanish |
| Scope mask | **on**, feather 11% | `scopeMask.js:62,104` |
| Celestial ring | off | |
| Cyber sonar (Cyber theme only) | on: rings 8, range 100, power 70, opacity 84, sector 24 | |

**Hidden coupling.** Selecting NVG, FLIR or CRT forces the HUD on (`hud.js:815`) and forces detection to DENSE 75% unless the user has overridden it (`visualSettings.js:768-775`). Selecting the HUD layout "cyber" reskins the whole shell and forces FLIR with Ironbow at 0.42 (`legacy/src/hudLayouts.js:11`).

**Per-style presets.** `STYLE_PRESET_DEFAULTS` (`visualPresets.js`) sets shader parameters, bloom, sharpen, HUD and detection for retro, surveillance and thermal.

**Persistence.** The URL hash is the main store for visual state (section 1.7). Cyber sonar settings live only in `documentElement.dataset` and are never persisted.

### 1.6 First-run and key-setup flow

**Key registry** (`legacy/src/keySetupCore.mjs`, 516, pure)

- `KEY_SETUP_KEYS` has 10 entries: google-maps (client-exposed), google-maps-server, openai, aisstream, firms, tomtom, cesium-ion (client-exposed), opensky (id plus secret), mapillary (client-exposed) and launch-library.
- Each entry has a tier (metered or free), the capability it `unlocks`, and a `getUrl`.
- `keySetupRequirement(id)` produces the "Needs X — add it in Provider Settings" text reused on layer rows and map chips.

**"POWER UP" flow** (`legacy/src/keySetup.js`, 593; dev server only)

1. `GET /api/setup/status` returns presence and source only, never values. If the request fails (production build or a LAN visitor), the chip and dialog are removed.
2. The bottom-right chip reads "POWER UP · N KEYS WAITING". The dialog has one row per key: tier dot, capability, link and paste field. Externally managed keys (shell or Keychain) are shown read-only. The dialog also has a voice-auth toggle: API key or ChatGPT/Codex OAuth.
3. SAVE sends `POST /api/setup/keys` with `{ENV_VAR: value | null}`. Validation (`validateKeySetupUpdates`) requires:
   - names from the allowlist;
   - at most 16 entries;
   - values of at most 512 characters, printable ASCII, no spaces, and none of `# " ' $ \` or backtick.
4. `upsertDotenvValues` writes the repo-root `.env`, or Pinokio's `ENVIRONMENT`.
   - The file is chmod 0600 *before* any secret is written. On macOS ACLs are stripped with `/bin/chmod -N`; on Windows the user SID is checked.
   - The server then sets `process.env`, restarts Vite, and the page reloads.

**Admission gate** (`admitKeySetupRequest`, `keySetupCore.mjs:239-337`). A request is refused if any of these holds:

- it carries proxy or forwarding headers;
- any sharing or tunnel mode is active;
- the socket is not loopback;
- the Host header is not local;
- it is a POST without an exact same-origin `Origin`;
- the content type is not JSON.

Responses are `no-store` and must not be framed. **This gate is the most carefully engineered security code in the shell; preserve its rules.**

**First run** (`legacy/src/firstRunExperience.js`, 642; `legacy/src/app/startupChrome.js`)

- **Loader.** It stays up for at least 1 s and until the initial share restore settles. Then it fades, and the launcher appears.
- **When it is shown:**
  - never for share links or embeds;
  - `?welcome=0` / `?welcome=1` override it;
  - durable suppression is written only by "Don't show this again"; any other close sets only the session flag.
- **The four missions:**
  - LIVE CONTACTS (the Contacts context mode);
  - SPACE MISSIONS;
  - ENVIRONMENTAL (earthquakes plus FIRMS, then fly to the globe);
  - EXPLORE MANUALLY.

  A failed mission keeps the launcher open for retry.
- **Copy** is marketing ("It feels like a forbidden cockpit…", `welcome.html`).
- **Known defect** (`firstRunExperience.js:96-109`): there is no KEY REQUIRED terminal state, so a keyless FIRMS reads as "LOAD FAILED".

### 1.7 Share links and URL state

There are three mechanisms. Scene file sharing is covered in 4.9.

**Live hash** (`legacy/src/sharelink.js`, 707; `ShareLinkManager`)

- A `URLSearchParams` hash, rewritten through `history.replaceState` after a 500 ms debounce on every camera change (`sharelink.js:516-547`). It is **not compressed**.
- Fields:

  | Group | Keys |
  |---|---|
  | Version | `v=2` |
  | Camera | `lat`/`lon` (4 dp), `alt`, `heading`, `pitch`, `roll` |
  | Style | `style` (normal / crt / nvg / flir / anime / noir / snow), plus shader parameter tokens `sp` |
  | Effects | `bloom`, `bi`, `bv`; `sharpen`, `si`; `hud`, `hv` |
  | Detection | `dm`, `dd`, `da`, `kf`, `ko` |
  | Celestial ring | `cr` |
  | Scope | `sc`, `scf`, `sce` |
  | Map stack | `map` |
  | Layers | `l`: dot-joined layer tokens from `LAYER_STATE_REGISTRY` (`legacy/src/data/layerState.js:569`), max 4,096 chars |
  | Layer options | `lo`: traffic road mode, weather opacity and product, street-level `sinceDays`, radio filter and volume, tracked flight, military or satellite id; max 512 chars |
  | Panels | `ui`: open and pinned state |
  | Timestamp | `at`: written only by "copy link" |

- **Tracked follows expire** relative to `at`: flights after 90 s, military after 45 s, satellites after 300 s (`layerState.js:456-479`). A shared live target is a hand-off, not a bookmark. That is a good, honest rule.
- **Not captured:**
  - data time (no data-time model exists);
  - imagery date or selection;
  - annotations, which travel only through the embed `an` parameter (`legacy/src/view/index.js:84-86,325-340`) and disappear on the first camera move because the manager rebuilds the hash from scratch;
  - the selected entity, beyond tracking ids.

**Restoration** (`legacy/src/ui/shareRestoration.js`, 306)

- It parses the hash *before* panel chrome initialises, so a shared view ignores the recipient's stored panel preferences.
- It reserves a deferred navigation generation, and flies only if no newer user or voice navigation has won.
- It hands layers to `LayerStateCoordinator` and reports expired follows.
- It holds URL writes until the restore settles, then emits `gev:initial-share-restore-settled`.
- `copyLink()` shows the toast "Link copied!" or "Copy failed".

### 1.8 Error surfaces and loading feedback

- **Toast** (`legacy/src/ui/shellFeedback.js:197-205`). A single slot that replaces itself, with a fixed 2 s lifetime. It has no severity, no queue, no dismiss and no history.
- **Global loading chip** (`legacy/src/loadingFeedback.js`, 484). A *pure reducer* over normalised layer records:
  - 160 ms reveal delay; 2.2 s terminal dwell (5 s on failure); a "long" state at 30 s.
  - Labels: LOADING LIVE DATA, LOAD COMPLETE, LOAD CANCELLED, LOAD FAILED, plus retry states.
  - Guidance statuses (zoom in, empty, idle) are never treated as errors.
  - It carries persistent or timed notices.
  - Its tickers (60 ms and 100 ms) pause while the tab is hidden.
- **Traffic sync chip** ("syncing road network NN%") and **CCTV sync chip** ("loading frames n/m").
- **Layer rows** (`legacy/src/ui/layerPanel.js`, 660) show the feed state: ON, LOADING, DEGRADED, STALE, PARTIAL, FALLBACK or UNAVAILABLE. They also show counts and "key required" tooltips. **This is the most honest status surface in the app.**
- **Inline errors:** `#sl-error role=alert`, `.panel-surface-error`, the voice control's `data-status=error`, and the status lines in first run and key setup.
- **No skeletons or progress bars**, apart from the scene progress fill. Status chip text is animated by split-flap (section 2).
- **FPS monitor** (`frameRateMonitor.js`, 81). It counts `scene.postRender` events over 1 s windows.

### 1.9 Design tokens, fonts and colours today

**Default tokens** (`legacy/src/ui/styles/foundation.css:6-33`)

| Group | Tokens |
|---|---|
| Surfaces | `--bg-dark #0a0a0f`; `--glass-bg rgba(12,12,20,.72)`; `--glass-border rgba(255,255,255,.08)`, hover `.15`; `--menu-bg #12121c` |
| Accent | `--accent #00d4ff` (neon cyan); `--accent-dim rgba(0,212,255,.15)`; `--accent-glow rgba(0,212,255,.4)` |
| Text | `--text-primary #e8eaed`; `--text-secondary` at 50% alpha; `--text-dim` at 30% alpha |
| Type | `--font-mono 'JetBrains Mono','SF Mono','Fira Code',monospace`; `--font-sans 'Inter',-apple-system,BlinkMacSystemFont,sans-serif` |
| Shape and motion | `--panel-radius 16px`; `--btn-radius 10px`; `--transition-fast 150ms` / `--transition-smooth 300ms`, both `cubic-bezier(.4,0,.2,1)` |
| Layout | `--left-stack-x 52px`, `--right-rail-x 52px`, `--left-stack-top 26vh`, `--left-stack-gap 72px`, `--left-collapsed-width 176px`, `--display-panel-expanded-width 272px`, cockpit radio widths 148 and 232 px |
| Panel surface | `--panel-surface-{font,detail,frame,shadow,title,error(#ff9b8e),width(300px),collapsed-width}` |

**Cyber theme** (`legacy/src/ui/styles/cyber.css:84-131`, set on `:root[data-ui-theme='cyber']`)

- Colours: `--cyber-red #c45454`, `--cyber-red-bright #e16666`, `--cyber-slate #26343a`, `--cyber-slate-raised #334a53`, `--cyber-grid rgba(188,211,218,.1)`, surface `rgba(18,24,27,.92)`.
- Shape: 1 px radii, a 12-point chamfered `--cyber-panel-frame` `clip-path`, and a `body::after` full-screen grid.
- It switches the body font to mono and sets `--accent #e9f0f1`.

**Token discipline is weak:**

- Raw colours dominate: `rgba(0,212,255,…)` ×113, `rgba(54,220,255)` ×44, `#22e6e6` ×34, `rgba(196,84,84)` ×52, plus amber cautions `#ffc46b` and `#d5a863`.
- There are no spacing or z-index tokens.
- 85 `!important`.
- About 116 mono font declarations against 16 sans. The UI is uppercase, letter-spaced monospace almost everywhere.

**Aesthetic.** Dark frosted glass (80 `backdrop-filter: blur(16–28px) saturate(1.4)`), a neon-cyan accent with 38 `text-shadow` glows, and amber cautions.

For Lavender, keep only the *idea* of a dark neutral surface and a monospace face for numerals and coordinates. Replace everything else with a disciplined token set (section 1.10).

### 1.10 Shell recommendation for Lavender

1. **A fixed CSS-grid workspace with no floating chrome over the map:**
   - top bar: search, basemap, share, settings;
   - left sidebar: layers and workspaces;
   - right inspector: selection, imagery, weather, context;
   - bottom status bar: cursor coordinates in DD/DMS/MGRS, view-centre coordinates, altitude, scale, UTC, feed health, notification count.

   This replaces the measure-the-obstacles rail engine.
2. **One typed store with selectors**, persisted through a single versioned schema (`lavender:v1:*` locally; workspaces in Rust). The URL becomes a serialisable *subset* of it. Port the `stateChannel` invariants: frozen snapshots, revisions, isolated listeners.
3. **One command registry** (section 3.9) with scopes for map, panel, modal and text. It drives shortcuts, a command palette, menus and a `?` help overlay. Escape becomes a stack, not ad-hoc listeners.
4. **Sober tokens:**
   - neutral greys and a single accent;
   - semantic status colours: ok, stale, degraded, error, key-required, approximate;
   - a 4/8 spacing scale, 4–6 px radii, z-index tokens;
   - self-hosted Inter and JetBrains Mono (tabular numerals), with an icon subset;
   - WCAG AA contrast;
   - no glows, blur or scanlines;
   - reduced motion honoured by default.
5. **Port as patterns:** lifecycle and `UiLifetime`, the owner model, two-phase shutdown, restore lanes, navigation generations, the loading reducer, the feed-state vocabulary, and the key-setup gates.

---

## 2. Visual effects and display modes: full inventory

### 2.1 Key findings

1. **The six styles are cosmetic colour remaps of the rendered frame, labelled as sensors.**
   - The NVG "timestamp" is seconds since the app started, drawn as if it were a clock (`legacy/src/styles/surveillance.js:103`; `time` is `wallNow() - startTime`, `visualEffects.js:181`).
   - FLIR invents temperatures and a frame counter (2.3).
2. **The animated styles force continuous rendering.** CRT, NVG, FLIR and Snow use a `time` uniform, so `VisualEffects` holds `style-anim` (`visualEffects.js:197-207`). The whole scene plus the shader then renders at 60 fps, which is exactly the GPU cost `renderGovernor.js` was written to remove.
3. **The styles destroy information, and other code was written to compensate:**
   - `legacy/src/data/trafficPresetStyle.js` (150) and `transitPresetStyle.js` (145) restyle point symbols per style.
   - An "IR boost" turns fog off and reloads every aircraft model as flat white, unlit "hot" targets (`visualSettings.js:362-405`, `layers/flights/state.js:424-445`, `layers/military/state.js:337-345`). This fabricates heat signatures.
   - `layers/transit/selection.js:124-140` remaps clicks to undo the NVG barrel distortion. **No other layer does**, so clicks can land on the wrong object in NVG.
4. **Three circular masks stack by default:**
   - the scope mask darkens about 49% of a 16:9 viewport;
   - labels outside the circle fade to 1% opacity;
   - NVG and FLIR add their own tube masks.

   Together they hide data in the screen corners. The scope mask's own header comment admits it "emerged from six zero-intensity style PostProcessStages" and was later "made real" as branding (`scopeMask.js:6-12`).
5. **The app does have real thermal-adjacent data**, just not in the style system: NASA FIRMS fire detections and NOAA infrared cloud imagery (`legacy/src/tools/queries/atmosphere.js:85-91`). An honest Lavender feature points users there.

### 2.2 Inventory and verdicts

Cost legend:

- **P**: a full-screen GPU pass.
- **C**: forces continuous rendering.
- **rAF**: a JS animation loop.
- **2D**: a canvas-2D redraw.

| # | Effect (code / UI name) | What it does | Files (LOC) | Cost | Toggle / default | Verdict and reasoning |
|---|---|---|---|---|---|---|
| 1 | `normal` | No filter | — | — | key 1 | **Keep** |
| 2 | `retro` / "CRT" | Barrel distortion, line jitter, chromatic fringing, pixel grid, dithering, posterise to 10 levels, RGB stripe mask, scrolling scanlines, ghosting, ~30 Hz flicker (`sin(time*188.5)`), random glitch bars every ~0.5 s, warm tint, vignette | `legacy/src/styles/retro.js` (178) | P (~6 reads/px + 64-entry array), **C** | key 2; params pixelation, distortion, instability | **Remove.** Pure decoration that degrades legibility. The flicker and glitch bars are not gated by reduced motion, which is a photosensitivity risk |
| 3 | `surveillance` / "NVG" | Lens distortion, round tube mask, pixelation, luminance → green phosphor, 11×11 bloom, grain, honeycomb texture, scanlines; a fake HUD drawn in the shader (crosshair, label blocks, fake timestamp, blinking REC dot) | `styles/surveillance.js` (279) | P (~122 reads/px; ~450 M reads per frame at 2560×1440), **C** | key 3; params gain, bloom, scanlineStr, pixelation | **Remove.** Not night vision. Its distortion misaligns picking. Nothing in it can be salvaged |
| 4 | `thermal` / "FLIR" | Tube mask, pixelation, 5×5 blur, luminance treated as "temperature" in a window around 0.5, 12-step contours, white-hot / black-hot, Ironbow palette *linearly blended* by a slider, 9×9 hot-spot bloom, drifting noise; fake HUD with crosshair, **fabricated °C** (`thermal.js:263`), fake frame counter (`:301`) and scale bar | `styles/thermal.js` (340) | P (~108 reads/px + noise), **C** | key 4; params sensitivity, bloom, mode, pixelation, palette | **Redesign** as an honest static "False colour (luminance)" imagery filter (2.8). The single-channel stretch, perceptual palette and optional contours have analytic value for reading relief and texture. Label it "derived from RGB brightness, not thermal", and remove the fake temperature, noise, blur, mask and bloom |
| 5 | `anime` | Cel-shade luminance banding, saturation boost, edge outlines, warm shift | `styles/anime.js` (70) | P (5 reads), static | key 5 | **Remove.** Keep at most the edge-detection kernel as an optional "edge emphasis" adjustment |
| 6 | `noir` | Greyscale, contrast, static grain, vignette, 15% sepia | `styles/noir.js` (57) | P (1 read), static | key 6 | **Redesign** as "Monochrome basemap" (greyscale plus contrast; no grain, vignette or sepia). A grey basemap is a standard GIS technique for making coloured data stand out |
| 7 | `snow` | Five parallax layers of fake falling snowflakes, frost, cool tint, fog | `styles/snow.js` (93) | P (~90 hashes/px), **C** | key 7 | **Remove.** It invents weather. Real snow belongs in the weather layers |
| 8 | Style crossfade | 500 ms eased fade between styles | `visualEffects.js:170-210`, `visualPresets.js:10` | rAF plus render hold | every style change | **Remove.** Switch instantly |
| 9 | Sharpen | 3×3 unsharp mask, `amount = 0.1 + 2v`; produces halos | `visualPresets.js:121-145`, `visualEffects.js:101-168` | P (9 reads), every frame | **on, 49**; share `sharpen` / `si` | **Redesign.** Off by default, under "Imagery adjustments". Analysts should see unaltered pixels by default |
| 10 | Bloom | Cesium bloom re-tuned, 0–200 slider with old-scale migration | `legacy/src/bloom.js` (89), `visualEffects.js:77-153` | Multi-pass P | off; share `bloom` / `bi` / `bv` | **Remove.** Glow with no information |
| 11 | IR boost | In NVG/FLIR: fog off, aircraft models reloaded as unlit white, `gev:vision-change` event | `visualSettings.js:362-405,1692-1700`, flights and military `state.js` | Model reloads | automatic | **Remove.** Fabricates heat signatures |
| 12 | Cockpit vision modes | optical / crt / nvg / thermal / anime / noir / snow inside cockpit; saves and restores stage intensities | `legacy/src/cockpitVisionPolicy.js` (67), `ui/cockpitInstruments.js:217-256` | as above | cockpit vision button | **Remove.** Any surviving imagery adjustment applies globally, not per mode |
| 13 | Scope mask | Fixed canvas: transparent circle of radius 0.525·height, near-black outside; 11% feathered edge; opacity 0.94 above 10,000 km, fully opaque below 7,000 km | `legacy/src/scopeMask.js` (572) | 2D; redrawn on resize, DPR change or opacity step (~12 per zoom); cheap | **on**; "Scope" button, Feather slider; share `sc` / `scf` / `sce` | **Remove.** A branding frame that hides about half the screen |
| 14 | Label fade outside the circle | Labels fade over 7% of the radius to 1% opacity outside | `legacy/src/celestialRing.js:11-120` | per-label maths | Fade / Outside sliders; share `kf` / `ko` | **Remove** together with #13 |
| 15 | Celestial ring | At globe zoom: ring around the globe, sun and moon direction markers, glowing sun rays and moon haze on two rotating canvases, 2.4 s automatic camera flight | `celestialRing.js` (844), `foundation.css:197-272` | postRender capped at 30 fps; 60 s timer | off; "Celestial" button; Normal style only; share `cr` | **Redesign** as a day/night terminator plus a sub-solar and sub-lunar point layer. The sun and moon positions are real and useful for imagery and illumination reasoning. Drop the rays, glow and auto-flight |
| 16 | "Detection" overlay | Canvas labels on aircraft, ships, satellites, vehicles, cameras etc., with a density and declutter engine and an accessible list. Decoration: corner brackets, scrolling full-screen scanlines, screen blend plus `contrast() saturate() drop-shadow()` glow on a full-viewport canvas, focus ring, 220 ms fade-in, per-style themes | `legacy/src/data/detection.js` (1,663), `detectionDraw.js` (267), `detectionRenderDemand.js` (196), `legacy/src/overlays/worldOverlayTokens.js` (228) | 2D every rendered frame + full-viewport CSS filter pass. **Measured 34–41 fps vs 60** (2.4) | **on, DENSE 75%**; key D; share `dm` / `dd` / `da` | **Redesign** and rename "Labels" (nothing is being detected). Keep the label decluttering and density allocation and the off-screen accessible list. Drop the brackets, scanlines, glow, blend mode and per-style themes |
| 17 | Intelligence HUD | Corner telemetry. See 2.9 field by field | `legacy/src/hud.js` (950), `hudLayouts.js` (64), `hudLayoutPolicy.js` (7), `overlays.css` | timers at 1 s, 800 ms, 250 ms and 15 s; 24 ms typewriter | **on**, `tactical`; key H; auto-shown with CRT, NVG and FLIR | **Redesign** as a plain status bar. Delete the classification banners, random ids, REC, typewriter, GSD/NIIRS and BAND/BITS/LVL |
| 18 | "Cyber" theme | `data-ui-theme=cyber`: red and slate reskin, chamfered `clip-path` panels, full-screen grid on top of everything, offset text shadows, voice "scan" and "lock" animations, eight looping cockpit-instrument animations. Also forces FLIR plus Ironbow at 0.42 | `legacy/src/ui/styles/cyber.css` (2,837), `hudLayouts.js:11-64` | Compositor work; grid always on top | HUD Layout = Cyber | **Remove.** At most borrow the slate neutrals and compact density into Lavender's one design system |
| 19 | Cyber sonar | Rotating radar wedge (5.2 s per sweep) with rings, plus a hook that **rewrites Cesium point and billboard draw commands** so contacts outside the wedge dim and tint | `legacy/src/cyberSonar.js` (562), `cyberSonarGpu.js` (295), `cyberSonarScene.js` (211), `ui/cyberSonarControls.js` (86), `cyber.css:1447-1532` | Replaces `scene.primitives.update` and works only on Cesium 1.138.0; requests a render every 80 ms | Cyber only; on | **Remove.** A fake radar sweep that changes data visibility on a timer, coupled to Cesium internals |
| 20 | Cockpit clouds | Weather-driven FBM ray-marched cloudscape on a separate low-resolution WebGL canvas. Real cloud cover drives a generic, unlocated cloudscape | `legacy/src/cockpitCloudEffects.js` (614), `weatherEffectsMath.js` (87) | rAF at 60, draws at 12 fps; extra WebGL context | off; localStorage | **Remove.** Show cloud cover, visibility and wind as numbers. Rain, snow, fog and haze values are computed but never used |
| 21 | Cockpit decoration | Visor SVG (targeting rings, "VISOR LOCK · ACTIVE", "OPTICAL PLANE · 01"), a **static** roll arc and pitch rails (a fake attitude indicator, since there is no attitude data), nudging altitude and speed pointers, blinking live dot, rolling digits | `legacy/src/ui/styles/cockpit.css` (2,189), `ui/templates/cockpit.html` (328) | CSS loops | cockpit | **Remove.** See 2.10 |
| 22 | TR-3B reclassify | Cockpit chip "Reclassify as TR-3B" swaps the tracked aircraft for a conspiracy black-triangle model | `ui/cockpitTrackingController.js:35-47` | model swap | cockpit | **Remove.** Fabricates the identity of a real aircraft |
| 23 | Logo gaze | The logo's eye or globe follows the pointer | `legacy/src/logoGaze.js` (181) | rAF while easing | always (off under reduced motion) | **Remove** |
| 24 | Split-flap text | Departure-board flip animation on status chips | `legacy/src/splitFlap.js` (546), `status.css:300-385` | CSS + one timer per change | `SPLIT_FLAP_ENABLED = true` (`splitFlap.js:62`) | **Remove.** Use instant text. Keep the technique of a stable text node for `aria-live` |
| 25 | Startup fly-in | Starts at 25 km nadir, waits 500 ms, then a 4 s cubic flight to Austin at 600 m and −30° | `legacy/src/camera.js:52-82`, `app/controls.js:42-46` | camera flight | every load without a share link | **Remove.** Open instantly on the last-used or workspace view |
| 26 | Auto-orbit | Camera circles the last POI at 6°/s | `legacy/src/orbit.js` (94) | holds continuous render | key O | **Remove** (it also duplicates the verb orbit, 5.7) |
| 27 | Route dolly | "Fly route": banked turns up to 10°, ±20 m altitude "breathing", +26 m lift into turns, a gaze that leads by 6.5 s | `cameraVerbs.js:1451+` | holds continuous render | voice; Directions FLY chip | **Redesign** as a constant-speed, level, scrubbable route preview. Keep the floor and clearance safety logic |
| 28 | Cinematic scene recipes | "Global Flights Radar" (CRT, bloom 62), "Orbital Watch" (NVG), "Thermal Threat Board" (FLIR), "City Overload", "Omniscience Pullback" | `legacy/src/scenes/recipes.js` (984), `visualSettings.js:1402` | as the styles | Scenes panel, voice `control_scene` | **Remove** (section 4) |
| 29 | Celestial / globe auto-flights; intercity "world jump" | 2.4 s auto-flight to frame the ring; 5.2 s intercity jump that suspends overlays | `celestialRing.js`, `ui/locationNavigation.js` | flights | automatic | **Remove** the automatic flights; keep short, user-initiated transitions only |
| 30 | HUD typewriter and REC blink | 2 chars per 24 ms summary reveal; 800 ms REC blink | `hud.js:674` | timers | with the HUD | **Remove** |
| 31 | CCTV projection, viewshed volumes | Live camera frames projected into the 3D city; coverage volumes | layer code (`layers/cctv`), UI `legacy/src/ui/cctv*.js` (919) | texture projection | CCTV panel | **Keep / Defer.** It has real analytic value (where a camera can see), but it is a layer feature with policy questions (5.10) |
| 32 | Imagery A/B split, recent imagery | Swipe comparison of two imagery sources; day cards | `legacy/src/ui/imagerySplit.js` (176), `recentImagery.js` (1,083), `imageryBoxTool.js` (293), `legacy/src/maps/imageryComparison.js` (124) | cheap | Recent Imagery panel | **Keep.** Genuine analytic value |
| 33 | Frame-rate readout | Frames per second in the title bar | `ui/frameRateMonitor.js` (81) | 1 s interval while shown | key `` ` `` | **Keep**, moved to a diagnostics panel |
| 34 | Safe-frame overlay | 16:9 / 9:16 frame with rule-of-thirds guides | `overlays.css:1-65`, `recordingControls.js` | — | recording mode | **Remove.** Nothing is recorded; revisit if report or video export arrives |
| 35 | Shell glows and glass | `.title-glow`, `.panel-glow` (radial gradient, 30 px blur), 38 `text-shadow` glows, 80 `backdrop-filter` blurs, dialog "scanline" bars (`first-run.css:56`, `provider-settings.css:103`) | various CSS | compositor passes while the globe renders | always | **Remove.** Use opaque or near-opaque neutral surfaces |

Supporting plumbing:

- `legacy/src/ui/effects.js` (7) and `visualInput.js` (2) are re-export files.
- `visualPresets.js` (156) holds the preset tables.

Under Lavender none of these modules survives as code. The slider-from-uniforms pattern is fine to reuse for the remaining imagery adjustments.

### 2.3 Notes on specific effects

- **Picking and distortion.** NVG lens distortion (`0.5 * intensity`) and CRT barrel distortion move pixels away from where Cesium picks them. Only transit remaps clicks. Any Lavender post-process must be geometry-preserving.
- **Detection theming.** Each style has its own detection palette, glow, blend mode, filter and scanline opacity (`legacy/src/overlays/worldOverlayTokens.js:107-227`). The full-viewport CSS `drop-shadow` on a screen-blended canvas is a compositor pass on every frame.
- **Coupled defaults.** The Contacts context mode forces detection to the tactical preset (`legacy/src/contactsDetectionPolicy.js`). Military styles force HUD and detection on. Lavender must keep display preferences independent of modes.
- **Marketing claims.** "Reskin reality: GLSL sensor looks" (`README.md:64`), "Military HUD" (`:66`) and "re-renders through a different sensor" (README "First Five Minutes") are inaccurate and must not carry over.

### 2.4 Measured cost

From `legacy/docs/PERFORMANCE.md`: Apple M5, Chrome 150, 1440×900, DPR 1, fps in motion / at rest.

| Scene | fps |
|---|---|
| Normal | 60 / 60 |
| CRT | 60 / 60 |
| NVG | 60 / 60 |
| **FLIR** | **49 / 60** |
| Anime | 60 / 59.8 |
| **Noir** | **47 / 56.6** |
| **Snow** | **42.3 / 45.8** |
| Detection 25% | 39.3 / 41.1 |
| Detection 50% | 37.4 / 39.8 |
| Detection 100% | 34.4 / 35.5 |
| Cockpit | 49.6 / 49.2 |
| Combined operational | 39.9 / 43.1 |

- The combined static scene issued **48,665 text draws** in motion.
- The "detection" overlay, which is *on by default*, is the single largest steady-state cost.
- These numbers come from a fast GPU; integrated laptop GPUs will be far worse, especially with continuous render held.

### 2.5 Render governor: Keep the idea

`legacy/src/renderGovernor.js` (147):

- Owners register named "holds". While any hold exists Cesium renders continuously (`requestRenderMode = false`). With zero holds it switches to on-demand rendering and draws one settling frame.
- It sets `maximumRenderTimeChange = Infinity`.
- `governorRequestRender(reason)` requests a single frame and records the last 16 reasons for diagnostics.
- The viewer itself sets `targetFrameRate = 60`, `msaaSamples: 4` and `preserveDrawingBuffer: true` (`legacy/src/app/viewer.js:123-127`). `preserveDrawingBuffer` exists for viewport screenshots to voice, and it has a cost.

What defeats it:

- animated styles (`style-anim`);
- orbit (`camera-orbit`);
- camera verbs;
- cyber sonar's 80 ms re-requests.

Self-throttled modules:

- celestial ring: 30 fps;
- clouds: 12 fps;
- scope mask: altitude sampled every 120 ms;
- detection: frames requested only during a fade or solve.

**For Lavender:** keep request-render-by-default, named holds with diagnostics, and "no decorative module may hold continuous render". Time-animated data such as moving tracks and timeline playback should request frames at the data cadence.

### 2.6 Motion and accessibility

**`prefers-reduced-motion` is honoured in:**

- JS: split-flap, logo gaze, sonar, cockpit clouds, route-dolly shaping, first run, radio controls, and the cyclone, wind and weather layers;
- CSS: 12 media blocks.

**It is not honoured in:**

- any animated shader, including CRT flicker and glitch bars;
- the style crossfade;
- HUD REC blink and typewriter;
- detection scanlines;
- startup fly-in, orbit and celestial auto-flight;
- `logo-pulse`, the loader `pulse`, `detection-pulse`, `cockpit-live`, `gev-voice-bar`, `cctv-frame-shimmer`, `sl-spin`, `command-dock-reveal`.

**Patterns to keep:**

- the off-screen accessible list of canvas labels (`#world-overlay-actions`);
- the stable text node beneath any animated label;
- `aria-pressed` on display toggles where it exists.

### 2.7 CSS animation verdicts

**Remove:**

- `logo-pulse` and the loader `pulse` (use a determinate progress indicator);
- `detection-pulse`;
- `title-radio-broadcast` (`foundation.css:562`);
- `cockpit-altitude-pointer`, `cockpit-speed-pointer`, `cockpit-live`, `cockpit-number-roll-*`, and all `cockpit-instrument-*` keyframes;
- `gev-cyber-voice-scan`, `gev-cyber-voice-lock`, `cyber-sonar-sweep`;
- `gev-flap-in` / `gev-flap-out`;
- `mission-thrust-wave` (`layers.css:974`);
- dialog "scanline" bars;
- the Cyber grid (`cyber.css:131-160`).

**Keep, short, functional and gated on reduced motion:**

- panel and card entry transitions (`command-dock-reveal`, `cockpit-brief-enter`, `gev-voice-card-in`, `gev-pointer-reticle-in`);
- `sl-spin` and `cctv-frame-shimmer` as loading indicators;
- `first-run-scroll-fade` as a scroll hint;
- `gev-voice-bar`, only if it is driven by real audio level (today it free-runs).

**Keep as affordances:** resize-grip stripes and "unavailable" hatching.

### 2.8 What replaces the styles in Lavender: "Imagery adjustments"

A single optional, **static** post-process stage. It has no `time` uniform, holds no continuous render, never distorts geometry, applies instantly, and shows a persistent "Adjusted imagery" tag in the status bar while active. It is saved in the workspace.

| Adjustment | Derived from |
|---|---|
| Monochrome basemap (greyscale and contrast) | Noir |
| False colour (luminance): window/level or auto-stretch; perceptual palettes chosen from a list, not blended (grey, inverted, inferno, viridis, Ironbow); optional contour lines; mandatory caption "derived from RGB brightness — not thermal", with a link to the real thermal sources (FIRMS, NOAA IR) | FLIR |
| Sharpen, off by default | Sharpen |
| Edge emphasis, optional | Anime's edge kernel only |

Once these exist, the following can all be deleted:

- `trafficPresetStyle`, `transitPresetStyle`;
- IR boost and the TR-3B thermal icon variants;
- the transit pick remap;
- per-style detection themes and HUD colours.

### 2.9 HUD field by field (`legacy/src/hud.js`)

`IntelHUD` (`hud.js:93`) describes itself as an "NRO/NGA satellite aesthetic". It is built with `innerHTML` (`_buildDOM`, `hud.js:184`), polls the camera at 4 Hz while visible, and looks elements up by id on every tick.

| Field | Source | Verdict |
|---|---|---|
| `TOP SECRET // SI-TK // NOFORN`, `PAGE 1/1` | static | **Fake.** Classification theatre |
| `KH11-4xxx`, `OPS-41xx`, `ORB: 47xxx PASS: DESC-1xx` | `Math.random()` (`hud.js:169-172`) | **Fake** |
| `● REC` and UTC timestamp | timers | REC is fake; **UTC is useful** |
| MGRS (10 m) and DMS lat/lon | camera sub-point | Useful, but it is the **camera** position, not the view centre or cursor. Kilometres off at oblique pitch |
| `GSD: x m NIIRS: y` | `alt × 0.000375`, then simplified GIQE (`hud.js:373-376`) | **Misleading.** Ignores FOV, screen and actual imagery source |
| ALT (MSL via a lazy EGM96 geoid, cached per 0.01° cell) | `legacy/src/data/geoid.js` | **Useful**, carefully done |
| `SUN: x° EL` | `_estimateSunElevation` (`hud.js:505`) | Useful idea, weak implementation: day-of-year approximated as `date + 30*month`, no equation of time |
| ONA = 90 + pitch | camera | Useful as "tilt" |
| `COLL: hh:mm:ssZ` | wall clock | **Misleading.** Presented as imagery collection time |
| `BAND: PAN / BITS: 11 / LVL: 1A` | static (`hud.js:241`) | **Fake** |
| Summary line | mode; view band (STREET <1.2 km … GLOBAL); nearest of 45 curated POIs within 150 km (`legacy/src/hudLocality.js`); continent from bounding boxes; footprint `WINDOW w×h KM`; `UTC±n` from `lon/15` (wrong); feed-provenance tag (`legacy/src/hudSummaryResponse.js`) | The view band, footprint and **provenance tag** are good. The time zone is wrong. The continent and POI lookups should be reverse geocoding |
| AI summary | five-word OpenAI summary (`gpt-5-nano`, `legacy/server/providers/openai/constants.js:17`) every 15 s and on `moveEnd`; accepted only if it echoes the non-nominal feed state | **Remove** (cost, novelty). The deterministic summary is enough |
| Corner brackets, `.hud-sonar` sweep, per-shader glow colours | CSS | **Fake/decorative** |

The HUD's *visibility* is coupled to the shader: it appears with CRT, NVG or FLIR (`hud.js:815`). Its layouts are tactical, operator, minimal and cyber (`hudLayoutPolicy.js`).

**Lavender status bar:**

- cursor coordinate in DD, DMS or MGRS;
- view-centre coordinate;
- camera MSL and above-ground altitude;
- heading and pitch;
- scale bar and footprint;
- correct sun azimuth and elevation (Cesium's sun position, or SPA in Rust);
- UTC and the timeline cursor time;
- per-layer data freshness.

`legacy/src/locationStatus.js` (68) is a mini status line for the location panel. Preserve it, minus the emoji.

### 2.10 Cockpit mode

**What it is.** A first-person camera riding a tracked ADS-B aircraft, civil or military.

**Entry** requires all of the following (`contextModePolicy.js:396`, `cockpitEntryAllowed`):

- the Contacts context mode is active;
- both flight layers are on;
- `viewer.trackedEntity` is an aircraft.

**Size.** About 4,300 JS (`legacy/src/cockpit*.js`, `legacy/src/ui/cockpit*.js`), plus 2,189 CSS and 328 HTML.

**Architecture.**

- `CockpitCoordinator` builds `CockpitViewController`. That controller is a god object: its methods are spread across about ten files and invoked with `.call(this)`.
- It hooks `scene.preUpdate` and `trackedEntityChanged`, and talks to the rest of the app through window events.

**Camera** (`ui/cockpitCamera.js`, `ui/cockpitPresentation.js`, `legacy/src/cockpitMath.js`):

- The eye sits 7 m forward and 2.6 m above the aircraft, at a **fixed −4° pitch** with no roll.
- Heading slews toward the ground track at 28°/s.
- Position is dead-reckoned from course and speed, with a bounded correction toward the displayed position (`cockpitAnchorCorrectionStep`, `cockpitMath.js:27`).
- When the feed is stale the camera holds the exact displayed position.
- Ground clamp: at least 12 m above the mesh or ground, probed every 500 ms.
- On entry it **hides the aircraft and disables all mouse input** (`cockpitTrackingController.js:165`), so the user cannot look around.

**Instruments** (`ui/cockpitInstruments.js`):

- callsign, ground speed, altitude, heading tape, UTC, lat/lon;
- feed state: LIVE TRACK, STALE FEED or SURFACE FALLBACK;
- origin and destination with relative bearing.

**Context panel** (`ui/cockpitContext.js`, `cockpitSignals.js`):

- cohort counts within a radius (flights, military, vessels, installations);
- nearest item with distance and nose-relative bearing;
- the explicit uncertainty line `n INPUTS UNKNOWN · NOT AN ALL-CLEAR`;
- a `CONTACT LOST` hold;
- previous/next contact.

**Briefing carousel** (`ui/cockpitBriefing.js`): signals, Google News RSS regional news, and Open-Meteo local weather. It rotates every 9 s.

**Spectacle:** vision modes (#12), clouds (#20), the visor and fake attitude indicator (#21), TR-3B (#22), the CSS loops and the news carousel.

**Analytic value:**

- following a live track smoothly;
- an approximate viewpoint from the track's position (line-of-sight reasoning);
- proximity cohort counts with relative bearings;
- origin and destination;
- explicit stale, lost and unknown states;
- stepping through nearby contacts.

**Verdict: Replace** with a generic **Follow / Viewpoint camera** for any moving track (aircraft, vessel, satellite, vehicle):

- honest labels ("heading derived from course; no attitude data");
- a user-controllable look offset and FOV;
- an optional "look at selected point".

Keep `slewHeading`, `cockpitAnchorCorrectionStep`, `cockpitGroundSafeHeight`, `bearingBetweenCoordinates` and `relativeBearing` as TS utilities. Move proximity analysis to a Rust spatial query that feeds the entity inspector.

---

## 3. Voice control (OpenAI Realtime)

### 3.1 Architecture

**Session establishment** uses WebRTC with a server-minted ephemeral secret.

1. **Mint.** The browser calls `GET /api/realtime/token?tier=standard|mini[&auth=oauth]` (`legacy/src/voice/realtimeBackend.js:18-70`).
   - The server (`legacy/server/providers/openai/realtime.js:130`, `createRealtimeTokenHandler`) builds the **entire session config server-side**: instructions, tools, voice and VAD.
   - It POSTs that config to `https://api.openai.com/v1/realtime/client_secrets` with the server-held key and returns the `ek_…` secret plus echo headers (`X-GEV-Voice-Model`, tier, auth, transcribe model).
   - The client cannot change the tools or instructions.
2. **Connect.** `RealtimeConnection.start` (`legacy/src/voice/realtimeConnection.js:58-240`):
   - calls `getUserMedia` (mono, with echo cancellation, noise suppression and AGC);
   - creates an `RTCPeerConnection` with data channel `oai-events`;
   - POSTs the SDP to `/v1/realtime/calls` with `Bearer ek_…`.

   Start epochs make a superseded attempt release whatever it acquired. ICE `disconnected` gets 6 s of grace.
3. **Config** (`legacy/server/providers/openai/constants.js`):

   | Setting | Value |
   |---|---|
   | Model | `gpt-realtime-2` (standard) or `gpt-realtime-2.1-mini` (mini) |
   | Voice | `marin` |
   | Reasoning | low |
   | Truncation | `retention_ratio` 0.5; `post_instructions` 3,000 tokens |
   | Noise reduction | `near_field` |
   | Transcription | `gpt-4o-mini-transcribe` |
   | Turn detection | `semantic_vad`, eagerness low, `interrupt_response: false` (barge-in is handled client-side) |

   Every setting has an env override.
4. **Prompt weight.** `instructions.js` is about 4.8k tokens. The 30 app-action schemas are about 7.7k tokens, and the voice catalog tools are added on top. That is a static prefix of more than 12.5k tokens before any conversation.
5. **Server hardening.**
   - same-site gating;
   - a 30/min per-IP throttle shared with the HUD summary (`GEV_RATELIMIT_OPENAI_PER_MIN`);
   - a 30 s timeout and `redirect: 'error'`;
   - upstream error bodies are not passed through;
   - an unknown `tier` falls back to `standard` via an own-property check.
6. **ChatGPT/Codex OAuth.**
   - Client: `legacy/src/voice/cloudVoiceAuth.js`. Server: `codex-auth.js`, 360 LOC.
   - The server reads `~/.codex/auth.json` and uses the ChatGPT access token to mint Realtime secrets. It is loopback-only and spawns `codex login`.
   - The README itself says this contract is unconfirmed. There is no cost estimate and no cap in this mode.
7. **Debug log.**
   - `/api/realtime/debug-log` appends sanitised events to `.gev-logs/realtime-conversations.jsonl`.
   - Keys, bearer tokens, `ek_` secrets and image data URLs are redacted.
   - Content fields are stripped unless `GEV_VOICE_LOG_CONTENT=1`.
   - Limits: 8 MB per request, 32 MB rotation, 120 writes/min.

**Module layering** (`legacy/docs/VOICE-OWNERSHIP.md`)

| Layer | Modules |
|---|---|
| Pure policy | `voiceCost.js` (540), `realtimePreferences.js`, `realtimeInputPolicy.js` (500 ms Space hold), `realtimeProtocol.js` (2.5 s call dedupe, per-result instructions), `realtimeEvents.js`, `speech.js` (574), `speechLint.js`, `resultDisplay.js`, `voiceCardPresentation.js` (reducer), `layerManifest.js` (524; 28 layers with aliases, capabilities and typed analyst fields with units), `actionSchemas.js` (974), `deixis.js` (370), `referents.js` |
| Provider-neutral lifetime | `session.js` (184). Adapter contract `start` / `stop` / `sendText` / `sendMapEvent`; event vocabulary `state`, `transcript`, `action-call`, `progress`, `action-result`, `interruption`, `completion`, `turn-metrics`, `pointer`; one AbortController and generation per action |
| Composition | `commands.js` → `sessionCommands.js` → `realtimeSession.js` → `GevRealtimeController` (`realtimeController.js`, 498), which extends the 570-LOC back-compat `RealtimeFacade` |
| Owners | Connection (transport and media), Turns (`realtimeTurns.js`, 1,077: responses, supersede, dedupe, tool execution), Radio (`realtimeRadio.js`, 364), Input (`realtimeInput.js`, 421: push-to-talk and meters), Cost (`realtimeCost.js`, 270), Viewport (`realtimeViewport.js`, 490), Diagnostics, TurnMetrics (`turnMetrics.js`, 314), Narration and earcon |
| Actions | `gevRealtime.js` `initGevVoiceCommands`: `createGevActionRunner` (`gevActions.js`, 5,019) wrapped by `withToolCatalog`. Names in `GEV_ACTION_SCHEMAS` go to the runner; any other name goes to the lazily imported tool catalog |

**Ownership invariants worth keeping:**

- Shutdown order: invalidate work → close transport → stop audio analysis → release media → clear state.
- Late callbacks can never enter a replacement session.
- Every accepted or refused tool call gets **exactly one** terminal output.
- New typed input supersedes old intent.
- The cost meter stays bound to the model the server actually served.

### 3.2 Turn flow (`realtimeTurns.js:220-690`)

1. `speech_started`: barge-in bookkeeping, then a `pointer_context` item.
2. The model emits a function call.
3. Spend-cap gate, then superseded-response refusal, then call-id dedupe (2.5 s).
4. `session.runAction` → `resolveDeicticArgs` → `runGevAction` → `attachVoiceResult` → record referents.
5. `function_call_output` is sent.
6. An optional viewport image or pointer crop is attached.
7. `response.create` with per-result instructions.

### 3.3 Action and tool schema surface

**A. 30 app actions** (`legacy/src/voice/actionSchemas.js`; model-facing descriptions in `legacy/server/providers/openai/toolDescriptions.js`)

**Camera and navigation**

| Action | Parameters and behaviour |
|---|---|
| `fly_to_location` | `locationId` (an 8-city enum), `query`, `latitude`/`longitude`, `viewMode` close/overview, `rangeM`, `waitForArrival`, `referent` |
| `adjust_camera_zoom` | `direction`, `amount` little/medium/lot (factors 1.25 / 1.6 / 2.4) |
| `zoom_to_globe` | No parameters |
| `move_camera` | `motion` orbit/pan/tilt/rotate/stop, `direction`, `speed`, `mode` once/continuous |
| `fly_route` | `label`, `speed` |
| `frame_overhead` | `target` flights/military/satellites/vessels, `radiusKm` |
| `track_entity` | `query` (callsign, hex, MMSI, NORAD, or `'pointer'`), `layerId`, `referent` |
| `stop_tracking` | |
| `select_nearest_aircraft` | Composite: fly, wait, enable the layer, refresh, select the nearest airborne aircraft |

**Layers, panels and modes**

| Action | Parameters |
|---|---|
| `set_layer_visibility` | `layerId` (27-value enum from the manifest), `enabled` |
| `show_data_layers_menu` | |
| `set_panel_open` | `panelId` (10 panels), `open` |
| `set_context_mode` | |
| `control_cockpit` | `action` enter/exit/previous/next/status, `targetLayer`, `aircraftClass` |

**Read-only queries**

| Action | Parameters and behaviour |
|---|---|
| `get_entity_context` | `scope` auto/selected/in_view/pointer, `referent`, `layerId`, `limit`. May trigger a screenshot |
| `get_current_view_state` | |
| `analyst_query` | `layers[]`; `scope{kind: view/region/radius/anywhere/pointer, name, km, center}`; `filters[{field, op: gt/gte/lt/lte/eq/neq/contains, value}]`; `sortBy`; `sortDir`; `limit`; `followUp`. Runs on `legacy/src/data/analystEngine.js` (250k record cap). Returns `count`, `complete`/`partial`, `scopeLabel`, `items`, `feedProvenance`/`feedState` and `countsReconciliation` |
| `next_iss_pass` | |
| `next_satellite_pass` | `target`, lat/lon, `minElevationDeg`, `visibleOnly` |

**Annotation**

| Action | Parameters and behaviour |
|---|---|
| `annotate_map` | 1–24 annotations. Each has `type` pin/highlight/area/arrow/route/label; `target` (or `'pointer'`), lat/lon, or `points[2..12]`; `toTarget`; route `mode`; `label`; `color` (5); `footprint`; `intent`; normalised `screenX`/`screenY`. Plus `flyTo` and `persist` |
| `clear_annotations` | Destructive; guarded only by prompt text |

**Visual and spectacle**

| Action | Parameters |
|---|---|
| `set_visual_style` | |
| `set_hud` | `visible`, `layout` |
| `set_cyber_sonar` | |
| `set_detection` | |
| `set_map_stack` | photoreal/bing-aerial/bing-labels/esri-imagery/osm |
| `set_post_processing` | bloom, sharpen |
| `control_scene` | list/play/stop/next/status |

**Media**

| Action | Behaviour |
|---|---|
| `control_cctv` | 12 actions, including nearest, viewshed and autohop |
| `control_radio` | 11 actions. A successful play **ends the voice session** to hand over the speaker |

**B. Catalog tools on the voice surface.** Of the 40 catalog tools, 25 are on voice (`legacy/src/tools/surfaces.js`):

- brief and awareness: `situation_brief`, `military_awareness`, `get_regional_brief`;
- aircraft and vessels: `get_aircraft_track`, `get_aircraft_info`, `get_vessel_track`;
- places and terrain: `search_places`, `places_nearby`, `plan_route`, `get_map_features`, `get_terrain_height`;
- weather and hazards: `get_weather`, `get_wind`, `get_cyclones`, `get_fire_perimeters`;
- mobility: `get_bike_share`, `get_transit_vehicles`, `get_traffic_flow`;
- infrastructure and finders: `find_submarine_cables`, `find_military_installations`, `find_cctv_cameras`, `find_alpr_cameras`, `find_radio_stations`;
- events: `get_recent_launches`, `get_bhote_koshi_flood`.

The other 15 are MCP-only. Voice answers several of them from loaded layers through `analyst_query`; the image-returning ones are excluded because voice is text-only.

**Dispatch.**

- One long if-chain in `runGevAction` (`gevActions.js:331-1186`). It calls `styleManager.*` (the shell facade), `dataManager` *private* methods (`_setEnabledWithIntent`, `_waitForVisibilityIntent`), camera verbs, locations, the Director, annotations, the analyst engine, the Cesium `viewer` directly, and `window.__godsEyeView`.
- **There is no runtime JSON-Schema validation.** Each handler normalises through alias maps (`PANEL_ALIASES`, `LAYER_ALIASES`, `CITY_ALIASES`…) and throws on unknown values.
- `createActionTools` (`actionSchemas.js:899-974`) merges descriptions into schemas and **throws if a description tries to change an argument**. It also strips top-level `anyOf`, `oneOf` and `allOf`. This is a good guard.
- The runner is already the app's command bus: embed and MCP-panel views and share-link annotations route through it (`legacy/src/app/tools.js:183`).

**Results.**

- Builders in `speech.js:430-470` attach `say` (at most 25 words, numbers verbatim), `display{title, lines, chips, notes, sources}`, `referents` (at most 5), `schedule` and `identityLine`.
- The voice card (`voiceCard.js`, `voiceCardPresentation.js`, `voice-card.css`) shows captions, plan steps, progress, result chips and lines, and numbered referents. It hides after 14 s idle.
- Flaw: the *whole* result, including card-only `display` data, is serialised into `function_call_output`, so it costs model tokens.

### 3.4 Deixis, pointer context and viewport capture

**Pointer tracking** (`pointerContext.js` 366, `pointerPick.js` 61)

- There is no pick per mouse move. A 250 ms dwell within 12 px triggers one pick, and a snapshot reuses it for up to 1 s.
- Snapshot moments: Space keydown, `speech_started`, or a typed command.
- The snapshot is `{entity{layerId, id, label, kind}, ground{lat, lon, heightM}, radiusKm = clamp(0.75 × camera height in km, 5, 400), cameraKey}`.

**What reaches the model**

- With push-to-talk: a `pointer_context` system item of about 60–100 tokens, sent as JSON data rather than prose so that labels cannot inject instructions.
- With open mic or typed input: only `{pointing: kind}`. Coordinates are revealed only when a tool resolves `'pointer'`.

**Sentinel resolution** (`deixis.js`)

- The model passes `'pointer'` or `referent: n`, never ids or coordinates.
- The runner rewrites the arguments for `track_entity`, `fly_to_location`, `get_entity_context`, `analyst_query` and `annotate_map`.
- Failures return sentences the model can speak.
- `referents.js` mirrors the numbered card rows. It is session-scoped and generation-checked.

**Images** (`realtimeViewport.js`, policy in `realtimeProtocol.js:89-100`)

- **Full viewport.** Sent after `get_entity_context` only at local scale (camera below 10 km) *and* when there is no structured identity.
  - JPEG quality 0.74, at most 1200×900 px and 200 KB, `detail: 'high'`.
  - It waits for a fresh frame and skips near-black frames.
- **Pointer crop.** 512×512 around the pointer, only on bare ground.
- At most **one image** is kept in the conversation; older ones are deleted.
- `annotate_map` screen coordinates are remapped from crop to canvas, and refused if the camera has moved since.

**Reticle** (`pointerReticle.js`, 135). A CSS ring at the pointer. It hides when the camera moves, 1.5 s after the reply, or after 10 s at most.

**Privacy**

- Canvas pixels are sent to OpenAI with **no opt-in and no redaction**. That includes annotation labels, CCTV and ALPR markers, and projected camera video.
- Transcripts and pointer coordinates are also sent.
- `get_entity_context` reverse-geocodes the view through Google, then Photon, then Nominatim, which tells third parties where the analyst is looking.

Lavender needs per-workspace opt-in, layer redaction and an audit entry for every capture.

### 3.5 Cost controls

**Rates** (`voiceCost.js`, "verified 2026-08-18"), USD per 1M tokens:

| Model | Text in / out | Audio in / out | Image in |
|---|---|---|---|
| `gpt-realtime-2` | 4 / 24 | 32 / 64 | 5 |
| `gpt-realtime-2.1-mini` | 0.6 / 2.4 | 10 / 20 | 0.8 |

Transcription is billed separately. Unknown models are billed at the most expensive rates.

**Limits** (`VOICE_COST_LIMITS = {warnUsd: 2, capUsd: 5}`, `voiceCost.js:233-235`)

- The warning is a one-shot latch: a console line plus `data-level="warn"` on the chip.
- The cap is a one-shot latch that stops the session and refuses further tool calls.
- Both persist in `localStorage` (`godsEyeView.voiceCost.limits`, `.tier`) and apply at the next session.

**Accounting**

- Usage from `response.done` and transcription usage is summed.
- Missing modality detail and residual tokens are attributed to audio, so the estimate errs high.
- The tracker is bound to the **server-echoed** model id, so an env override cannot under-meter.
- An `incomplete` flag marks a response still in flight at teardown.

**UI.** A STD/MINI toggle and a "~$0.42" chip with ok/warn/cap levels (`voice-cost.css`).

**Telemetry.** `turnMetrics.js` records per-turn spans: end-of-utterance, first text, first audio, max silence, per-tool timing, spoken words and hedge hits (`speechLint.js`, 18 hedge phrases).

**Absent**

- No idle hang-up and no wall-clock session limit.
- No daily or monthly budget.
- **No server-side enforcement.** The $5 cap is trusted to the client, and anyone who can reach the server can mint up to 30 tokens/min/IP.

### 3.6 Speech and narration

- **Narration** (`narration.js`, 270). Earcon after 700 ms, first line at 1.5 s, step gap 3 s, "still working" at 8 s.
  - Order of precedence: the model's preamble, then code-authored progress lines, then the earcon.
  - Unheard lines are retracted when a tool finishes.
- **Earcon** (`earcon.js`, 66). Quiet 880/1320 Hz ticks, played only while the push-to-talk mic is muted.
- **Prompt honesty rules worth keeping:**
  - confirm the *resulting* state;
  - never claim an action without `ok = true`;
  - state counts verbatim with a named scope;
  - no filler.

### 3.7 What is valuable, and verdicts

| Piece | Verdict | Target | Reasoning |
|---|---|---|---|
| `actionSchemas` + description-merge guard + `surfaces.js` + `layerManifest` | **Improve** into one typed **CommandRegistry** with codegen for every surface (palette, keyboard, voice, MCP, LLM agent) | TS shared schema package; Rust mirrors backend commands | The concept already works across voice, embeds and share links |
| `gevActions.js` runner | **Replace** with one handler per command against Lavender store and view APIs | TS | God module with private APIs and globals |
| Result envelope (`say`, `display`, `referents`, `scopeLabel`, `complete`/`partial`, `feedState`) | **Preserve** as the universal command result. Stop sending `display` to the model | TS | "Information over spectacle" in data form |
| `analyst_query` contract | **Preserve the contract, Replace the execution** | Rust query endpoints + TS adapter | Honest counts over the authoritative store, not over whatever is loaded in the browser |
| Deixis, referents, pointer context | **Preserve**, generalised to `'selection'` and list rows | TS | Useful for the palette ("track #2") and agents too |
| `session.js` adapter contract and lifecycle invariants | **Preserve** | TS | |
| Ephemeral token minting | **Preserve the concept, Replace the implementation** | Rust | Mint, enforce budgets, TTL and idle timeout, and keep the usage ledger server-side |
| `voiceCost` estimator and meter | **Preserve** the logic (authoritative in Rust, chip in TS) | Both | Conservative and bound to the served model |
| `turnMetrics`, `speechLint`, debug-log sanitiser | **Preserve** as QA telemetry, content off by default | TS emit, Rust store | |
| Viewport and crop capture | **Improve**: opt-in, redaction, audit | Both | Privacy |
| Visual, HUD, sonar, detection, post-processing, scene, cockpit, radio actions; `frame_overhead`; `fly_route`; `select_nearest_aircraft` | **Remove** | — | Spectacle, or composites an agent can plan itself |
| Codex/ChatGPT OAuth path | **Remove** | — | Unsupported contract; reads another tool's credential file |
| HUD AI summary (`gpt-5-nano` every 15 s) | **Remove** | — | |
| Narration scheduler and earcon | **Defer** until a voice MVP | TS | |
| 4.8k-token disambiguation prompt | **Replace** with fewer, orthogonal tools | Shared | 56 overlapping tools need prompt patches to route correctly |

---

## 4. Director, scenes, data packs and sharing

### 4.1 What it is

**A cinematic camera-tour player for social-media clips.** It is not an analytic timeline.

- `legacy/src/scenes/director.js:1-13` describes itself as a "Deterministic cinematic scene playback engine for social-media clip capture".
- `legacy/src/scenes/recipes.js:1-4` says "Scene recipes optimized for short social clips".
- A *project* holds *scenes*, and a scene is an ordered list of *shots*. A shot is a camera pose or anchor-based move, a flight duration and a hold, visual post-processing (style, bloom, sharpen, HUD variant, detection, scope), a map stack, and layer toggles.
- The built-in recipes (`recipes.js:592-975`) are pure spectacle (see 2.2 #28).
- The only "evidence" content is the hard-coded **Bhote Koshi (Nepal) flood** sequence (4.6).
- Voice drives it through `control_scene`.

### 4.2 Files

| Area | Files | LOC |
|---|---|---:|
| Portable core | `legacy/src/director/**` (document, documentFields, cameraDocument, camera, timeline, clock, playback, authoring, `packs/*`, `interactions/*`, `sharing/*`) | 1,897 |
| App layer | `legacy/src/scenes/**`: director.js 2,483, recipes 984, project 404, sharing 404, nepalEvidencePack 349, dataPacks/presentation 224, shotPresentation 171, scenePolicy 170, interactions 165, playbackAdapter 78, cameraMotion 70 | 5,663 |
| UI | `legacy/src/ui/sceneControls.js` 242, `scenePresentation.js` 156, `sceneSharing.js` 137, `shareRestoration.js` 306, `railTimeline.js` 200, `recordingControls.js` 97 | 1,138 |
| Nepal-specific (coupled, outside scope) | `legacy/src/data/bhoteKoshi*`, `legacy/src/ui/styles/bhote-koshi.css` 613 | ~7,300 |

Tests are extensive (`director.test.mjs` 1,586 lines plus the packs, sharing, clock and timeline suites). They are a useful behavioural spec for cancellation, seek and stale imports.

### 4.3 Scene document schema

- **Version.** `SCENE_DOCUMENT_VERSION = 6` (`legacy/src/director/document.js:19`).
  - Versions 1–5 and unversioned files migrate on import.
  - Doc drift: `legacy/docs/SCENE-DOCUMENT.md:3-10` still names version 5 as the write format.
- **Root.** `version`, `scenes[]`, `createdAt`, `updatedAt`, `installedBuiltInSceneIds`.
- **Scene.**
  - `id`, `title`, `shots[]`, `releaseLayerIds`, `appliedShotPacks`;
  - v4 `anchors[]`: up to 1,024 of `{id, title, lat, lon, alt, altitudeReference: "ellipsoid"}`;
  - v5 `dataPacks[]`: up to 8.
- **Shot.**
  - `id`, `title`, `durationSec`, `holdSec`;
  - `camera`: an inline pose, or `anchorId` plus heading/pitch/roll;
  - `visual`, `layers`;
  - v4 `move{from, easing: linear | cubic-in-out}`;
  - v5 `dataPackIds`;
  - v6 `interactions[]`: up to 64.
- **`visual` is nearly all spectacle:** `style`, `mapStack`, `styleParams`, `bloom`, `sharpen`, `hud`, `detection`, `scope`.
- **Validation is excellent and portable** (`legacy/src/director/documentFields.js:2-11`, `cameraDocument.js`):
  - limits of 5 MiB, 256 scenes, 10k shots, depth 24, 200k nodes and 64 KiB strings;
  - unknown fields rejected with field paths;
  - prototype keys refused;
  - finite numeric bounds;
  - an invalid import leaves current state untouched;
  - an unreadable saved project is never overwritten (`director.js:258-342`).
- **Storage.** `localStorage` only, under `godsEyeView.sceneProject.v2`.

### 4.4 Clock and timeline

- **The Director never touches the Cesium clock.** There are no references to `viewer.clock`, `JulianDate` or `shouldAnimate`.
  - `legacy/src/director/clock.js` (301) is a wall-clock ticker over *authored seconds*, with generation counters and cancellable holds.
  - Time-dynamic data is **not** replayed: layers are toggled live during playback.
- `legacy/src/director/timeline.js` (110) is clean and pure:
  - `sceneTimingForShot`;
  - `sceneSeekState`;
  - `cameraAtProgress`: a cubic ease with shortest-arc heading/pitch/roll.
- `legacy/src/director/playback.js` (137) is a portable phase runner: `selectShot → applyVisual → applyLayers → travel → settle → hold → completeShot`, with token cancellation between phases.
- `legacy/src/ui/railTimeline.js` (200) is **unrelated to the Director**.
  - It is a discrete observation scrubber: previous/next, latest, play, a 150 ms coalesced drag commit, and UTC ticks.
  - Only the Weather panel uses it, for radar and satellite frames.
  - **It is the closest existing UI to Lavender's timeline.**

### 4.5 Camera shots and motion

- `camera.flyTo` with cubic-in-out easing. Cesium auto-raises the arc, which produces swoops; the Nepal opening shot goes from 15,628 km down to about 6 km.
- Explicit v4 moves interpolate lat/lon/alt/HPR (`legacy/src/scenes/cameraMotion.js`, `director/camera.js`). Heights are ellipsoidal only and there is no terrain avoidance.
- Interaction "focus" is an instant top-down pose.
- Camera ownership arbitration is solid: pointer or wheel input, navigation hand-off and cockpit all cancel playback.
- **Flourishes to drop:** global-to-street pull-backs, orbital swoops, shaders, bloom, HUD, detection, scope, safe frames, beat reveals, animated evidence paths and autoplaying media.

### 4.6 Data packs and the Nepal evidence pack

**Data packs** (`legacy/src/director/packs/*`, `legacy/src/scenes/dataPacks/*`, `legacy/docs/DIRECTOR-DATA-PACKS.md`)

- **Manifest:** `{id, version: 1, format: geojson | image | media, source{adapter: "assets" | "scene-bundle", path}, attribution{text, license, url?}, placement, byteLength?, sha256?}`.
- **Limits:**

  | Limit | Value |
  |---|---|
  | Packs per scene | 8 |
  | Size per asset | 8 MiB |
  | Size per shot | 32 MiB |
  | Features | 2,000 |
  | Positions | 50k |
  | PNG dimensions | ≤ 4096² |
  | Load deadline | 15 s |

- **Network hygiene:** confined relative paths, no credentials, no redirects, no-store, idempotent `dispose()`.
- GeoJSON styling properties are ignored, and media never autoplays.
- **The provenance, integrity and attribution model is the best-designed part of the Director.**

**Nepal / Bhote Koshi pack** (`legacy/src/scenes/nepalEvidencePack.js`, `recipes.js:8-590`, `legacy/public/events/bhote-koshi-2026/`)

- A narrative of a 26 Aug 2026 outburst flood.
- It contains:
  - pre- and post-event satellite crops;
  - a reconstructed flood centreline;
  - witness anchors and field reports;
  - 16 "evidence spine" records linking public posts.
- Per-beat choreography is hard-coded.
- **It is licensed CC BY-NC 4.0, so it cannot ship commercially.** Capture times are noted as unverified.
- The reusable generic piece is the before/after swipe, which `imagerySplit.js` and `maps/imageryComparison.js` already share with Recent Imagery.

### 4.7 Interactions (v6)

- Clicking a GeoJSON feature offers four kinds of button:
  - `card`: plain text plus an HTTPS link, no HTML;
  - `focus`: an anchor;
  - `shot`: jump to a shot;
  - `layer`: toggle a layer.
- Bounded at 64 per shot and 4 KiB of text, user-initiated only, and accessible (Tab, Enter, `aria-live`).
- The plain-text **source card** is a good primitive for citations.

### 4.8 Recording

**There is no video capture.** There is no `MediaRecorder`, `captureStream` or `getDisplayMedia` anywhere.

- "Recording" mode only hides panels and draws safe frames for an external screen recorder.
- `director.downloadLastRunMetadata()` exports a `scene-run-*.json` run telemetry file.

### 4.9 Scene sharing (third mechanism)

- **Formats** (`legacy/src/director/sharing/*`, `legacy/src/scenes/sharing.js`, `legacy/docs/DIRECTOR-SHARING.md`):
  - a single scene as JSON;
  - a `.gevbundle.json` of `{format: "gev-scene-bundle", version: 1, project, assets: [{path, mimeType, base64, sha256}]}`.
- **The bundle is uncompressed base64.** Limits: 50 MiB total, 64 files, 8 MiB per file, 32 MiB of assets.
- **Import** goes through a preview dialog that shows missing sources and layers and verifies hashes.
- **Bundle bytes live only in session memory.** They must be reimported after a reload.

The live URL hash and restoration are covered in 1.7.

### 4.10 Mapping to Lavender capabilities

**Timeline playback.** The Director provides nothing here. Lavender needs:

1. A workspace time model: a UTC cursor, a window, and live versus historical mode.
2. Rust endpoints that serve time-windowed history: aircraft and vessel tracks, quakes, fires and imagery dates. The tool catalog already has `get_aircraft_track` and `get_vessel_track` as seeds.
3. A frontend that drives Cesium `clock.currentTime` and time-dynamic entities (`SampledPositionProperty`).
4. A scrubber generalised from `railTimeline.js`: UTC cursor, step, latest, play.

From the Director keep only the cancellation and generation discipline of `clock.js` and `playback.js`.

**Saved workspaces.** Evolve the scene document into a **Workspace document**, keeping its validation, versioning and migration rules (`deny_unknown_fields` with serde/schemars in Rust; TS types generated from it). Content:

- camera and saved viewpoints (from the anchors);
- layers and filters;
- time cursor and window;
- annotations **with resolved geometry**;
- selection;
- basemap and imagery choice (including before/after pairs);
- imagery adjustments;
- notes.

Drop shots, the `visual` block, recipes and packs-as-code. Persist in Rust; `localStorage` is at most an offline draft cache.

**Reports.** An ordered list of figures. Each figure holds:

- a view: camera, layers, time and annotations;
- a plain-text caption (as in `card`);
- source citations (from the pack attribution model);
- a data-snapshot hash or timestamp.

Render server-side to HTML or PDF from static snapshots. A non-cinematic "step through figures" presentation can reuse the playback-runner pattern with short, neutral transitions.

**Share links.** Replace the shader-heavy hash with two things:

- server-stored workspace or report ids (short links);
- a minimal versioned deep-link hash (camera, layers, time, selection).

Keep the "newer user action wins" restore rule and the follow-expiry honesty rule.

**User data packs.** Keep the manifest concept (licence, attribution, sha256, limits) as **user-imported overlay layers**, stored content-addressed in Rust. Defer an "event / evidence pack" schema covering geolocated source, capture time and verification status.

---

## 5. Annotations, tools, MCP, SDR, camera verbs and locations

### 5.1 Annotations (`legacy/src/annotations/**`, 6,202 LOC)

**Files**

| File | LOC | Role |
|---|---:|---|
| `annotationEngine.js` | 1,597 | Model and lifecycle |
| `resolver.js` | 2,117 | Name → geometry |
| `screenAnnotationRenderer.js` | 881 | SVG overlay |
| `drawTool.js` | 546 | Manual drawing |
| `worldAnnotationRenderer.js` | 539 | Cesium entities |
| `drawMode.js` | 303 | Pure geometry |
| `hybridAnnotationRenderer.js` | 162 | Active renderer |
| `index.js`, `annotationResolver.js` | 57 | Entry points |

The engine is wired in `legacy/src/app/tools.js:44-57`.

**Types**

| Type | Notes |
|---|---|
| `pin` | |
| `highlight` | The default |
| `label` | |
| `area` | Polygon or compound, with holes and multipolygons |
| `route` | Street-following via `/api/route` (OSRM), with distance and duration and an honest `fallback` flag |
| `arrow` | From → to, with distance; this is the measurement primitive |

Colours: primary, amber, cyan, green and red.

**Model** (`buildAnnotation`, `annotationEngine.js:865-950`)

- World-anchored lon/lat/height.
- Fields: `ring`, `polygons`, `footprintKind`, `buildingHeight`, `pendingOutline` / `outlineUnavailable`, `targetKey` / `intentKey` for dedupe, and `viewport`.
- A **`synthesized`** flag marks an approximate buffered disc, drawn dashed. This is an honesty flag worth keeping.

**Lifecycle**

- TTL of 22 s with a fade unless `persist: true`.
- At most 120 live marks, with dedupe.
- Progressive outline upgrade: the point appears first and the polygon arrives later, with retry back-off (8 s, 25 s), Overpass rate-limit handling and 2 concurrent upgrades.
- A camera assist frames off-screen marks.

**Persistence: none.** Marks are in memory only and use session-local ids (`anno-${seq}`). They are not in the scene document or the share hash.

**Resolver.** Turns place names into geometry (`resolveAnnotationTarget`, `resolver.js:142`). Sources:

- bundled US Census and Natural Earth admin boundaries;
- neighbourhood polygons;
- Google Places text search through a proxy;
- Nominatim;
- Overpass, for building footprints, street corridors, smallest enclosing area, monuments and admin areas.

Ranking heuristics are tuned to field tests in Austin and San Francisco. Positive caches never expire; negative caches last 60 s. `resolveRegionRingForQuery` (`:2025`) also feeds `analyst_query` region scopes.

**Renderers**

- **World** (`worldAnnotationRenderer.js`): Cesium entities draped on 3D tiles (`classificationType: CESIUM_3D_TILE`), with extruded building volumes and pulse fades.
- **Screen** (`screenAnnotationRenderer.js`): an SVG overlay reprojected every frame with horizon occlusion. Its "hand-drawn whiteboard" style uses pulsing reticles, bowed arrows and draw-on animation.
- **Hybrid** is the active renderer: areas in world space, labels and everything else in screen space.

**Draw tool**

- `drawMode.js` is pure, with no Cesium or DOM:
  - area, line and pin, with minimum vertices 3/2/1 and a maximum of 512;
  - duplicate clicks within 0.5 m are rejected;
  - great-circle length, spherical area and dateline unwrap;
  - live measurement text;
  - it produces the same spec as voice, flagged `manual: true`.
- `drawTool.js` (DISPLAY ▸ Draw):
  - claims pointer ownership (`legacy/src/data/inputOwnership.js`);
  - previews over the surface height;
  - Enter or double-click to finish, Backspace to undo a vertex, Esc to cancel;
  - a label prompt on finish;
  - clean `destroy()`.
- **Missing:** vertex editing, moving a mark, deleting a single mark, snapping.

**Leftovers.** The globals `window.__gevAnnotations` and `window.__CESIUM__`, and a hard-coded San Francisco `demo()` / `tour()` (`annotationEngine.js:1131-1180`).

### 5.2 Tools catalog (`legacy/src/tools/**`, 5,994 LOC)

**Definition.** `defineTool({name, kind: query | action, title, description, inputSchema, annotations, requires[], ui, run})` (`legacy/src/tools/catalog.js`).

- `run(args, {services, signal, tools})` returns `{summary, data, images?}`.
- A `ToolError` taxonomy: `invalid_arguments`, `unavailable`, `unsupported`, `malformed`, `retry_later`.

**Schema** (`legacy/src/tools/schema.js`). A JSON-Schema subset, with unsupported keywords rejected at definition time and **full runtime validation**. The voice actions lack this.

**Composition.** `composeCatalog({tools, services, replace, interceptors})`:

- tools whose `requires` services are absent are omitted;
- interceptors wrap every call;
- composite tools call other tools.

**Shared argument shapes**

| Shape | Contents |
|---|---|
| `AREA_SCHEMA` | Exactly one of `place`, an antimeridian-aware `bbox`, or `lat/lon/radius_km` |
| `LIMIT_SCHEMA` | Default 25, max 200; results report total, returned and truncated |
| `VIEW_ARGUMENTS` | For deep links |

Services (`services.js`) fetch the app's own `/api/*` routes over HTTP.

**The 40 tools** (`legacy/src/tools/index.js:86-127`; V = on voice, M = on MCP)

| Tool | Purpose / source | V | M | Lavender |
|---|---|---|---|---|
| `get_earthquakes` | USGS M2.5+, 24 h | – | ✓ | Preserve |
| `get_active_fires` | NASA FIRMS | – | ✓ | Preserve |
| `get_recent_launches` | Launch Library 2 | ✓ | ✓ | Defer |
| `aircraft_in_area`, `find_aircraft` | adsb.lol / OpenSky | – | ✓ | Preserve |
| `get_aircraft_track` | Track ≤200 points | ✓ | ✓ | Preserve (timeline seed) |
| `get_aircraft_info` | adsbdb | ✓ | ✓ | Preserve |
| `vessels_in_area`, `find_vessel` | AISStream | – | ✓ | Preserve |
| `get_vessel_track` | | ✓ | ✓ | Preserve (timeline seed) |
| `next_satellite_pass`, `satellites_overhead` | CelesTrak TLE + SGP4 | – | ✓ | Preserve (Rust `sgp4`) |
| `find_cctv_cameras`, `get_cctv_snapshot` | city CCTV | ✓/– | ✓ | Defer (policy) |
| `find_alpr_cameras` | OSM / DeFlock tiles | ✓ | ✓ | Remove or Defer (policy) |
| `find_radio_stations` | Radio Browser | ✓ | – | Remove |
| `search_places`, `places_nearby` | Google Places proxy | ✓ | – | Improve (geocoder service) / Defer |
| `plan_route` | OSRM | ✓ | – | Defer |
| `get_bike_share`, `get_transit_vehicles`, `get_traffic_flow` | GBFS / GTFS-RT / TomTom | ✓ | –/–/✓ | Remove / Defer / Defer |
| `get_weather`, `get_weather_map`, `get_wind` | Open-Meteo / NOAA / GFS-IFS | ✓/–/✓ | –/✓/– | Improve |
| `get_recent_imagery` | Landsat / Sentinel-2 HLS via NASA CMR + GIBS | – | ✓ | **Preserve** (core GEOINT) |
| `find_submarine_cables` | Bundled TeleGeography (CC BY-NC-SA) | ✓ | ✓ | Replace the data (licence) |
| `find_infrastructure` | Bundled OSM datacenters and dams (ODbL) | – | ✓ | Preserve |
| `get_bhote_koshi_flood` | Bundled Nepal event (CC BY-NC) | ✓ | ✓ | Remove |
| `get_regional_brief` | Nominatim, Open-Meteo, GDELT, Google News | ✓ | – | Improve |
| `get_cyclones`, `get_fire_perimeters` | NHC/CPHC, WFIGS | ✓ | ✓ | Preserve |
| `get_terrain_height` | ≤20 points → ground, geoid and ellipsoid heights | ✓ | ✓ | Preserve |
| `find_military_installations`, `get_map_features` | OSM / Overpass | ✓ | ✓ | Preserve |
| `situation_brief` | Composite: weather, quakes, fires, aircraft, ships, cyclones | ✓ | ✓ | Preserve (report seed) |
| `military_awareness` | Composite, radius ≤250 km | ✓ | ✓ | Improve |
| `get_hud_caption` | LLM caption | – | – | Remove |
| `show_in_gods_eye_view` | View link plus MCP Apps panel | – | ✓ | Replace with "open in workspace" |
| `panel_request` | App-only HTTP tunnel for the panel | – | app | Remove or Defer |

### 5.3 The "tools panel"

There is **no in-app tools panel**. The "panel" is an MCP Apps view:

- `ui://gods-eye-view/globe` is served as `text/html;profile=mcp-app` (`legacy/src/tools/globePanel.js`, runtime `legacy/src/app/globePanelRuntime.js`).
- It is a separate embed build running inside the chat host.
- It tunnels every app HTTP request through `panel_request`. The tunnel has 512 KiB parts, gzip, a 64 MiB cap and a **deny-list** of paths (`legacy/src/tools/queries/panelRequest.js:27-45`), not an allow-list.
- The panel key is admittedly not access control.

**Verdict: Defer.** It is fragile and heavy for v1.

### 5.4 MCP exposure

- **Protocol** (`legacy/src/tools/mcp/protocol.js`, 217):
  - methods `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`;
  - protocol versions 2025-11-25, 2025-06-18 and 2025-03-26;
  - no prompts or subscriptions.
- **stdio transport** (`legacy/server/mcp/stdio.js`): newline-delimited JSON-RPC. It honours `notifications/cancelled` and logs method and tool names, never arguments.
- **HTTP transport** (`legacy/src/tools/mcp/http.js` mounted at `/mcp`): stateless Streamable HTTP, JSON only, a 1 MiB body cap and a 30 s timeout.
- **Auth: none.** `isLocalMcpRequest` only checks:
  - a loopback remote address;
  - no proxy headers;
  - sharing off;
  - a loopback Host on the right port (a DNS-rebinding guard);
  - `Origin` equal to `Host`.
- The MCP server calls the running app over HTTP (`--api-base http://localhost:4173`).
- `legacy/docs/MCP_SETUP.md` covers Claude Desktop, Codex and ChatGPT desktop.
- The top-level `legacy/tools/` directory is **not MCP**. It holds offline CLI scripts: Google tile stitching (`sat-ortho.mjs`), street-view and pano utilities, and a puppeteer render. **Remove**: bulk Google tile stitching likely violates the provider's terms.

**Verdict: Replace.** Run MCP inside the Rust backend (for example the `rmcp` crate) with:

- real authentication (a bearer token even locally);
- per-tool audit logging;
- rate limits;
- an allow-listed tool set;
- tools that call internal services directly.

An optional stdio shim can remain.

### 5.5 Two registries, one future

- Voice app actions (30, client-side, if-chain dispatch) and catalog tools (40, schema-validated, server-capable) overlap.
  - `next_satellite_pass` exists in both.
  - `analyst_query` overlaps `aircraft_in_area`, `vessels_in_area`, `find_*`, earthquakes, fires and satellites, which are switched off on voice for that reason.
- The catalog's `kind: 'action'` is defined but unused.

**Lavender: one registry** (Rust as source of truth via schemars, with generated TS types) with `kind: query | action`:

- **queries** execute server-side;
- **actions** are typed client commands that the frontend executes: `fly_to`, `set_layers`, `select`, `annotate`, `set_time`, `save_workspace`.

MCP, voice and LLM agents, the command palette and keyboard shortcuts all consume it. Preserve from the catalog:

- the surface split;
- `requires` gating;
- interceptors;
- the `ToolError` taxonomy;
- the `{summary, data, view}` result shape, merged with the voice result envelope.

### 5.6 SDR radio (WebUSB RTL-SDR) and internet radio

**Local SDR** (`legacy/src/sdr/**`, 2,620; UI `legacy/src/ui/localSdr*.js`, 443)

| File | LOC | Role |
|---|---:|---|
| `controller.js` | 1,367 | Uses `@jtarrio/webrtlsdr` `RTL2832U_Provider` over WebUSB. Modes: FM 87.5–108 MHz and ADS-B 1090 MHz; 32,768-sample reads; 5 s stall teardown; bounded close |
| `adsbDecoder.js` | 594 | In-browser Mode-S / ADS-B decoder |
| `sdr.worker.js` | 168 | Demodulation worker |
| `sdrAudioWorklet.js` | 48 | Audio output |
| `spectrum.js` | 147 | FM seek by directional peak |
| `gain.js` | 93 | Per-mode gain, persisted |
| `usbDevices.js` | 203 | Remembered device |

**Local ADS-B layer** (`legacy/docs/LOCAL-RECEIVERS.md`, `legacy/src/layers/localAdsb/*`). It merges two inputs by ICAO address:

- the browser SDR;
- **decoder feeds**: the `aircraft.json` from dump1090-fa, readsb or tar1090 (1090 MHz), or dump978 / skyaware978 (978 MHz UAT).

The newest fix wins, and the card names every band and source heard in the last 60 s.

**Limitations.**

- Chrome and Edge only (WebUSB).
- One tuner is shared between FM and ADS-B.
- A dongle owned by a decoder cannot also be opened by the browser.

**Verdict.**

- **Replace** with Rust-side ingest of decoder feeds (`aircraft.json` polling, plus Beast/SBS sockets). This gives first-party, low-latency ADS-B, which is valuable for a professional deployment.
- **Remove** browser FM audio and the in-browser decoder.
- **Defer** direct RTL-SDR access, which belongs in a Rust daemon if it is ever needed.

**Internet radio** (`legacy/src/ui/radio*.js`, ~1,570; `radio.css` 1,039)

- Radio Browser stations drawn as globe pins, with an analog tuner needle that flies the globe to each broadcaster. Voice hands the speaker over to Radio.
- **Remove.** Entertainment, not intelligence.

### 5.7 Camera verbs and camera stack

**`legacy/src/cameraVerbs.js` (1,550)**

- Its header calls it "the spy satellite simulator feel".
- Module-level singleton state drives one motion at a time on `viewer.clock.onTick` with wall-clock dt.
- Any canvas pointer-down or wheel interrupts it (`:1221`).

| Verb | Behaviour | Classification |
|---|---|---|
| `orbit` left/right | `lookAt` around the screen-centre target at 2 / 6 / 15 °/s; "once" turns 30° in 0.9 s; waits for an in-flight flight to settle; refuses while tracking | Continuous orbit is flourish. A bounded orbit helps 3D parallax inspection. **Improve** |
| `pan` | 0.08 / 0.2 / 0.45 view-heights per second; once = 0.25 | **Preserve** |
| `tilt` | 4 / 10 / 20 °/s; once = 15°; pitch clamped to [−89°, −5°] with an honest refusal at the limit | **Preserve** |
| `rotate` | Same rates; once = 15° | **Preserve** |
| `stop` | Clears `lookAtTransform` unless tracking; levels roll | **Preserve** |
| `adjustOrbitRange` | Zoom during an orbit scales its radius, clamped to 80 m–5,000 km | **Preserve** |
| `flyRoute` (`:1451`) | "Cinematic dolly" (2.2 #27). Safety: ≥90 m clearance, 1.2 s arming wait for floor data, ≤10 m/s descent, terrain warm-up, mesh probe | **Replace** with a level, constant-speed, scrubbable preview that reuses the safety logic |

**Rest of the camera stack**

| File | LOC | What it does | Verdict |
|---|---:|---|---|
| `legacy/src/camera.js` | 82 | `CAMERA_PRESETS` (duplicates locations) and the `flyToAustin` intro | **Remove** |
| `legacy/src/orbit.js` | 94 | Duplicate orbit on the `O` key | **Remove** |
| `legacy/src/cameraGroundGuard.js` | 198 | After an arrival, samples ground height under the target and eye (8 tries at 600 ms); lifts with a 0.9 s flight if clearance is under 120 m; cancels on input. Detects take-over by **monkey-patching** `camera.flyTo/setView/lookAt/cancelFlight` | **Improve**: keep the behaviour, implement inside a single camera-owner service |
| `legacy/src/navigationPolicy.js` + `legacy/src/ui/navigationController.js` | 141 + 271 | Navigation generations. Explicit navigation stamps a generation, releases follow owners and flies. Deferred navigation (e.g. after a geocode) stamps first and re-checks. Cockpit refuses with a toast. Hand-off is announced as `gev:navigation-authority-taken` | **Preserve the concept** as a typed camera-owner/lease store |
| `legacy/src/ui/cameraOrientationControls.js` | 379 | `pickViewTarget` falls back pick → globe → ellipsoid. Tilt toggles −35° oblique and −89° nadir; north-up rotates about the view centre; both 650 ms and keep a tracked entity framed | **Preserve** |
| `legacy/src/worldFocus.js` | 96 | Per-kind click-to-focus framing (vessel 1,200 m range at −30°; fire 3,000 m at −35°); 1.9 s `flyToBoundingSphere` with ECEF validity checks | **Preserve**, generalised to all entity kinds |

**Fly durations seen:**

| Action | Duration |
|---|---|
| Landmark / search | 3.0 s |
| Globe reset | 2.8 s to 18,000 km |
| CCTV / world focus | 1.9 s |
| Intercity jump | 5.2 s |

Lavender should standardise on short (≤1.5 s), user-initiated transitions, with an instant option under reduced motion.

**Cross-cutting problem.** At least six competing camera owners exist:

- the verb tick;
- `OrbitController`;
- the cockpit `preUpdate`;
- the ground guard;
- the orientation animator;
- Director motions.

Lavender needs **one `CameraController`** with an explicit owner/lease model. The legacy generation stamps are the seed.

### 5.8 Locations and search

**`legacy/src/locations.js` (1,550)**

- `CITY_POIS` (`:26`) holds 9 cities × 5 POIs = **45 curated presets**: Austin, San Francisco, New York, Tokyo, London, Paris, Dubai, Washington DC and Tallinn.
- Shape:
  - city: `{name, groundElevation, viewBounds, pois[]}`;
  - POI: `{name, lat, lon, alt (range m), pitch, heading, buildingHeight, buildingBounds?}`.
- The presets feed the city pills, the Q–T keys, an offline preset geocoder, the HUD's nearest-POI line, and the voice 8-city enum.

**Framing logic (valuable; port to TS)**

| Function | What it does |
|---|---|
| `geocodeNavigationMode` (`:1013`) | Maps place types to framing modes: region overview, city overview, neighbourhood close, street corridor, area overview, precise place |
| `placeFramingViewport` | Sanity gate. "Tokyo" geocodes to the prefecture box, which would land in the Pacific; this falls back to a metro box |
| `regionFramingPlan` (`:1240`) | Frames natural regions along their long axis |
| `resolveBuildingBounds` | Fetches the building outline from Overpass and sizes framing from its bounding sphere |

Ground floor is resolved before framing, and `searchAndFlyTo` (`:813`) handles cancellation.

**Search chain** (`legacy/src/search/*`, 869)

1. Coordinate parser.
2. Presets.
3. An "Alps" special case.
4. Google Geocoding. **The key is put in the browser URL.**
5. Photon (6 s timeout, 64-entry cache).
6. Local `/api/geocode` Nominatim.

`placeSearch.js` has a 12 s budget and caches hits for 5 min and misses for 30 s. The coordinate parser accepts **decimal degrees only**: no DMS and no MGRS, even though the HUD displays MGRS.

**Verdict**

- **Replace** the curated presets with user bookmarks and saved viewpoints (Rust).
- **Preserve** the framing algorithms (TS).
- **Move** geocoding, Overpass outlines and reverse geocoding to Rust, so keys stay server-side and caches are shared.
- **Improve** the parser to accept DMS and MGRS.
- **Remove** `alpsGeocoder.js`.

### 5.9 CCTV focus, context modes and map stack

**CCTV focus** (`legacy/src/cctvFocusPolicy.js` 81, `cctvFocusRequest.js` 78)

- Enabling the CCTV layer **auto-flies to the nearest camera** unless something else owns the camera.
- A world click dispatches `gev:cctv-request-focus`.
- **Replace** with one generic focus-request bus shared with `worldFocus`. **Remove** the surprise auto-flight.

**Contacts detection policy** (`legacy/src/contactsDetectionPolicy.js`, 184)

- Contacts mode forces detection to Dense 75%.
- **Remove** together with the decorative overlay.

**Context modes.** Section 1.3. **Replace** with explicit, named layer presets or workspaces stored in Rust, with no hidden coupling to the camera, detection, panels or cockpit.

**Map stack**

- `legacy/src/mapStackChips.js` (165) offers five basemaps: Google photoreal 3D, Bing Aerial, Bing Labels, Esri imagery and OSM. Unavailable chips state the **real reason**, with an "ION" badge where relevant. **Preserve.**
- `legacy/src/mapStackController.js` (32) reads `window.__GOOGLE_MAPS_API_KEY__`. **Improve**: mint or proxy tokens through Rust.

**Attribution.** `legacy/src/creditKeyboard.js` (74) adds keyboard access to the attribution lightbox. **Preserve**: attribution is a licensing requirement.

### 5.10 Other panels (brief)

| Panel | Files (LOC) | What it does | Verdict | Target |
|---|---|---|---|---|
| Recent imagery | `legacy/src/ui/recentImagery.js` (1,083), `imagerySplit.js` (176), `imageryBoxTool.js` (293), `legacy/src/maps/imageryComparison.js` (124) | Draw a box to get day cards (Sentinel-2 / Landsat 8–9 at 30 m, daily overview at 250 m) with cloud and coverage reasons. Modes IMAGE / VS BASEMAP / A–B with an accessible split slider; opacity; export | **Preserve + Improve.** Highest GEOINT value in the UI. Move STAC/CMR search to Rust | Both |
| Weather | `legacy/src/ui/weatherPanel.js` (296) + `railTimeline.js` | Observed vs forecast, UTC and age stamps, history timeline; radar is US-only | **Preserve** the semantics; generalise the timeline | Both |
| Street level | `legacy/src/ui/streetLevelControls.js` (637), `streetLevelPresentation.js` (163) | Mapillary provider chips, panorama filter, "since" slider, coverage, image cones, FOLLOW, true modal expand | **Improve** (dock it in the inspector rather than floating it) | Both |
| CCTV | `legacy/src/ui/cctv*.js` (919) | Feeds, nearest camera, calibration gizmo, provenance badge (CALIBRATED / CURATED / RAW PRIOR), viewshed | **Defer** behind a policy review. Keep the provenance badge idea | Both |
| Clear layers | `legacy/src/ui/clearLayersControl.js` (23) | One-click clear | **Preserve** with undo | TS |
| Recording | `legacy/src/ui/recordingControls.js` (97) | Safe frames only | **Remove** | — |

---

## 6. Feature table

The **Target** column says where the capability should live: TS = frontend component or module; Rust = backend; Both = both.

| Feature | Files (`legacy/…`) | Current limitations | Category | Reasoning | Target |
|---|---|---|---|---|---|
| App lifecycle and disposal | `src/app/application.js`, `src/ui/uiLifetime.js`, `src/app/stateChannel.js` | Untyped; the state channel is under-used | **Preserve** (pattern) | Phased start/stop, AbortSignal, frozen snapshots and isolated listeners are sound | TS |
| Shell orchestrator and facade | `src/ui/applicationShell.js`, `shellFacade.js`, `composition.js`, `shellElements.js` | God object; 250+ forwarding members; state held in the DOM | **Replace** | A typed store, feature modules and DI make it unnecessary | TS |
| Ownership rules and two-phase shutdown | `docs/UI-OWNERSHIP.md` | Enforced only by convention and tests | **Preserve** (pattern) | Prevents late callbacks stealing the camera | TS |
| Rail layout engine | `src/ui/panelLayoutController.js`, `leftPanelRail.js`, `rightPanelRail.js`, `panelRailGeometry.js`, `panelMeasurement.js`, `src/panelStackLayout.js`, `src/rightRailPolicy.js` | Measures DOM obstacles; races transitions; tied to the HUD | **Replace** | A CSS-grid workspace (sidebars, inspector, status bar) removes the problem | TS |
| Panel chrome, floating and resize | `src/ui/panelChrome.js`, `panelPositionControls.js`, `panelDisclosure.js`, `panelResize.js` | Two persistence key versions; hover trays | **Replace** (keep the disclosure, Escape and refocus semantics and the pure `resizeBox`) | Docked panes first; floating only where needed | TS |
| Rail cards, row list, chips | `src/ui/railCards.js`, `railCardBlocks.js`, `rowList.js`, `chipGroup.js` | Hand-rolled keyed reconciliation | **Improve** | Become framework components with the same block vocabulary | TS |
| Layer panel and feed-state vocabulary | `src/ui/layerPanel.js`, `layerBindings.js` | Hard-coded groups; KEY REQUIRED folded into failure | **Preserve** (vocabulary, single-flight toggles, key hints) | Most honest status surface; drive it from a backend capability registry | Both |
| Map-stack chips | `src/mapStackChips.js`, `src/ui/mapSourceControls.js`, `src/mapStackController.js` | Google key in a window global | **Preserve** / **Improve** | Honest unavailable reasons; proxy tokens server-side | Both |
| Keyboard shortcuts | `src/ui/applicationShortcuts.js`, `surfaceKeyboard.js`, ~15 ad-hoc listeners | No registry or help; C conflicts with cockpit; Escape order-dependent | **Replace** | Scoped keymap from the command registry, palette, `?` overlay, Escape stack | TS |
| Command / action registry | `src/voice/actionSchemas.js`, `src/tools/catalog.js`, `schema.js`, `surfaces.js`, `src/voice/layerManifest.js` | Two registries; voice actions not runtime-validated | **Improve** (unify) | One schema source for palette, keys, voice, MCP and agents | Both |
| Voice action runner | `src/voice/gevActions.js` | 5k-LOC if-chain; private APIs; globals | **Replace** | One handler per command on Lavender APIs | TS |
| Command result envelope | `src/voice/speech.js`, `resultDisplay.js`, `voiceCard*.js` | Display data sent to the model | **Preserve** | Universal, honest result presentation | TS |
| Analyst query | `src/data/analystEngine.js` (contract in `actionSchemas.js`) | Runs over browser-loaded records only | **Preserve contract**, **Replace execution** | Authoritative counts with provenance | Rust + TS adapter |
| Deixis and referents | `src/voice/deixis.js`, `referents.js`, `pointerContext.js`, `pointerPick.js`, `pointerReticle.js` | Voice-only | **Preserve** (generalise) | "This", "#2" and "selection" for every surface | TS |
| Voice session (Realtime) | `src/voice/realtime*.js`, `session.js`, `server/providers/openai/*` | Client-only cost cap; no idle timeout; OAuth hack; 56 tools | **Defer** (then rebuild as a thin adapter) | Not core to v1; build the registry first | Both |
| Voice cost metering | `src/voice/voiceCost.js`, `realtimeCost.js`, `turnMetrics.js` | Not server-enforced | **Preserve** (logic) | Conservative and model-bound; authoritative ledger in Rust | Both |
| Viewport capture to an LLM | `src/voice/realtimeViewport.js`, `realtimeProtocol.js` | No opt-in or redaction | **Improve** | Privacy and audit | Both |
| HUD | `src/hud.js`, `hudLayouts.js`, `hudLayoutPolicy.js`, `hudLocality.js`, `hudSummaryResponse.js` | Fabricated fields; tied to shaders; camera position, not cursor | **Replace** with a status bar | Keep the geoid MSL, footprint, view band and provenance tag | TS (sun / reverse geocode in Rust) |
| Visual styles (CRT, NVG, Anime, Snow) | `src/styles/retro.js`, `surveillance.js`, `anime.js`, `snow.js` | Fake sensors; continuous render; break picking | **Remove** | Spectacle | — |
| False-colour and monochrome imagery | `src/styles/thermal.js`, `noir.js` | Fake temperature; noise; masks | **Replace** with "Imagery adjustments" | Honest static analytic filters | TS |
| Bloom, sharpen, crossfade | `src/bloom.js`, `src/ui/visualEffects.js`, `visualPresets.js` | Sharpen on by default | **Remove** (bloom, crossfade); **Improve** (sharpen off by default) | Unaltered imagery by default | TS |
| Scope mask, label fade, celestial ring | `src/scopeMask.js`, `src/celestialRing.js` | Hide data; auto-flight | **Remove** (scope, fade); **Replace** celestial with a terminator and sub-solar layer | Illumination is analytic; the frame is not | TS |
| Detection overlay | `src/data/detection.js`, `detectionDraw.js`, `src/overlays/worldOverlay*.js` | Decorative brackets and scanlines; largest frame cost | **Improve** → "Labels" | Keep declutter, density and the accessible list | TS |
| Cyber theme and sonar | `src/ui/styles/cyber.css`, `src/cyberSonar*.js`, `src/ui/cyberSonarControls.js` | Patches Cesium internals; fake sweep | **Remove** | Spectacle | — |
| Logo gaze, split-flap, startup fly-in, auto-orbit | `src/logoGaze.js`, `src/splitFlap.js`, `src/camera.js`, `src/orbit.js` | Decorative motion | **Remove** | Spectacle | — |
| Cockpit view | `src/cockpit*.js`, `src/ui/cockpit*.js`, `src/ui/styles/cockpit.css`, `src/ui/templates/cockpit.html`, `src/cockpitCloudEffects.js` | Fixed pitch; no free look; tied to two layers and Contacts; TR-3B; clouds | **Replace** with a generic Follow/Viewpoint camera | Keep the smoothing, ground clamp, proximity and stale/lost honesty | TS (+ Rust proximity) |
| Camera authority and generations | `src/navigationPolicy.js`, `src/ui/navigationController.js` | Window events; six competing owners | **Improve** | One `CameraController` with leases | TS |
| Camera verbs | `src/cameraVerbs.js` | Singleton; route dolly flourishes | **Improve** (pan, tilt, rotate, bounded orbit, stop) / **Replace** (route preview) | Useful nudges for keyboard and agents | TS |
| Ground guard | `src/cameraGroundGuard.js` | Monkey-patches the camera | **Improve** | Keeps the camera out of terrain and buildings | TS |
| Tilt and north-up controls | `src/ui/cameraOrientationControls.js` | — | **Preserve** | Standard GIS affordance that respects tracking | TS |
| Click-to-focus framing | `src/worldFocus.js`, `src/cctvFocus*.js` | Per-kind constants; CCTV auto-fly | **Preserve** (generalise) / **Remove** (auto-fly) | Predictable framing per entity kind | TS |
| Location presets | `src/locations.js` `CITY_POIS`, `src/camera.js` | 45 hard-coded POIs | **Replace** with bookmarks and saved viewpoints | User data, not demo data | Rust + TS |
| Place-framing algorithms | `src/locations.js:813-1240` | Coupled to the module | **Preserve** | Solves real geocode-box pathologies | TS |
| Geocoder chain | `src/search/*`, `src/keylessGeocoder.js`, `src/nominatimGeocode.js` | Google key in the browser; no DMS or MGRS | **Replace** (backend chain) + **Improve** (parser) | Keys server-side; shared cache | Rust + TS parser |
| Context modes | `src/contextModePolicy.js`, `src/ui/context*.js` | Complex transactions; hidden coupling | **Replace** with named layer presets / workspaces | Explicit beats implicit | Both |
| First-run experience | `src/firstRunExperience.js`, `src/app/startupChrome.js`, `welcome.html` | Marketing copy; missions bound to context modes | **Improve** | A quiet workspace picker; keep the show policy (never for links or embeds) | TS |
| Provider key setup | `src/keySetup.js`, `src/keySetupCore.mjs`, `server/standalone/key-setup-hardening.mjs` | Writes `.env` and restarts the dev server; three keys exposed to the client | **Replace** mechanism, **Preserve** gates | Backend secret store with hot reload; never echo values | Rust (+ TS settings UI) |
| Loading feedback | `src/loadingFeedback.js`, `src/ui/shellFeedback.js` | Single toast; no severity; no KEY REQUIRED state | **Improve** | Keep the reducer; add a notification log, severity and key-required state | TS |
| Share links (live hash) | `src/sharelink.js`, `src/data/layerState.js` | Uncompressed; shader-dominated; no time or annotations | **Replace** | Server workspace ids plus a minimal versioned deep link | Both |
| Share restoration | `src/ui/shareRestoration.js` | Coupled to legacy navigation | **Improve** | Keep "newer user action wins", restore lanes and follow expiry | TS |
| Embed / postMessage view API | `src/app/embed.js`, `src/view/index.js` | No origin allowlist | **Defer** | If kept: allowlist and `frame-ancestors` | TS |
| Director / scene player | `src/scenes/director.js`, `recipes.js`, `src/director/*` | Clip-capture tool; wall clock; spectacle fields | **Remove** (player, recipes) | Doesn't match Lavender's needs | — |
| Scene document validation | `src/director/document.js`, `documentFields.js`, `cameraDocument.js` | `localStorage`-bound | **Preserve** (pattern) → Workspace doc | Versioning, migration, refuse-unknown, keep unreadable saves | Rust + generated TS |
| Timeline scrubber | `src/ui/railTimeline.js` | Weather-only, discrete ticks | **Improve** → workspace time slider | Closest existing UI to timeline playback | TS |
| Data-pack manifest | `src/director/packs/*`, `docs/DIRECTOR-DATA-PACKS.md` | Static assets or session-only bundles | **Preserve** (concept) as user overlays | Provenance, licence and integrity matter | Rust store + TS renderer |
| Interaction "card" | `src/director/interactions/*` | Scene-bound | **Improve** → source-citation element | Supports reports | TS |
| Scene bundles | `src/director/sharing/*`, `src/scenes/sharing.js` | Uncompressed base64; session-only | **Defer** → server upload or zip export | Persistence belongs in the backend | Rust |
| Nepal evidence pack | `src/scenes/nepalEvidencePack.js`, `src/data/bhoteKoshi*`, `public/events/bhote-koshi-2026/` | CC BY-NC; bespoke code | **Remove**; **Defer** a generic evidence-pack schema | Licence blocker | — |
| Recording / safe frames | `src/ui/recordingControls.js`, `overlays.css` | Captures nothing | **Remove** | — | — |
| Annotation model | `src/annotations/annotationEngine.js` | No persistence; 22 s TTL default; pulses | **Improve** | Persisted workspace objects with UUIDs, author, time and geometry provenance | TS model, Rust persistence |
| Annotation resolver | `src/annotations/resolver.js` | Field-tuned heuristics; non-deterministic re-resolution | **Replace** → geometry-resolution service | Persist resolved geometry; keys server-side | Rust |
| Annotation renderers | `src/annotations/world*`, `screen*`, `hybrid*` | Sketch aesthetic | **Improve** (world draping; labels with de-collision only) | Readability | TS |
| Draw and measure tool | `src/annotations/drawMode.js`, `drawTool.js` | No edit, move, delete or snap | **Preserve** (drawMode) / **Improve** (tool) | Core analyst primitive with spherical measurement | TS |
| Tool catalog (queries) | `src/tools/**`, `src/tools/queries/*` | Calls the app over HTTP; some licence and policy issues | **Improve** (move execution to Rust) | Typed, validated queries; reuse for agents and reports | Rust |
| MCP server | `src/tools/mcp/*`, `server/mcp/*` | No auth; deny-list tunnel | **Replace** | Rust MCP with auth, audit and allow-list | Rust |
| MCP Apps globe panel | `src/tools/globePanel.js`, `src/app/globePanelRuntime.js` | Fragile host workarounds | **Defer** | — | TS |
| Offline tile and pano CLI tools | `tools/*.mjs` | Possible ToS issues | **Remove** | — | — |
| Local ADS-B (decoder feeds and browser SDR) | `src/sdr/*`, `src/ui/localSdr*.js`, `src/layers/localAdsb/*` | WebUSB is Chrome-only; one tuner | **Replace** (Rust ingest of `aircraft.json`, Beast and SBS) | First-party sensors are valuable professionally | Rust |
| Browser FM radio and internet radio | `src/sdr/sdrAudioWorklet.js`, `spectrum.js`, `src/ui/radio*.js` | Entertainment | **Remove** | — | — |
| Recent imagery and A/B split | `src/ui/recentImagery.js`, `imagerySplit.js`, `imageryBoxTool.js`, `src/maps/imageryComparison.js` | Large single adapter | **Preserve + Improve** | Highest GEOINT value; move search to the backend | Both |
| Weather panel | `src/ui/weatherPanel.js` | US-centric radar | **Preserve** | Observed vs forecast with age stamps | Both |
| Street level | `src/ui/streetLevel*.js` | Floating window | **Improve** | Ground-truth imagery | Both |
| CCTV UI | `src/ui/cctv*.js` | Policy questions; heavy calibration UI | **Defer** | Revisit after policy review | Both |
| Render governor | `src/renderGovernor.js` | Defeated by decoration | **Preserve** | Idle GPU by default | TS |
| FPS monitor | `src/ui/frameRateMonitor.js` | Title-bar only | **Preserve** (diagnostics panel) | — | TS |
| Attribution keyboard support | `src/creditKeyboard.js` | — | **Preserve** | Licensing and accessibility | TS |
| Standalone composition | `src/standalone/*` | Page singleton | **Remove** | Superseded by the Lavender bootstrap | — |

---

## 7. Interaction patterns worth keeping

**Selection, inspection and tracking**

1. **Click to select, then track or follow.** Clicking an entity selects it and shows its telemetry card. Tracking locks the camera and draws a fading trail. **Esc releases** (`layers/flights/tracking.js:1111`, satellites, transit, bikeshare).
   - Keep: one consistent select / inspect / follow / release grammar for every entity kind.
   - Keep: honest states: LIVE, STALE (camera holds the last displayed position), CONTACT LOST and "n inputs unknown · not an all-clear".
2. **Follow-camera smoothing.** Dead reckoning with a bounded correction toward the displayed fix (`cockpitMath.js:27`), heading slew, and a ground-safe minimum height. This avoids lurches when late feed updates arrive. Use it for any followed track.
3. **Proximity roster.** Nearest-N contacts and cohort counts around a subject, with distance and relative bearing, stepped with previous/next (cockpit context panel). Turn this into an inspector section backed by a Rust spatial query.
4. **Per-kind focus framing.** `worldFocus.js` frames a vessel, fire or aircraft at sensible range and pitch, with ECEF validity checks. Generalise it to a table of framing rules per entity kind.

**Camera**

5. **Camera authority.** Navigation generations: the newest explicit user action wins; deferred navigation re-checks after an await; late callbacks cannot take the camera; a share restore yields to a user who has already moved. Implement it as a single camera controller with leases.
6. **Tilt and north-up toggles** that keep a tracked target centred (`cameraOrientationControls.js`).
7. **Nudge verbs.** Pan, tilt and rotate by "a little / once", with explicit refusals at the limits. Bind them to keys and expose them as commands.
8. **Ground guard after arrival.** Keep cameras out of buildings and terrain, and cancel on any user input.
9. **Interruptibility.** Any pointer-down or wheel interrupts automated motion (`cameraVerbs.js:1221`). Keep this as a global rule.

**Search and measurement**

10. **Keyless coordinate search plus semantic framing.** Coordinates and bundled places resolve offline. Geocode results are framed by place type (region, city, neighbourhood, street corridor, building), with the "Tokyo is a prefecture" sanity gate. Extend the parser to DMS and MGRS.
11. **Draw and measure.** Area, line or pin; Enter or double-click to finish; Backspace to undo a vertex; Esc to cancel; label prompt; live great-circle length and spherical area (`drawMode.js`). Add vertex editing, delete and snapping.
12. **Distance arrow.** A from → to annotation that reports distance and stays pinned to the world while orbiting. This is the measurement primitive for voice and agents.

**Imagery and time**

13. **Imagery box tool and A/B swipe.** Draw a box to get dated imagery cards with cloud and coverage reasons. Compare against the basemap or between two dates using an accessible slider (Home / End / arrow keys).
14. **Observation timeline.** Previous/next, latest, play, a coalesced drag commit, and UTC plus age labels (`railTimeline.js`). This is the basis for the workspace time slider.

**Honesty and feedback**

15. **Feed-state honesty everywhere.** Per-layer ON / LOADING / DEGRADED / STALE / PARTIAL / FALLBACK / UNAVAILABLE; "key required" hints that name the key; guidance states (zoom in, empty) that are never errors; `RECONSTRUCTED ESTIMATE` labels on modelled data; dashed rendering for approximate (`synthesized`) geometry.
16. **Share-link follow expiry.** A shared live target is a hand-off with an expiry, not a bookmark.

**Commands and agents**

17. **Point-and-ask deixis.** "This", "that one", and "#2" resolve through `'pointer'` and `referent: n` sentinels, with camera-pose validity checks. Useful for the command palette and agents, not only voice.
18. **Command honesty contract.** Confirm only actions that returned `ok`; state counts verbatim with a named scope; report partial or stale data; exactly one terminal result per call; new input supersedes old intent.

**Accessibility**

19. **Accessible canvas.** Canvas-drawn labels are mirrored into a focusable off-screen list; busy controls keep focus (`aria-busy`); Escape collapses the nearest panel and returns focus to its toggle; focus is restored to the opener after a surface closes.
20. **Push-to-talk on hold, native on tap.** Space held for at least 500 ms; a short tap keeps native button behaviour. Reuse the same hold-versus-tap rule for any future modal key.

---

## 8. Cross-cutting risks and carry-over principles

**Licence blockers.** The following must not ship in Lavender:

- the Nepal / Bhote Koshi pack and derived data (CC BY-NC 4.0);
- the bundled submarine cables (CC BY-NC-SA).

See `docs/audit/data-sources-and-licensing.md`.

**Policy-sensitive surfaces needing an explicit decision:**

- CCTV snapshots and projection;
- ALPR camera mapping;
- viewport screenshots sent to a cloud LLM.

**Security posture**

| Item | Verdict |
|---|---|
| Bounded parsing, refuse-unknown fields, sha256 integrity, no executable content in documents, key-setup admission gates | **Strong.** Carry over |
| MCP and the Realtime token endpoint have no auth (loopback only) | **Weak.** Lavender needs real auth |
| Voice spend cap is client-enforced | **Weak.** Enforce server-side |
| Embed has no origin allowlist | **Weak** |
| Google key sent in browser URLs | **Weak** |
| `window.__godsEyeView`, `__gevAnnotations` and `__CESIUM__` globals | **Weak.** Remove |

**Coupling to undo in the rewrite**

- Display state is tied to modes: shader → HUD, shader → detection, Contacts → detection, Cyber → FLIR, context mode → cockpit and panels.
- Six camera owners.
- An untyped window event bus.
- DOM classes used as state.
- Layer code compensating for shaders.

**Principles to adopt**

1. No decorative module may hold continuous rendering.
2. No post-process may distort geometry or invent values.
3. Every derived or approximate value is labelled as such.
4. Defaults show unaltered data.
5. One registry, one store, one camera controller.

---

## Appendix: verification notes

This document synthesises a full read of the in-scope source, done in parallel by subsystem. The following facts were re-checked directly against the source before writing:

- **FLIR fabrications:** the fabricated temperature (`thermal.js:263`) and the fake frame counter (`thermal.js:301`).
- **HUD fakes:** the random ids (`hud.js:169-172`) and the classification strings (`hud.js:192,200`).
- **First-load defaults:** `visualPresets.js:44-66`.
- **Scope mask default:** `scopeMask.js:104`; share default at `sharelink.js:217`.
- **Render hold:** `visualEffects.js:205`.
- **TR-3B toggle:** `cockpitTrackingController.js:35`.
- **Cockpit `C`-key capture:** `cockpitInput.js:56-69`.
- **Voice:**
  - 30 unique action names in `actionSchemas.js`;
  - cost limits $2 warn and $5 cap (`voiceCost.js:234-235`);
  - models `gpt-realtime-2` and `gpt-realtime-2.1-mini`;
  - voice `marin` and `semantic_vad`.
- **Tool catalog:** 40 tools in `tools/index.js`.
- **Scene document:** `SCENE_DOCUMENT_VERSION = 6`, and the stale `SCENE-DOCUMENT.md`.
- **Viewer settings:** `viewer.js:123-127`.
- **Split-flap:** `SPLIT_FLAP_ENABLED = true`.
- **Local ADS-B:** merges decoder feeds and the browser SDR (`docs/LOCAL-RECEIVERS.md`).

Performance figures come from `legacy/docs/PERFORMANCE.md` (a single Apple M5 capture) and are not a hardware baseline.
