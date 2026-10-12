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
| WS4 Shared core | ENG-925 | L07 ENG-942 · L08 ENG-943 · L09 ENG-944 · L10 ENG-945 |
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

## Environment notes
- Rust 1.99.0 pinned (installed, latest stable); `cargo-tauri` 2.11.3, `cargo-deny`, `cargo-nextest` available.
- Node 25 (Homebrew) is what's on PATH; the monorepo pins Node 22 via mise (mise not installed here).
- Dependency rule (owner decision 2026-10-12): latest versions, exact pins, no release-age gate.
- pnpm 12 is required (`packageManager: pnpm@12.11.2`). Corepack cannot start pnpm 12 (it looks for `bin/pnpm.cjs`); install pnpm standalone. The installed `pnpm` on this Mac is the corepack shim from nvm's Node 18.
- GUI apps (Tauri) cannot open windows from the agent's sandboxed shell; run them unsandboxed.
- A locked macOS screen stops WebView rendering (`visibility: hidden`); desktop frame checks need an unlocked session. Headless Chrome (Playwright) is unaffected.
- Preview config `lavender-fixture` (in the workspace `.claude/launch.json`) serves `apps/web/dist` from `lavender-server` in fixture mode on :7413 (build the server and the web app first).
