# Codebase Map

Last updated: 2026-10-14.

The browser documentation lives in `apps/web/src/docs/`, with its HTML entry in
`apps/web/docs/index.html`. It reads the Markdown in `docs/` through Vite and reuses
the design system. It does not start the platform runtime or feed controller.

Paths are relative to Lavender's root. Find a responsibility here, then use the
[reference](reference.md) for its useful functions.

| Location | Responsibility |
|---|---|
| [`Cargo.toml`](../../Cargo.toml), [`Cargo.lock`](../../Cargo.lock), [`rust-toolchain.toml`](../../rust-toolchain.toml) | Rust workspace, exact dependencies, reproducible toolchain. Default members exclude desktop. |
| [`crates/lavender-domain/src`](../../crates/lavender-domain/src/) | Shared contract: `entities/`, `feed.rs`, `catalog.rs`, `geo.rs`, `stream.rs`, `error.rs`, `search.rs`, `track.rs`, `status.rs`, `source.rs`. |
| [`crates/lavender-geo/src`](../../crates/lavender-geo/src/) | `coords.rs` and `mgrs.rs` parse coordinates; `region.rs` plans map cells; `geodesy.rs` computes distances/bearings; `orbit.rs` propagates satellites; `index.rs` provides a spatial index. |
| [`crates/lavender-core/src/core.rs`](../../crates/lavender-core/src/core.rs) | Build the shared application and follow its public operations. |
| [`crates/lavender-core/src/providers`](../../crates/lavender-core/src/providers/) | `adsb_lol.rs`, `usgs.rs`, `celestrak.rs`, `cyclones.rs`; wire parsing in `celestrak/parsing.rs` and `cyclones/parsing.rs`; recorded/synthetic data in `fixture.rs`. |
| [`crates/lavender-core/src/hub.rs`](../../crates/lavender-core/src/hub.rs) | Public feed hub types and entry points. Read this before its child modules. |
| [`hub/slots.rs`](../../crates/lavender-core/src/hub/slots.rs) | Region planning, client budgets, shared leases, idle eviction and cleanup. |
| [`hub/refresh.rs`](../../crates/lavender-core/src/hub/refresh.rs) | Fetch cadence, health recording, retry delays and supervision. |
| [`hub/subscription.rs`](../../crates/lavender-core/src/hub/subscription.rs) | Each viewer's query, cancellation, region retries and freshness timer. |
| [`hub/snapshot.rs`](../../crates/lavender-core/src/hub/snapshot.rs) | Merge, deduplication, filtering, item caps and freshness. |
| [`fetch.rs`](../../crates/lavender-core/src/fetch.rs), [`config.rs`](../../crates/lavender-core/src/config.rs) | Egress policy and typed core settings. |
| [`fetch/dns.rs`](../../crates/lavender-core/src/fetch/dns.rs) | Bounded public-address validation; the connector uses the validated DNS answers, with explicit configured development exceptions. |
| [`diagnostics.rs`](../../crates/lavender-core/src/diagnostics.rs) | Redacting process panic hook, installed explicitly by server and desktop executables; constructing an embedded core does not change process hooks. |
| [`registry.rs`](../../crates/lavender-core/src/registry.rs), [`sources.rs`](../../crates/lavender-core/src/sources.rs), [`health.rs`](../../crates/lavender-core/src/health.rs) | Layer availability, licensing/credits and provider health. |
| [`search.rs`](../../crates/lavender-core/src/search.rs), [`settings.rs`](../../crates/lavender-core/src/settings.rs), [`secrets.rs`](../../crates/lavender-core/src/secrets.rs) | Loaded-entity/coordinate search, persisted version field, environment or in-memory secrets. |
| [`search/geocoder.rs`](../../crates/lavender-core/src/search/geocoder.rs) | Opt-in Nominatim/Photon lookup, bounded cache/coalescing, process-wide host pacing and cancellation; `geocoder/parsing.rs` normalizes places. |
| [`providers/celestrak/cache.rs`](../../crates/lavender-core/src/providers/celestrak/cache.rs) | Bounded atomic OMM cache envelopes, exact resource identity, conditional validation metadata and legacy cache compatibility. |
| [`aircraft_history.rs`](../../crates/lavender-core/src/aircraft_history.rs) | Bounded session observations captured by shared refreshes, validated generation/position times, monotonic idle eviction and no-network reads. |
| [`orbit/passes.rs`](../../crates/lavender-geo/src/orbit/passes.rs) | WGS84 observer geometry and bounded rise/peak/set search, independent reference fixtures under `lavender-geo/tests/fixtures`. |
| [`workspaceDocument.ts`](../../apps/web/src/state/workspaceDocument.ts), [`workspaceShare.ts`](../../apps/web/src/runtime/workspaceShare.ts) | Validated, bounded v2 URL documents including annotations, strict v1 migration, independent restore revisions, browser navigation and copy-link feedback. |
| [`features/annotations`](../../apps/web/src/features/annotations/), [`renderers/annotations.ts`](../../apps/web/src/globe/renderers/annotations.ts) | Pin/line/area editing, notes, keyboard drawing and approximate spherical measurements; bounded device persistence is coordinated in `runtime/createRuntime.ts`. |
| [`renderers/cyclones.ts`](../../apps/web/src/globe/renderers/cyclones.ts), [`renderers/surfaceArc.ts`](../../apps/web/src/globe/renderers/surfaceArc.ts) | Keyed storm points and same-advisory multipart tracks/cones; bounded ellipsoid arcs and safe rejection of unsupported near-antipodal segments. |
| [`crates/lavender-server/src`](../../crates/lavender-server/src/) | `main.rs` startup/logging/shutdown; `config.rs` TOML/env; `lib.rs` router and headers; `routes.rs` HTTP; `ws.rs` live transport; `clients.rs` connection budgets; `security.rs` host/origin/rate controls; `error.rs` HTTP envelopes. |
| [`apps/desktop/src-tauri`](../../apps/desktop/src-tauri/) | Tauri application, commands, window-bound subscriptions, capabilities and CSP. |
| [`apps/web/src/platform`](../../apps/web/src/platform/) | Transport interface and web/desktop adapters. The only frontend area importing Tauri APIs. |
| [`apps/web/src/runtime`](../../apps/web/src/runtime/), [`state`](../../apps/web/src/state/), [`feeds`](../../apps/web/src/feeds/) | Startup, React context, stores, persistence and live-feed lifecycle. |
| [`apps/web/src/globe`](../../apps/web/src/globe/) | Cesium lifecycle, imagery, camera, picking, capture and `renderers/`. |
| [`apps/web/src/features`](../../apps/web/src/features/), [`shell`](../../apps/web/src/shell/) | Layers, inspector, search, commands, sources and application layout. |
| [`apps/web/src/design`](../../apps/web/src/design/) | Tokens, fonts, formatting, icons and accessible primitives. |
| [`apps/web/src/lib`](../../apps/web/src/lib/) | Named browser computations: coordinates, MGRS, orbit, URLs and time. |
| [`apps/web/src/contracts`](../../apps/web/src/contracts/) | Barrel plus generated TypeScript; change Rust contract types instead of editing `generated/`. |
| [`crates/lavender-core/tests`](../../crates/lavender-core/tests/), [`crates/lavender-server/tests`](../../crates/lavender-server/tests/) | Provider/hub and HTTP/WebSocket integration tests. |
| [`apps/web/e2e`](../../apps/web/e2e/) | Playwright journeys and offline provider documents. Unit tests sit beside frontend code. Rust private-unit tests may live in a sibling `tests.rs`. |
| [`scripts`](../../scripts/), [`project.json`](../../project.json), [`apps/web/project.json`](../../apps/web/project.json) | Whole-app check, contract-drift check and local Nx targets. |
| [`config/development.example.toml`](../../config/development.example.toml) | Optional, secret-free development configuration. |
| [`docs`](../) | Contributor guides, specifications, audit evidence and roadmap. |
| [`legacy`](../../legacy/) | Frozen upstream source, tests, fixtures and its MIT notice. New production code never imports its backend. |
| `target/`, `apps/web/dist/`, `apps/web/node_modules/`, `var/` | Ignored build, dependency and runtime output; do not commit. |

To locate a function, use `rg 'fn snapshot|fn fetch|normalize_aircraft' crates` or
`rg 'applySnapshot|subscribe\(' apps/web/src`. You do not need a generated index
or an AI-specific code explorer to navigate the project.
