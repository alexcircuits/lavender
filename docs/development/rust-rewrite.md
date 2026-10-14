# Rust Rewrite Guide

Rust gives the web server and desktop app one reusable backend, with typed data,
controlled external requests and bounded live-feed work. The React/TypeScript
interface remains responsible for presentation and browser rendering. This is an
incremental feature migration, not a new architecture for every provider.

The new application already runs without the legacy Node server. That does **not**
mean all upstream functionality has migrated: many layers exist only in `legacy/`.

## Migration status — checked 2026-10-13

These are capability assessments from source and evidence, alongside the actual
Flokzy workflow values. A task can remain open after its basic feed works because
its acceptance criteria also require tracks, additional sources or measurements.

| Capability | Migration state | Implemented replacement | Remaining work / tracker status |
|---|---|---|---|
| Contract, geo, fetch policy, hub, registry, core | **Completed** | `crates/lavender-{domain,geo,core}` | ENG-939–945: Done; keep their history. |
| HTTP/WS and built frontend hosting | **Completed** | `lavender-server/src/{lib,routes,ws}.rs` | ENG-946: Done. Production deployment is separate. |
| USGS earthquakes | **Completed** | `providers/usgs.rs`, earthquake renderer | ENG-947: Done. Other hazard renderers are not covered. |
| Aircraft | **In progress** | `providers/adsb_lol.rs`, keyed renderer, bounded `aircraft_history.rs`, HTTP/Tauri history and inspector | ENG-948: In Progress; optional OpenSky, permitted enrichment and live pacing soak remain. History uses validated upstream generation time and position age; unreliable timestamps are not captured. ENG-974: In Review. |
| Satellites | **Completed provider/track/pass scope** | `providers/celestrak.rs`, atomic conditional cache, `lavender-geo/src/orbit.rs`, browser propagation, track/pass APIs and inspector | ENG-949: Done. ETag/Last-Modified revalidation, restart and redirect safety, quotas, inclusive track cap and geometric reference tests pass. ENG-976 remains In Review for renderer/performance scope; commercial permissions remain ENG-938. |
| Search | **In progress** | `core/src/search.rs`, opt-in `search/geocoder.rs`, explicit place action in search/palette | ENG-952: In Progress; future vessel search and production endpoint choice remain. Nominatim/Photon contracts, bounded cache, attribution and process-wide pacing are implemented. ENG-986: Done for interface work. |
| Desktop transport | **In progress** in tracker | Tauri commands/channels, `TauriPlatform`, recorded macOS self-check | ENG-995: Done; ENG-996: In Progress. Unlocked rendering evidence exists from 2026-10-12; reconcile it with the task before closure. |
| Fires and cyclones | **Not started** providers | No Rust feed/entity yet | ENG-950/951: Todo. ENG-975: In Progress contains the implemented earthquake renderer and missing fire/cyclone renderers. |
| Vessels, weather, wind, perimeters, launches, static context, terrain | **Not started** | No production Rust provider yet | ENG-953–959: Todo. Source licensing and GRIB decoding need their own decisions/tests. |
| Traffic, transit, bike-share, directions, CCTV, Mapillary, installations, cables, recent imagery | **Not started** | Legacy reference only | ENG-960–968: Todo; readiness also depends on terms and feature-specific inputs. |
| Keychain, Windows/Linux, installers, updates | **Not started** | Desktop currently uses session-only secrets | ENG-997–999: Todo. A macOS prototype is not a distributable desktop release. |
| Commercial clearance, deployment and legacy removal | **Blocked** release gates | Registry enforces known restrictions; release sign-off is absent | ENG-938, ENG-1007–1009: Todo; owner decisions, provider terms, full parity and regression are prerequisites. This is a release dependency, not an invented Flokzy “Blocked” status. |
| React UI, Cesium rendering, browser propagation, frontend tools | **Intentionally retained in JavaScript/TypeScript** | `apps/web` | Do not migrate these to Rust merely to remove Node build tooling. |

For every task link and recorded dependency, see the [Task Index](task-index.md).
The [route specification](../architecture/rust-migration.md) and
[feature parity matrix](../audit/feature-parity-matrix.md) preserve the larger target.
Planned paths in those documents are not existing modules.

## Legacy modules and their replacements

| Legacy reference | Current Rust replacement or next task |
|---|---|
| [`server/providers/aircraft/adsb-lol.js`](../../legacy/server/providers/aircraft/adsb-lol.js) | [`adsb_lol.rs`](../../crates/lavender-core/src/providers/adsb_lol.rs) |
| [`aircraft/tracks.js`](../../legacy/server/providers/aircraft/tracks.js) | [`aircraft_history.rs`](../../crates/lavender-core/src/aircraft_history.rs): session observations, bounded per aircraft and globally; no upstream backfill. |
| [`aircraft/opensky.js`](../../legacy/server/providers/aircraft/opensky.js), [`enrichment.js`](../../legacy/server/providers/aircraft/enrichment.js) | Not implemented; finish the licensed optional sources in ENG-948. |
| [`server/providers/space/celestrak.js`](../../legacy/server/providers/space/celestrak.js) | [`celestrak.rs`](../../crates/lavender-core/src/providers/celestrak.rs); [`orbit/passes.rs`](../../crates/lavender-geo/src/orbit/passes.rs) predicts geometric observer passes. |
| [`src/layers/earthquakes/source.js`](../../legacy/src/layers/earthquakes/source.js) | [`usgs.rs`](../../crates/lavender-core/src/providers/usgs.rs) replaces the upstream browser-side USGS request. |
| [`server/providers/firms.js`](../../legacy/server/providers/firms.js), [`cyclones.js`](../../legacy/server/providers/cyclones.js) | ENG-950/951; no corresponding provider exists yet. |
| [`server/providers/vessels/`](../../legacy/server/providers/vessels/), [`weather.js`](../../legacy/server/providers/weather.js), [`wind/`](../../legacy/server/providers/wind/) | ENG-953–955. |
| [`server/providers/`](../../legacy/server/providers/), [`server/mcp/`](../../legacy/server/mcp/) | Remaining source/automation migrations in the task index. |

## How compatibility works

The new frontend uses `/api/v1`, shared domain types and generated TypeScript.
It does not depend on legacy `/api/flights` or other Connect mounts. There is no
legacy-route compatibility shim. Preserve **user-visible behavior and useful data**
through fixtures and the parity matrix, rather than copying accidental routing
behavior or insecure input handling.

For a refactor of migrated code, keep existing public Rust signatures and serialized
messages compatible. For a new feed, extend domain types deliberately, regenerate
bindings and update both transport contract tests. See [Add a Provider](add-provider.md).

## Migrate one more module

1. Read its task/comments and both audit rows. Identify inputs, normalized fields,
   quotas, cache behavior, error cases, attribution and licenses before coding.
2. Find upstream tests and legally usable fixtures. Record what the old feature
   actually displays and which fields support that display.
3. Implement a provider inside the existing core. Keep parsing testable without
   networking; use `Fetcher` and cancellation for external requests.
4. Add the feed/type to domain, source registry and core construction; use the
   generic feed API unless a genuinely different operation needs a route.
5. Connect a renderer and interface behavior through `LavenderPlatform`.
6. Test valid/invalid/empty data, unavailable upstream, stale fallback, rate limits,
   cancellation, license refusal and attribution. Compare both old and new outputs
   on the same fixture. Document intentional differences explicitly.
7. Run checks and a fixture-backed analyst journey. Put evidence and remaining
   gaps in Flokzy; update this table and the feature parity matrix.

## When legacy code can go

Keep the frozen reference until the relevant parity rows, provider/error tests,
frontend journeys, notices and release sign-off are verified. Do not delete a
legacy provider simply because its basic Rust feed returns JSON. ENG-1009 depends
on the wider migrations and licensing sign-off. No legacy deletion is part of the
current readability pass.

The final architecture remains the same four cohesive crates and two adapters.
Each additional source normally becomes a module in `lavender-core`, rather than
another crate, generic service framework or wrapper chain.
