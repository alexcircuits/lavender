# Lavender — architecture specification

- Status: v1, 2026-10-11
- Decisions: `docs/adr/0001`–`0008`
- Related: `rust-migration.md`, `frontend.md`, `tauri.md`, `security.md`, `../performance/specification.md`

## 1. System context

```
                         ┌──────────────────────────────────────────────┐
                         │          apps/web  (one frontend)            │
                         │  React shell · features · design system      │
                         │  GlobeEngine (Cesium) · layer renderers      │
                         │              │                               │
                         │     LavenderPlatform (typed interface)       │
                         │        ┌─────┴──────┐                        │
                         │   WebPlatform   TauriPlatform                │
                         └────────┼────────────┼────────────────────────┘
                     HTTP + WebSocket          Tauri IPC (invoke + Channel)
                                  │            │
                   ┌──────────────┴───┐   ┌────┴──────────────────────┐
                   │ lavender-server  │   │ apps/desktop/src-tauri    │
                   │ Axum adapter     │   │ Tauri command adapter     │
                   │ static hosting   │   │ keychain secrets, app dirs│
                   └────────┬─────────┘   └────┬──────────────────────┘
                            └────────┬─────────┘
                       ┌─────────────┴──────────────┐
                       │       lavender-core        │
                       │ LavenderCore facade        │
                       │ feed hub · providers       │
                       │ fetch policy · caches      │
                       │ search · health · settings │
                       ├────────────────────────────┤
                       │ lavender-geo │ lavender-domain │
                       └─────────────┬──────────────┘
                                     │ HTTPS (allowlisted hosts only)
                          External data providers (USGS, adsb.lol, NASA, …)

   Map tiles (imagery / terrain / 3D Tiles) are fetched by Cesium directly from
   catalog-listed tile hosts — the only browser egress (ADR 0008).
```

## 2. Rust workspace

```
Cargo.toml   (resolver 3, edition 2024, rust 1.99, exact-pinned workspace deps, latest versions)
crates/lavender-domain   types, validation, IDs, errors, ts-rs bindings
crates/lavender-geo      geodesy, bbox/antimeridian, region keys, SGP4, R-tree index
crates/lavender-core     LavenderCore, providers, fetch, cache, feed hub, search, health
crates/lavender-server   Axum binary: /api/v1, /api/v1/stream, static SPA, security layers
apps/desktop/src-tauri   Tauri binary (workspace member, not a default member)
```

Lint policy (from the kvbnews template): `unsafe_code = "forbid"`; clippy
`pedantic` warn; `unwrap_used`, `expect_used` and `panic` deny outside tests;
`cargo deny` with a permissive-licence allowlist; `rustfmt` with
`style_edition = "2024"`.

### 2.1 `lavender-domain`

Key types (all `Serialize + Deserialize + TS`, camelCase):

| Type | Purpose |
|---|---|
| `GeoPoint { lat, lon, altM? }`, `BBox { west, south, east, north }` | Validated coordinates; `BBox` handles antimeridian crossing (`west > east`) |
| `FeedId` | `aircraft`, `vessels`, `satellites`, `earthquakes`, `fires`, `launches`, `cyclones`, … |
| `SourceInfo` | Provider registry entry: attribution, licence, commercial status, privacy class (ADR 0008) |
| `FeedQuery { feed, bbox?, filters? }` | What a subscriber wants |
| `FeedSnapshot { feed, source, observedAt?, fetchedAt, freshness, items: FeedItems, coverage, notices }` | One consistent answer |
| `FeedItems` | Tagged union: `aircraft: Aircraft[]`, `earthquakes: Earthquake[]`, … |
| `Freshness` | `live` / `stale { ageS }` / `degraded { reason }` / `unavailable { reason }` / `unconfigured` / `disabledByLicence` |
| `FeedUpdate` | `snapshot` / `error` / `status` (stream frame payload) |
| `ProviderHealth`, `SystemStatus` | Health per source and overall |
| `Catalog { layers: LayerDescriptor[], sources: SourceInfo[], imagery: ImageryDescriptor[] }` | What the frontend can show |
| `SearchQuery`, `SearchResults { places, entities, coordinates }` | Search |
| `ApiError { code, message, retryable }` | Error envelope for both transports |

### 2.2 `lavender-geo`

Pure functions and small structs: haversine/Vincenty distance, bearing,
destination point, bbox ops (contains, intersects, union, split at
antimeridian), region-key quantisation for coalescing, coordinate parsing
(decimal, DMS, MGRS via a port of `mgrs`), SGP4 propagation wrappers over the
`sgp4` crate (TEME→ECEF→geodetic, GMST), ground-track generation, and an R-tree
index (`rstar`) for point stores.

### 2.3 `lavender-core`

| Module | Responsibility |
|---|---|
| `core` (`LavenderCore`) | The facade: `catalog`, `status`, `snapshot`, `subscribe`, `search`, `satellite_track`, `settings`. Built with `LavenderCore::builder(config, secrets, settings_store, cache_dir)` |
| `config` | `CoreConfig`: provider enablement, base-URL overrides (tests), refresh intervals, licence mode, limits |
| `secrets` | `SecretStore` trait + `EnvSecretStore`, `MemorySecretStore` |
| `fetch` | `Fetcher`: reqwest (rustls), host allowlists, https-only, redirect policy, timeouts, body caps, per-host concurrency, retry with backoff for idempotent GETs, UA |
| `cache` | moka TTL caches with single-flight loading (`try_get_with`) |
| `hub` | Feed hub: (feed, region key) → refresh task + `watch` channel; ref-counted; cancellation via `CancellationToken`; bbox filtering per subscription |
| `providers::*` | One module per source: `fetch → parse → validate → normalise` into domain types, declaring `SourceInfo`, hosts and default cadence |
| `search` | Coordinate parser, place geocoders (configurable), entity search across live hub snapshots |
| `health` | Per-source success/failure/latency tracking feeding `Freshness` and `/status` |
| `registry` | Source + layer registry; licence-mode gate |

**Provider contract**:

```rust
#[async_trait]
pub trait FeedProvider: Send + Sync {
    fn source(&self) -> &SourceInfo;
    fn feed(&self) -> FeedId;
    fn cadence(&self) -> Duration;                // default refresh interval
    fn scope(&self) -> ProviderScope;             // Global | Regional { max_span_deg }
    async fn fetch(&self, ctx: &FetchContext, region: &Region) -> Result<FeedItems, ProviderError>;
}
```

Global providers (USGS, FIRMS, satellites) fetch once per cadence, and each
subscription receives a bbox-filtered view. Regional providers (adsb.lol
point-radius queries) are keyed by quantised region.

### 2.4 `lavender-server`

| Route | Purpose |
|---|---|
| `GET /healthz` | Liveness (no dependencies) |
| `GET /api/v1/status` | `SystemStatus` (provider health, versions, licence mode) |
| `GET /api/v1/catalog` | `Catalog` incl. imagery descriptors and public tile keys |
| `GET /api/v1/feeds/{feed}?bbox=w,s,e,n&…` | `FeedSnapshot` (ETag) |
| `GET /api/v1/stream` (WebSocket) | Multiplexed subscriptions (ADR 0005) |
| `GET /api/v1/search?q=…&near=lat,lon` | `SearchResults` |
| `GET /api/v1/satellites/{norad}/track?minutes=&step=` | `SatelliteTrack` |
| `GET /*` | Static SPA from `apps/web/dist` with `index.html` fallback; hashed assets `immutable` |

Tower layers, in order: request id → tracing → panic catch → host check
(DNS-rebinding guard) → timeout → body limit → security headers (CSP, XFO DENY,
nosniff, Referrer-Policy, Permissions-Policy) → per-IP rate limit on `/api` →
routes. Errors use the `ApiError` JSON envelope with correct status codes (no
GEV-style `200 {ok:false}`).

## 3. Frontend (summary; detail in `frontend.md`)

```
apps/web/src
  main.tsx                composition root: createPlatform() → <App>
  platform/               LavenderPlatform, WebPlatform, TauriPlatform (the only I/O)
  contracts/              generated Rust types (ts-rs) + re-exports
  globe/                  GlobeEngine (Cesium owner), imagery, camera, picking, renderers/*
  state/                  Zustand stores: workspace, feeds, ui
  features/               layers, inspector, search, status, attribution, settings
  design/                 tokens.css, primitives (Button, Panel, ListRow, …)
  shell/                  AppShell, NavRail, panels layout
```

## 4. Key data flows

### 4.1 Live layer (web)

1. The user enables Aircraft. The workspace store records `aircraft: on`.
2. `FeedController` (frontend) calls `platform.subscribe({ feed: 'aircraft', bbox: viewBBox })`.
3. `WebPlatform` sends `{op:'subscribe'}` on the shared WebSocket.
4. The server calls `core.subscribe(query)`. The hub finds or starts the refresh
   task for (aircraft, region key). adsb.lol is fetched through `Fetcher`,
   normalised to `Aircraft`, then stored in a `watch` channel.
5. For each new value, the subscription filters to the subscriber's bbox, and
   the server sends `{type:'snapshot'}`.
6. The `AircraftRenderer` diffs by ICAO hex and updates the `BillboardCollection`
   in place. The feed store records count and freshness; React re-renders only
   the layer row.
7. Camera movement debounces into `subscription.update({ bbox })`.

### 4.2 Live layer (desktop)

Steps 3–5 become `invoke('feed_subscribe', { query, channel })`. The Tauri
command calls the same `core.subscribe` and forwards frames to the channel.

### 4.3 Satellite positions

The core holds orbital elements, refreshed every few hours. The frontend
receives elements (not positions) and propagates the visible or tracked subset
per frame. A Web Worker takes over above a few thousand objects. Ground tracks
and passes come from the core (`satellite_track`). This avoids streaming
positions at frame rate.

## 5. Cross-cutting concerns

| Concern | Approach |
|---|---|
| Observability | `tracing` spans per request/subscription/provider fetch; JSON logs in production; `/api/v1/status` health; Prometheus metrics endpoint later |
| Errors | `thiserror` per crate → `ApiError { code, message, retryable }`; no panics in request paths (lint-enforced) |
| Cancellation | Every task holds a `CancellationToken`; dropping the last subscriber cancels refresh; server shutdown is graceful |
| Concurrency | Bounded: per-host semaphores in `Fetcher`, a capped subscriptions-per-connection limit, a capped total refresh-task count |
| Time | `jiff` timestamps, UTC, RFC 3339 on the wire |
| Configuration | Typed, validated at startup, all errors at once (ADR 0006) |
| Security | `security.md` |
| Testing | Unit tests per module; provider contract tests against recorded fixtures through `wiremock`; server integration tests (HTTP + WebSocket); frontend Vitest + Playwright; rendering smoke test; Tauri IPC test |

## 6. Deployment modes

| Mode | Frontend | Backend | Notes |
|---|---|---|---|
| Development | Vite dev server (`:5410`) proxying `/api` → server | `cargo run -p lavender-server` (`:7410`) | Hot reload both sides |
| Web production | Built `dist/` served **by the Rust binary** | `lavender-server` | One process, no Node in production; TLS terminated by the host's reverse proxy |
| Desktop (future) | Same `dist/` embedded in Tauri | `lavender-core` in-process | Optional remote-server mode through `WebPlatform` with a configured base URL |

## 7. What is deliberately not here yet

Persistence of workspaces beyond device preferences, multi-user auth/RBAC,
historical stores, alerts, MCP and voice. Each has a roadmap task. Their
attachment points (`SettingsStore`, workspace document schema, command registry,
audit hook in the facade) exist so they arrive without restructuring.
