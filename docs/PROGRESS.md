# Lavender — progress log

Living record for future sessions. Flokzy (project **Lavender**, team ENG) is
authoritative for task status; this file holds the plan-id ↔ Flokzy mapping,
what was verified, and where to resume. Update it when a task changes state.

## How to resume

1. Read `docs/roadmap.md` (phases, dependencies) and this file.
2. In Flokzy, list Lavender tasks with status Todo whose blockers are done (`list_links`).
3. Read the task (`get_task` with comments) and the specs it references.
4. Work, test, record findings as Flokzy comments, close only when verified.

## Plan id ↔ Flokzy

| WS | Parent | Plan ids → Flokzy |
|---|---|---|
| WS1 Specs & decisions | ENG-922 | L01 ENG-936 · L02 ENG-937 · L03 ENG-938 |
| WS2 Rust foundations | ENG-923 | L04 ENG-939 · L05 ENG-940 · L06 ENG-941 |
| WS3 Providers & API | ENG-924 | L11 ENG-946 · L12 ENG-947 · L13 ENG-948 · L14 ENG-949 · L15 ENG-950 · L16 ENG-951 · L17 ENG-952 · L18 ENG-953 · L19 ENG-954 · L20 ENG-955 · L21 ENG-956 · L22 ENG-957 · L23 ENG-958 · L24 ENG-959 · L25 ENG-960 · L26 ENG-961 · L27 ENG-962 · L28 ENG-963 · L29 ENG-964 · L30 ENG-965 · L31 ENG-966 · L32 ENG-967 · L33 ENG-968 |
| WS4 Shared core | ENG-925 | L07 ENG-942 · L08 ENG-943 · L09 ENG-944 · L10 ENG-945 · readability ENG-1012 |
| WS5 Frontend architecture | ENG-926 | L34 ENG-969 · L35 ENG-970 · L36 ENG-971 · L37 ENG-972 |
| WS6 Rendering | ENG-927 | L38 ENG-973 · L39 ENG-974 · L40 ENG-975 · L41 ENG-976 · L42 ENG-977 · L43 ENG-978 · L44 ENG-979 · L45 ENG-980 |
| WS7 Design system | ENG-928 | L46 ENG-981 · L47 ENG-982 |
| WS8 Interface | ENG-929 | L48 ENG-983 · L49 ENG-984 · L50 ENG-985 · L51 ENG-986 · L52 ENG-987 · L53 ENG-988 · L54 ENG-989 |
| WS9 Feature migration | ENG-930 | L55 ENG-990 · L56 ENG-991 · L57 ENG-992 · L58 ENG-993 · L59 ENG-994 |
| WS10 Tauri | ENG-931 | L60 ENG-995 · L61 ENG-996 · L62 ENG-997 · L63 ENG-998 · L64 ENG-999 |
| WS11 Performance | ENG-932 | L65 ENG-1000 · L66 ENG-1001 |
| WS12 Security & testing | ENG-933 | L67 ENG-1002 · L68 ENG-1003 · L69 ENG-1004 |
| WS13 Documentation | ENG-934 | L70 ENG-1005 · L71 ENG-1006 |
| WS14 Release | ENG-935 | L73 ENG-1007 · L74 ENG-1008 · L75 ENG-1009 |

## Log

### 2026-10-11 — Phase 0 (discovery) and Flokzy plan
- Upstream copy moved intact to `legacy/` (it was untracked; no history lost).
- Audits written by parallel agents and reviewed: `docs/audit/upstream-backend.md`
  (58 endpoints), `upstream-layers.md` (30 layers), `upstream-ui-and-systems.md`,
  `data-sources-and-licensing.md`, `design-references.md`; research in `docs/research/`.
- Tauri × Cesium spike on macOS: passed (ENG-937 closed with evidence).
- Specs: product, architecture (overview, frontend, tauri, security), design
  system (map palette validated with the data-viz validator), performance,
  ADRs 0001–0008, roadmap.
- Flokzy: project Lavender + 14 workstream parents + 74 sub-tasks, all labelled
  and prioritised, `blocks` links per the roadmap. Verified by query: 88 tasks.
- Not committed to git (user did not ask). `products/lavender/` is untracked;
  other products have unrelated uncommitted changes that were left untouched.

### 2026-10-11 — Phase 1 foundations (Rust core, server, desktop, web scaffold)
- Rust workspace (`crates/lavender-{domain,geo,core,server}`, `apps/desktop/src-tauri`):
  contracts with ts-rs bindings (drift check `scripts/check-contracts.sh`), geodesy
  and MGRS, egress-safe fetcher, feed hub, registry with licence gate, USGS / adsb.lol
  / CelesTrak providers, search, `LavenderCore` facade; Axum server (API, WebSocket
  stream, SPA, security layers); Tauri commands + channels with per-command
  capabilities. ENG-939…947 closed with evidence.
- Live smoke (one-off, server stopped afterwards; adsb.lol rate-limits): earthquakes,
  satellites, aircraft over FI/EE, search and an ISS track all answered.
- Web scaffold, design tokens and the primitive library (ENG-969, 981, 982 closed).

### 2026-10-12 — Backend hardening (independent review findings)
- Fixed in `crates/**` with a regression test each: hub linger timers (one per
  slot) and idle-slot eviction; X-Forwarded-For read right to left; partial
  coverage and retry for regions that do not fit; correct `pendingRegions` in the
  all-error case; per-client region budgets; cancel-safe pacing; bounded rate
  limiter (moka, IPv6 /64 keys); WebSocket send/idle timeouts, ping/pong,
  frame-size cap, message-rate cap, connection caps; `Drop` for the hub;
  CelesTrak cache age, degraded freshness/health and per-group retry floor;
  row-by-row provider parsing and capped error text; MGRS band check (and
  zero-digit MGRS output); JSON envelope for every `/api` error, 429 for hub
  limits, security headers on host refusals, HTTP/2 authority; LOW items
  (provider panic supervision, freshness recheck timer, body-cap ceiling,
  track `from` bound, bounded cell planning, origin scheme check, desktop fmt).
- No wire-contract change (`scripts/check-contracts.sh`: up to date). New
  dependency: `moka` 0.12.16 (already a workspace dependency) in
  `lavender-server` with its `sync` feature.
- `cargo deny check`: bans, licences and sources pass; advisories failed only on
  RUSTSEC-2024-0370 (`proc-macro-error`, unmaintained, via Tauri's GTK tree),
  which predates this work. Resolved by a commented, scoped ignore in `deny.toml`
  (build-time macro, no safe upgrade; revisit with ENG-998/ENG-1008). Now all pass.
- Spec updated: `docs/architecture/security.md` §2 and §4. Flokzy ENG-1010 closed
  with the per-finding record.

### 2026-10-12 — Frontend vertical slice (web + desktop) and fixture-backed e2e
- **Architecture (written first, then parallelised against it):** `src/platform`
  (`LavenderPlatform`; Web adapter with ETag reuse, one multiplexed WebSocket,
  jittered reconnect + resubscribe, polling fallback; Tauri adapter on `invoke` +
  `Channel`; one contract suite for both), `src/state/stores.ts` (workspace, feeds,
  system, ui), `src/feeds/FeedController.ts` (subscriptions ↔ globe ↔ summaries,
  300 ms viewport debounce, held selection, satellite orbit on select),
  `src/runtime` (composition: catalog load with backoff, status polling, validated
  workspace persistence, imagery sync, React context with narrow selectors, the
  desktop self-check), `src/globe/index.ts` (`GlobeController` contract).
- **Globe engine** (`src/globe/**`): CesiumWidget in request-render mode with bounded
  render-loop restart and context-loss reporting; NASA GIBS WMTS on the 4326 `500m`
  grid (2×1 at level 0, 512 px tiles from (-180, 90); verified against GIBS
  GetCapabilities and one real tile per layer on 2026-10-12) with fade-in swap and
  bundled fallback; camera (reduced motion = instant); pick and rAF-throttled cursor;
  keyed in-place renderers for aircraft (canvas glyphs, civil/military shapes,
  budgeted labels on a ground-colour backing), earthquakes (5-step hazard ramp) and
  satellites (`satellite.js`, MIT; 7.1.0 since the upgrade below; client SGP4 agrees with `lavender-geo`
  within 0.0002° / 20 m on the fixture in `src/globe/renderers/__fixtures__/`);
  lavender selection halo; stale symbols at 60 %; the Cesium ion logo removed (no
  ion use); `captureFrame()` diagnostic with a lit-pixel ratio.
- **Interface** (`src/shell`, `src/features`, `src/lib`): nav rail, side islands,
  layer explorer (families, counts, freshness meters, honest states, options),
  inspector (aircraft / earthquake / satellite fact sheets, provenance, "no longer in
  feed"), search + ⌘K palette on a command registry (decimal / DMS / MGRS parsed
  without network), shortcut sheet, sources panel (attribution verbatim, licence,
  health), status bar (transport, cursor decimal + MGRS, camera height, UTC),
  responsive down to 375 px (bottom sheets).
- **Fixture mode** for end-to-end tests: `server.fixtures_dir` replays recorded /
  synthetic documents from `apps/web/e2e/fixtures/` through the real parsers, hub,
  API and stream (`crates/lavender-core/src/providers/fixture.rs`); every snapshot
  carries "Recorded fixture data, not live.". Playwright now builds the app and runs
  `lavender-server` in fixture mode on :7412.
- **Verified (2026-10-12):** Rust 217 tests, clippy `-D warnings`, fmt, `cargo deny`,
  contracts check; web 319 tests (42 files), tsc, Biome, build (initial JS 184.1 KiB
  gzip; engine and satellite.js in the lazy chunk); Playwright 3/3 (installed Chrome)
  — boot + default layer + lit-pixel rendering check + no console errors / CSP
  violations; security headers + CSP; aircraft on → search callsign → inspector →
  Esc → ⌘K coordinates. Manual browser walkthrough against the fixture server at
  1024 px and 375 px (screens reviewed: night imagery, aircraft labels, halo,
  satellite orbit + inspector sub-satellite point, sources panel).
- **Desktop (ENG-996):** the Tauri app ran the same frontend on the in-process core:
  self-check report `ok` on data — platform desktop, transport live, 45 live USGS
  earthquakes (cross-checked: the live feed had 45 features at that moment), globe
  attached, no errors or CSP violations. The rendered-frame part could not run: the
  macOS session was locked (`CGSSessionScreenIsLocked = 1`), so the WebView reported
  `visibility: hidden`, 0 fps. Rerun with the screen unlocked (see `docs/architecture/tauri.md`).
- **Known, not done:** perf benchmarks (10 000 aircraft diffs, 5 000 satellites) wait
  for the perf harness (ENG-1000); fires/cyclones renderers wait for their providers
  (ENG-950/951); Dialog primitive lacks hidden-title/top-placement options (worked
  around in the palette); feed chip and status bar subscribe to the whole summaries
  map; rail expansion on hover/⌘\ and bottom-sheet snap points not built.
- One unexplained `lavender-server` API test failure was seen once while the whole
  workspace compiled in parallel; 32 reruns (24 under heavy concurrency) passed.
  Most timing-sensitive candidate: `rate_limit_returns_429_with_retry_after` (1 token/s).

### 2026-10-12 — Upgrade to latest versions; desktop end-to-end verified
- Owner decision: use the latest versions (no release-age gate). Web: `@cesium/engine`
  26.4.0 + `@cesium/core` 0.1.0, `@base-ui/react` 1.9.0, satellite.js 7.1.0, Vite 8.3.4,
  Vitest 5.0.3, Playwright 1.64.0, happy-dom 21.0.1, Biome 2.5.15, `@types/node` 26.6.5,
  `@tauri-apps/api` 2.12.3, pnpm 12.11.2. Rust: toolchain 1.99.0, tauri 2.12.3,
  tauri-build 2.7.2, tokio 1.53.2, tokio-util 0.7.20, toml 1.1.8, serde_json 1.0.154,
  jiff 0.2.38, proptest 1.12.0, tokio-tungstenite 0.30.0; `cargo update` for the rest.
- Breakages and code changes (`docs/architecture/frontend.md` §1): CesiumJS 1.146 moved
  core types to `@cesium/core` (imports moved, guards extended); satellite.js 7's WASM
  pthreads worker broke the Vite build (plugin `satellite-js-without-wasm.ts` keeps it
  out; decay check now from the library); Rust 1.99 clippy `assert_is_empty` (2 tests);
  Vite's native config loader wants explicit `.ts` extensions; corepack cannot run
  pnpm 12 (install pnpm standalone).
- Verified: Rust fmt, clippy `-D warnings`, 217 tests, `cargo deny`, contracts; web tsc,
  319 tests, Biome, build (initial JS 186.3 KiB gzip, no WASM/worker emitted); Playwright
  4/4 × 3 runs; browser check with satellites on (no console errors, no WASM requests).
- Desktop self-check with the screen unlocked: `ok: true`, 53 live USGS earthquakes,
  36 fps, lit-pixel ratio 0.245 (evidence in `docs/architecture/evidence/`).

### 2026-10-13 — Human-first engineering and contributor documentation
- Continued from the clean nested Lavender Git repository at `8569c34`; created
  local branch `improvement/human-first-engineering`. No unrelated product edits.
- Kept the four cohesive crates and existing web/Tauri boundaries. Split hub
  scheduling, region leases, snapshots and subscriber lifecycle into private modules;
  separated test files and CelesTrak parsing; improved private operation names and
  module purpose docs. Public contracts and scheduling/security behaviour preserved.
- Added all ten contributor guides under `docs/development/`, glossary, task index
  and verification record. `docs/README.md` is the accessible home. The roadmap now
  has six readable milestones; `docs/roadmap-detail.md` preserves original L/WS/P scope.
- Added `scripts/check.sh`, a safe development TOML example and working Nx dev/check
  entry points. Nx frontend commands launch web pnpm 12.11.2 explicitly while the
  monorepo keeps its own pin; the dev target starts Rust and Vite together.
- Verified: full-workspace fmt/clippy, 217 original Rust tests plus one compiled
  Rustdoc example, cargo-deny, unchanged TS contracts; 319 frontend tests, TypeScript,
  Biome/build; four fixture E2E journeys; warning-free Rustdoc; custom-protocol native
  build; Nx dev/proxy/check, configuration startup, fixture API requests and 198 local
  documentation targets. Full detail and limitations: `development/verification.md`.
- Flokzy: refined all 67 initially open descriptions/titles in place; read-back
  confirmed discussion, metadata and dependency links were preserved. ENG-1012 closed
  with evidence. Other statuses remain unchanged; 24 of 90 tasks are now Done.
  ENG-996's later macOS evidence was reconciled in its description/comment, without
  claiming a fresh GUI run or closing the task. No duplicate project/task/cycle.
- Local changes remain uncommitted. Hosted monorepo integration (ENG-1004), remaining
  provider parity, keychain/cross-platform releases, performance and licence/hosting
  decisions remain separate open work. All verification servers stopped.

## 2026-10-13 — browser developer documentation (ENG-1014)

- Added `/docs/` as a separate Vite/React entry, using Lavender's existing Geist
  fonts, dark surfaces, lavender accent, Base UI dialogs and command controls.
- The browser reads the repository's 42 Markdown documents. Guides, source files,
  the historical archive and Mermaid rendering load on demand. Reading docs does
  not start feeds or load Cesium. The workspace has a documentation link.
- Navigation, section permalinks, full-text keyboard search, highlighted/copied
  examples, GFM tables, diagrams and a linked Rust/TypeScript source reader work in
  the built app. Mobile navigation uses a focus-trapped dialog.
- Verified TypeScript, lint, 325 frontend tests and seven production browser
  journeys, including the four existing analyst journeys. The docs journeys check
  CSP, console errors, source/Markdown reading, copied text, section reload/back,
  keyboard search, missing-page recovery and 320/360 px reflow.
- Initial compressed JavaScript remains below the 250 KiB budget for both entries;
  the build check now enforces this for the docs too. Desktop, tablet, phone and
  the narrow layout corresponding to 200% zoom were reviewed in the browser.
- Vite's file scope explicitly includes the Lavender repository, so Markdown and
  source files outside `apps/web` remain readable after hot reload. No monorepo
  or unrelated product directories were added to the scope.
- Work remains local and uncommitted on `improvement/human-first-engineering`.
  The docs preview on port 5410 stays running for the browser handoff. Hosted
  deployment and native desktop GUI verification remain separate work.

## 2026-10-13–14 — autonomous feature execution

- Continued on `improvement/human-first-engineering`, preserving the existing
  readability and browser-docs changes and unrelated monorepo work. Three coding
  agents owned aircraft/history, place search and share/UI work; general, Rust
  and TypeScript reviewers checked integration and follow-up satellite/cache work.
- **ENG-972 Done:** bounded v1 workspace links restore camera, imagery, layer
  options and selection. Deferred catalog/globe work respects newer user actions;
  browser back/forward restores the exact camera. Unsupported or oversized links
  fail safely. Native deep-link registration and annotations remain separate scope.
- **ENG-949 Done:** geometric observer passes now run through geo/core, HTTP,
  Tauri and the inspector. Independent propagation/pass fixtures and narrow
  window-edge regressions pass; ground tracks count their initial point in the
  1,440-point cap. The observer uses the screen-centre ground point. CelesTrak's
  bounded atomic cache preserves raw OMM, download/validation ages and conditional
  validators across restart; redirects cannot rebind validators to the wrong URL.
- **ENG-948 In Progress:** bounded session history is integrated through both
  adapters and the inspector (4,096 aircraft, 256 points each, 65,536 globally,
  30-minute idle TTL). Upstream document/position times control ordering; delayed
  responses cannot reorder trails and unreliable times are excluded from history.
  Optional licensed OpenSky, permitted enrichment and a longer live pacing soak remain.
- **ENG-952 In Progress:** configured Nominatim/Photon geocoding is integrated with
  bounded cached/coalesced lookups, host-wide pacing, cancellation, attribution and
  licence gating. Place search is explicitly submitted and can retry unchanged
  text; local coordinate/entity search stays local. Vessel/MMSI search awaits
  ENG-953; production instance/terms choice remains ENG-938.
- Final `scripts/check.sh` passed: 293 Rust unit/integration tests plus one Rustdoc
  test, 382 frontend tests in 47 files, formatting, Clippy, dependency checks,
  generated contracts, TypeScript, Biome and production builds. Native desktop
  all-target Clippy passed. Eleven production Chrome journeys passed, including
  share/restore, aircraft history and satellite passes; screenshots were inspected.
  Provider HTTP mocks and recorded fixtures do not claim fresh live-source soaks,
  production geocoder access, a native GUI run or Windows/Linux verification.
- Flokzy read-back: **27/91 tasks Done**, 64 open. ENG-957 and ENG-989 now have all
  recorded prerequisites Done; ENG-976 remains In Review for its separate renderer
  and performance acceptance. Updated migration, API/module references, roadmap,
  task index and verification record. No duplicate project/task/cycle or PR created.
  Work remains local and uncommitted; the earlier docs preview was preserved.

## 2026-10-14 — work-in-progress GitHub checkpoint

The owner requested a commit and push before further implementation. This checkpoint
includes the earlier verified work and the unfinished next batch on
`improvement/human-first-engineering`; it is not a release-ready verification result.

- ENG-951: the Rust cyclone feed, strict status/GIS parsing, source metadata,
  coalesced quota-safe caching, typed contracts and fixture/API integration are
  written; 23 focused checks passed. Cyclone rendering/inspection/legend and local
  search are written under ENG-975, but final browser integration remains pending.
- ENG-989: annotation editor, map drawing, spherical measurements, v2 workspace
  migration and device/share persistence are written. The initial focused suite
  and browser journey passed, but independent review found hydration data loss
  before catalog readiness, near-antipodal Cesium exceptions, persistence ordering
  and drawing-Escape ownership. These corrections remain required.
- ENG-1002: captured-log regressions, safer request/panic logging, validated DNS
  addresses used by the connector, explicit adapter panic hooks, build-check
  negative fixtures and product advisory workflow source are written. Focused Rust,
  Node and advisory checks passed. Review found missing prefixed secret-key names
  and CSP element/attribute override checks; hosted CI wiring remains unverified.
- Other known pending corrections: the cyclone healthy-refresh floor for configured
  cadences above 600 seconds; exception isolation/bounded expansion for cyclone
  arcs; neutral symbols for unreported cyclone wind; the Ion build transform must
  match the installed SDK's indirect default-token declaration. The transform is
  currently a known build blocker. One cyclone renderer test also needs its
  public-geometry assertion corrected. Final full checks and distribution rescan
  have not passed for this batch. Tasks remain In Progress.
- No unrelated monorepo files, generated build output, local credentials or test
  screenshots are intended for this checkpoint. Continue with these review findings
  before closing tasks or treating the branch as deployable.

## 2026-10-15–16 — providers, globe layers and the performance harness

The previous session stopped mid-fix with 141 uncommitted files. This pass finished that work
and added new layers. It ran as reviewed parallel lanes:
- Each lane worked in a git worktree under `/Volumes/NoirSystems/lavender-build/wt/<lane>`, cut
  from a local snapshot of the uncommitted tree.
- Each lane was implemented, independently reviewed, fixed and, where needed, rechecked.
- The lead merged each lane here with `lavender-build/integrate.sh <lane>`, a three-way merge
  per file, and resolved the conflicts by hand.
- Cargo targets live on `/Volumes/Code/lavender-build/` (owner: no builds on the system drive).

**Done in Flokzy, with evidence in each task's comments:**
- **Fires — ENG-950 (FIRMS):**
  - Hub dedup and FRP filtering fixed.
  - Exact counts and the ≤ 1 MB world snapshot proved on 166 549 real detections.
  - Activation measured at 161 ms in the harness.
- **Launches — ENG-957:**
  - The disk cache survives restarts.
  - Every LL2 attempt counts against the hourly quota.
  - Upcoming launches are included.
- **Wind — ENG-955:**
  - Real GFS (template 5.3) and IFS (5.42) GRIB2 fixtures, checked against ecCodes reference values.
  - Byte-order and bounded-memory tests.
  - Desktop and web adapters.
- **Vessels — ENG-953 (AIS store):**
  - AISStream link with a watchdog.
  - Bounded R-tree store with time-bucket eviction.
  - Track route.
  - 0.44 ms median area query over 50 000 ships.
- **Perimeters — ENG-956:**
  - WFIGS perimeters, simplified per zoom, with size caps.
  - InciWeb links via a bounded page scanner.
- **Hazard rendering — ENG-975:**
  - Quake rings, fire cells and cyclone spirals.
  - One shared fire validity rule.
  - An imagery-aware edge for contrast.
  - The camera-settlement change finished.
- **ENG-974 / ENG-976 (aircraft, satellites):** performance targets measured with the harness.

**Integrated, still open:**
- **Harness — ENG-1000:** `scripts/perf.sh`. The first capture is
  `docs/performance/2026-10-15-capture.md`. Done once that capture is committed.
- **Vessels on the globe — ENG-980:**
  - Renderer, inspector, track, search.
  - Its 12 000-ship target is measured only in a lane spec so far; it needs a harness capture.
- **First-wave layer UI — ENG-990:**
  - Launches list, pad markers and inspector merged.
  - Wind, perimeters, weather and context slices remain.
- **Search — ENG-952:** vessel search works; it waits on ENG-938 (geocoder terms).
- **Contrast follow-up — ENG-1048 (new):** inner-ring and stale-symbol contrast; fire-cell z-fighting.
- **New tasks:**
  - ENG-1046: the boundaries layer, in progress.
  - ENG-1047: conflict-layer scoping, which needs owner input before any follow-up tasks.

**Other changes:**
- `FixtureSecretStore`: fixture mode replays key-gated layers without any key.
- `LAVENDER_E2E_PORT`: parallel checkouts can run Playwright.
- The AIS TLS tests generate a throwaway CA in memory (`rcgen`, dev-only), so no private key is
  committed.

**Verified on the merged tree (2026-10-16):**
- `scripts/check.sh`: fmt, clippy `-D warnings`, Rust 620/620, contracts, cargo-deny; web tsc,
  Biome, Vitest 655/655, build/check-dist.
- Desktop clippy/tests with `LAVENDER_CHECK_DESKTOP=1` in the earlier pass.
- Playwright: 29 passed, 5 opt-in specs skipped.
- Live upstreams were not exercised for:
  - the FIRMS keyed endpoint (no MAP_KEY);
  - AISStream (no key);
  - InciWeb (HTTP 403 from this network).

**Still running:** batch 2b (lavender-lanes run `wf_0622bdaa-44a`, from snapshot
`lavender-build/SNAP3`). Lanes:
- `boundaries` (ENG-1046);
- `wind-ui` and `perimeters-ui` (ENG-990);
- `perf-core` (ENG-1001 server side).

Resume an interrupted run with `resumeFromRunId` and the same args, merge with `integrate.sh`,
then run `check.sh` and e2e. The lanes' documentation notes are collected in
`lavender-build/docnotes.json`; they still have to be applied to the shared docs.

## Environment notes
- Rust 1.99.0 pinned (installed, latest stable); `cargo-tauri` 2.11.3, `cargo-deny`, `cargo-nextest` available.
- Node 25 (Homebrew) is what's on PATH; the monorepo pins Node 22 via mise (mise not installed here).
- Dependency rule (owner decision 2026-10-12): latest versions, exact pins, no release-age gate.
- pnpm 12 is required (`packageManager: pnpm@12.11.2`). Corepack cannot start pnpm 12 (it looks for `bin/pnpm.cjs`); install pnpm standalone. The installed `pnpm` on this Mac is the corepack shim from nvm's Node 18.
- GUI apps (Tauri) cannot open windows from the agent's sandboxed shell; run them unsandboxed.
- A locked macOS screen stops WebView rendering (`visibility: hidden`); desktop frame checks need an unlocked session. Headless Chrome (Playwright) is unaffected.
- Preview config `lavender-fixture` (in the workspace `.claude/launch.json`) serves `apps/web/dist` from `lavender-server` in fixture mode on :7413 (build the server and the web app first).

## 2026-10-24 — Orchestrated continuation: upstream import + wave planning

- Imported all 1095 upstream commits under `legacy/` (subtree merge `30047df2`);
  `HEAD..upstream/main` is now empty and `legacy/` matches upstream/main byte
  for byte. Derived `boundaries.json` moved to
  `crates/lavender-core/data/natural_earth/` first so the mirror stays pristine.
- Checkpointed the in-flight second provider wave (`0eaebbb3`) after repairs:
  web typecheck + 772/772 vitest green, `cargo check --workspace` green.
  Fixes: renderer `locate()` aligned to `Located | undefined` with `symbolPx`;
  test fixtures updated to regenerated contracts (Freshness state union, bigint
  CCTV timestamps, TransitTrail/CameraSource/SubmarineCableItem shapes);
  FeedController guard for pseudo-feeds; missing `SubmarineCableApiItem`
  import; transit `PolylineCollection` value import.
- New docs: [upstream delta + port backlog](development/upstream-delta-2026-10.md),
  [language fit review](development/language-fit.md), [kanban](development/kanban.md).
- Waves in flight: LB-110 borders/divisions/region drawing (owner top
  priority), LB-111 flight motion + simulation, LB-112 launch ascent
  simulation, LB-113 kanban mission boards, LB-133 NoirSystems token
  alignment. Flokzy MCP is not connected to this environment; the kanban file
  is the interim tracker.

## 2026-10-24 (later) — wave 1 shipped: geofences, chrome tokens, simulations, mission boards

- **LB-110** geofence drawing + boundary classes + division drill-down
  (`12129766`); web 785/785.
- **LB-133** NoirSystems dark-register chrome (`06c74058`); all enforced
  contrast pairs re-measured.
- **LB-111/112** Rust simulation stack (`f2c49257`): motion.rs (dead-reckoning
  port), simulation.rs (seeded synthetic aircraft), geo/ascent.rs (2-DOF
  spherical ascent with pitch program, staging and insertion physics);
  Ascent* contracts exported; vision-IPC airport/port fixture bug fixed;
  workspace fmt/clippy(-D warnings)/tests/check-contracts all green.
- **LB-113** mission boards (`387f01f7`); web 789/789 (103 files).
- Environment note: background agents only survive while the main session is
  idle — the account's model concurrency rejects extra streams during active
  orchestrator turns. Sequential main-session execution is the reliable mode.

## 2026-10-24 (close) — LB-130 wrapper shipped; agent fleet blocked by provider cap

- Heatmap WASM wrapper + TS fallback and the pinned-toolchain rebuild
  script committed and pushed (`5607bb52`); web suite 795/795 (104 files).
- Repeated agent launches (5 across the afternoon, correct briefs,
  non-overlapping scopes) were all culled by the account's model
  concurrency cap / "Start Plan is busy" throttle — including while the
  main session was fully idle. Two ready-to-run briefs (LB-150 timeline
  replay, LB-151 sim hub wiring + divisions) are recorded in kanban.md.
- Everything of value is committed and pushed to
  github.com/alexcircuits/lavender (branch improvement/human-first-
  engineering, through 5607bb52).

## 2026-10-24 (final) — simulation source live through the hub

- **LB-151 core shipped** (`b67bb1c4`): `providers.simulation` config
  (default off) swaps the aircraft feed's source to the deterministic
  simulator at registration — global scope, step cadence, 'simulation'
  provenance stamp, synthetic-data snapshot notice. Workspace green.
- Agent fleet status: every launch this afternoon (7 across mixed briefs)
  was culled by the account's model-concurrency cap / provider throttle,
  including with the main session idle; two late launches failed at the
  transport layer. All shipped work was therefore executed in the main
  session. Ready-to-run briefs for the next slices (LB-150 timeline
  replay, LB-152 admin-1 divisions, LB-109 provider audit) live in
  kanban.md.

## 2026-10-25 — Wave B Layer Integration (ENG-991), Admin-1 Divisions (LB-152) & Timeline Replay (LB-150)

- **Wave B Layer Integration & Parity Sign-off (`ENG-991` Done)**:
  - Recent Satellite Imagery (`ENG-968` / `F-063`): `RecentImageryPanel.tsx` and `recentImagery.ts`
    Cesium renderer integrating NASA GIBS EPSG:3857 tile layers (`S30`, `L30`, `VIIRS`), day grouping,
    cloud cover rankings, interactive bounding-box drawing session on globe with camera suppression,
    and NASA EOSDIS attribution.
  - Imagery A/B Comparison (`F-064`): GPU-clipped comparisons using Cesium `scene.splitPosition` and
    `layer.splitDirection` via accessible draggable `ImagerySplitDivider.tsx` with ARIA keyboard controls.
  - Street-Level Photos (`ENG-965` / `F-062`): `StreetLevelViewerHost.tsx` container checking
    Mapillary credentials, telemetry compass/heading readout, capture date, thumbnail carousel,
    and CC BY-SA 4.0 credit.
  - Travel Directions & Turn-by-Turn Routing (`ENG-963` / `F-072`): `DirectionsPanel.tsx` and `DirectionsRenderer`
    with freestanding Zig WASM RDP decimation (`simplifyRdpWasm`), travel modes (`car`, `bike`, `foot`),
    maneuver step navigation, and camera focus.
  - Wave B Inspector Cards: CCTV with Mojo MLIR vision perception streaming (`CctvCard.tsx`), TeleGeography
    submarine cables (`CableCard.tsx`), GTFS-RT public transit with Zig WASM RDP trails (`TransitCard.tsx`),
    GBFS bikeshare docks (`BikeshareCard.tsx`), and defense installations (`MilitaryInstallationCard.tsx`).
  - Feature parity matrix (`docs/audit/feature-parity-matrix.md`) updated with verified status for
    rows F-048, F-049, F-056, F-062, F-063, F-064, F-068, F-069, F-070, F-071, F-072.
  - Flokzy task `ENG-991` completed and closed with full verification evidence.
- **LB-150 Timeline Replay (web)**:
  - Built `features/replay` (`ReplayPanel.tsx`, `interpolate.ts`, `replay.module.css`, `types.ts`, `index.ts`).
  - Integrated `'replay'` panel into `SidePanel` union, `SidePanel.tsx`, and `NavRail.tsx` with `ClockCounterClockwiseIcon`.
  - Replays historical trajectory for active aircraft (`platform.aircraftTrack`) and vessels (`platform.vesselTrack`).
  - Interactive timeline scrubber with UTC time readout, span formatting, and kinematics telemetry (lat, lon, altitude, heading).
  - Transport controls (Play/Pause, 1×/2×/4× speeds, Start/End seek shortcuts).
  - Camera follow via `globe.flyTo` at 500 ms throttled interval.
  - Mirrored globe track trail rendering (`globe.showAircraftTrack` / `globe.showVesselTrack`).
  - Comprehensive unit and integration test coverage (`interpolate.test.ts`, `ReplayPanel.test.tsx`),
    strict design system token compliance (`architecture.test.ts` 11/11 passing).


## 2026-10-25 (evening) — fleet wave verified and pushed

- Zombie-agent fleet (from the earlier 'failed' launches that survived
  server-side) delivered: Wave B inspector cards + directions + admin-1
  division serving (97aac60b), timeline replay + Wave B completion
  (93174c75), alerts inbox, briefing export, licence registry entries and
  the LB-109 provider-verification audit (95229e41, committed by the
  watchdog after verification).
- Watchdog fixes during verification: startup budget restored
  (code-split the four Wave B panels, 262→239.3 KiB); Zig 0.16.0 stable
  unblocked the kernel rebuild (d6401663) — heatmap + point-in-polygon
  kernels live in the browser (13/13 wrapper tests).
- Known load-flaky tests (pass in isolation): AnnotationsPanel Escape,
  launchesIsland focus — pre-existing 5 s timeouts on a busy machine.
- Demo server restarted on the new build: http://127.0.0.1:7410 with
  simulation + fixtures; /api/v1/divisions/{at,search} live.

## 2026-10-25 (night) — geofence alerting engine merged

- Reviewed and merged gemini/geofence-alerts (299c6155): core/src/alerting
  {engine,geometry,state}, default-off AlertingConfig with rule/inbox caps,
  hub snapshot hook, /api/v1/alerts/{rules,inbox} routes, sources entry;
  571 lines of core + route tests. This completes the full loop with the
  web Alerts panel shipped in 95229e41: drawn geofences -> monitored rules
  -> alert inbox. Demo server restarted on the merged build.
