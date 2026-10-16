# Functions and Modules Reference

Last updated: 2026-10-14.

Start with application operations; open the linked file for exact signatures and
Rust documentation. Private helper details belong beside their implementation.

Generate the detailed API reference locally with:

```bash
cargo doc --locked --no-deps
```

Open `target/doc/lavender_core/index.html` and `target/doc/lavender_geo/index.html`.
These generated pages are local build output, not committed GitHub documentation.

## Application entry points

All core methods below are in [`core.rs`](../../crates/lavender-core/src/core.rs).
`CoreError` conversions are in [`error.rs`](../../crates/lavender-core/src/error.rs).

| API | Input → output | Errors, side effects and typical caller |
|---|---|---|
| `LavenderCore::builder` / `LavenderCoreBuilder::build` | `CoreConfig`, optional stores/providers/cache directory → `Result<LavenderCore, CoreError>` | Validates all config and initializes the HTTP client; no feed task until requested. Server and desktop startup call it. |
| `catalog` | No arguments → `Catalog` | No failure result; reads registry, providers and secret presence. Called by the layer explorer and Sources panel through adapters. |
| `status` | No arguments → `SystemStatus` | No failure result; reads provider health, region/subscriber counts and licence availability. Called by status polling. |
| `snapshot` / `snapshot_with_budget` | `FeedQuery`, optional shared client `RegionBudget` → async `Result<FeedSnapshot, CoreError>` | Availability/limit/timeout/shutdown errors; starts or shares refresh work and waits for regions. HTTP feed handler and Tauri `feed_snapshot` call it. |
| `subscribe` / `subscribe_with_budget` | `FeedQuery`, optional client budget → `Result<FeedSubscription, CoreError>` | Rejects unavailable/unknown feeds or shutdown; starts a live task. Transport forwarders own the result until unsubscribe. Region capacity errors can also arrive as events. |
| `search` | `&SearchQuery` → async `Result<SearchResults, CoreError>` | Parses coordinates and loaded entities; `places: true` explicitly invokes the configured geocoder. Coordinates never need networking. Disabled/busy/upstream lookup becomes a notice; service commercial terms need acknowledgment. |
| `aircraft_track` | ICAO/TIS-B hex → `Result<AircraftTrack, CoreError>` | Reads retained observations only; invalid address is 400, no retained track is 404. Caps: 4,096 aircraft, 256 points each, 65,536 points overall, 30-minute idle expiry. |
| `satellite_track` | `SatelliteTrackRequest` → async `Result<SatelliteTrack, CoreError>` | Validates steps/time range and satellite ID; may load elements through `snapshot`. Computes up to 1,440 points. Inspector track requests call it. |
| `satellite_passes` | `SatellitePassRequest` → async `Result<SatellitePasses, CoreError>` | Geometric passes for a WGS84 observer: 1–24 hours, elevation 0–89°, observer altitude −500–10,000 m. Two concurrent CPU jobs per core; busy is 429. Window must be within 14 days of the elements. No optical visibility estimate. |
| `settings` | No arguments → `CoreSettings` | Reads the selected store; current document contains only `version`. There is no core update-settings command yet. |
| `shutdown` | No arguments → `()` | Cancels subscriptions and refresh tasks; repeated shutdown is safe. Called on application exit. |

Builder customization (`providers`, `secrets`, `settings`, `cache_dir`, `audit`)
exists for real adapter and test differences. Most application code only needs the
facade above, rather than the hub's internals. The Rustdoc example on `LavenderCore`
is compiled as a doc test.

## Providers and network policy

CelesTrak stores a bounded atomic envelope containing the raw OMM document,
resource URL, ETag/Last-Modified validators and download/revalidation times. An
unchanged 304 renews validation age; a failed refresh preserves it. Legacy raw
JSON caches are still read. Requests remain limited to one per group per two
hours, with the existing ten-minute failed-attempt floor. Conditional validators
are typed and bounded; redirects clear request validators, and responses from a
different final URL expose no validators for persistence against the original
resource. Ordinary Fetcher requests still reject unsolicited 304 responses.
Cache tests use mock HTTP and controlled cache
ages, not live CelesTrak traffic.

Cyclones combine `CurrentStorms.json` positions with ArcGIS forecast points,
tracks and cones from the exact advisory. The global feed covers the Atlantic
and eastern/central North Pacific; a cone describes centre-track uncertainty,
not the extent of wind or rain. A valid empty status skips GIS requests.
Invalid status fails the whole document; unavailable or invalid geometry retains
current status positions with a degraded notice. Earlier geometry is never
relabelled as the current advisory. Cadence and retry floors are at least ten
minutes, and cached observations expire after twelve hours. Limits are 32 storms,
128 KiB status, 512 KiB each for points/tracks, 2 MiB cones, 500 forecast-point
features, 32 track/cone features each, 25,000 geometry vertices overall, 10,000
vertices per line/ring and 128 parts/rings per container. Tests use synthetic
and mock documents; they do not establish live NOAA operational readiness.

Fetcher resolves a hostname once per connection through a bounded resolver
(at most 64 answers) and gives those validated addresses to Reqwest. Empty,
mixed public/private and non-public answer sets fail; literal IPs and redirects
are checked too. Ambient HTTP proxies are disabled. `fetch.allow_http_hosts`
is an explicit development/internal-service exception that also permits private
addresses for those hosts; its default is empty. These exceptions still require
the provider's configured host policy.

| API / file | Purpose and input → output | Errors, side effects and caller |
|---|---|---|
| [`FeedProvider::fetch`](../../crates/lavender-core/src/providers/mod.rs) | `&FetchContext`, `Region` → async `Result<Fetched, ProviderError>`; one source's normalized result | May perform allowed network/cache I/O; honors cancellation. The refresh loop calls it. `Fetched` carries items, original age, degraded reason and notices. |
| [`AdsbLolProvider::fetch` / `adsb_lol::parse`](../../crates/lavender-core/src/providers/adsb_lol.rs) | Fetch regional or military aircraft; parser takes bytes + military-feed flag → `(Vec<Aircraft>, dropped_count)` | Whole-document shape errors fail; invalid rows are skipped. Network pacing happens in Fetcher. Fixtures and provider tests also use the parser. |
| [`UsgsProvider::fetch` / `usgs::parse`](../../crates/lavender-core/src/providers/usgs.rs) | Fetch configured summary; bytes → `(Vec<Earthquake>, dropped_count)` | Shape/transport errors propagate; invalid features are skipped, text cleaned and coordinates validated. |
| [`CelestrakProvider::fetch`](../../crates/lavender-core/src/providers/celestrak.rs) | Configured groups → deduplicated `Fetched` satellite elements | Reuses memory/disk data with actual age; writes successful downloads atomically; partial group errors become degraded results. Each group respects cadence/retry floors. |
| [`celestrak::parse`](../../crates/lavender-core/src/providers/celestrak/parsing.rs) | Bytes + group name → `(Vec<SatelliteElements>, dropped_count)` | Rejects non-array JSON; skips invalid elements. Pure; public export remains `providers::celestrak::parse`. |
| [`CyclonesProvider::fetch`, `parse_status`, `attach_geometry`](../../crates/lavender-core/src/providers/cyclones.rs) | Configured NOAA roots → `Fetched`; pure status/GIS parsers → bounded typed cyclones | Coalesces refreshes, honors cancellation and keeps usable stale data on upstream failure. Strict status errors fail; GIS failures report degraded positions. |
| [`FixtureProvider::all`](../../crates/lavender-core/src/providers/fixture.rs) | Directory path → `Vec<Arc<dyn FeedProvider>>` | Builds recorded-data providers; files are read during fetch. Missing/bad files fail the relevant feed; every result is labelled recorded. Used only by explicit fixture mode. |
| [`Fetcher::new` / `get` / `get_json`](../../crates/lavender-core/src/fetch.rs) | Config; then URL + `HostPolicy` + `FetchOptions` → response bytes or decoded JSON and response metadata | Host/scheme/redirect/timeout/status/size/decode/cancellation failures are `FetchError`. Manages host concurrency, pacing and retry waits; providers call it. Errors avoid full URLs containing keys. |
| `ProviderError::parse` / `public_message` | Source and error, or existing provider failure → safe error/message | Removes controls and caps public text at 200 characters. Refresh/health/error paths call it; no network side effect. |
| [`install_redacting_panic_hook`](../../crates/lavender-core/src/diagnostics.rs) | No arguments → `()` | Changes the process panic hook, emitting source location but suppressing payloads/backtraces. Server/desktop executables opt in before processing; core construction leaves hooks alone. |

Private `normalize_aircraft`, `normalize_earthquake` and `normalize_elements` name
the records they validate. Public `parse` remains understandable within each
provider module and keeps its existing signature.

## Subscriptions, geography and validation

| API / file | Input → output | Errors, effects and caller |
|---|---|---|
| [`FeedSubscription::next`, `latest`, `update`](../../crates/lavender-core/src/hub/subscription.rs) | Await next/latest `SubscriptionEvent`; replace query with `FeedQuery` | `next` ends with `None` after cancellation/shutdown; update rejects a different feed. Dropping the handle releases regions. Web/desktop forwarders call these. |
| [`RegionBudget::new`, `in_use`](../../crates/lavender-core/src/hub/slots.rs) | Limit → budget; read distinct held regions | Constructor clamps minimum to one. Shared per network client on web. Leases account and return holds internally. |
| [`GeoPoint::new`, `BBox::new`](../../crates/lavender-domain/src/geo.rs) | Latitude/longitude, or west/south/east/north → validated type | `DomainError` for invalid numbers/ranges; supports antimeridian boxes. Provider parsers and API decoding call these. |
| [`parse_coordinates`](../../crates/lavender-geo/src/coords.rs) | `&str` → `Option<ParsedCoordinate>` | Decimal/DMS/MGRS; `None` for unreadable input, no I/O. Search calls it. |
| [`plan_cells`](../../crates/lavender-geo/src/region.rs) | `&BBox`, cell degrees, max cells → `RegionPlan` | Bounded planning reports truncation; pure. Hub region planning calls it. |
| [`Propagator::new`, `position`, `ground_track`](../../crates/lavender-geo/src/orbit.rs) | Elements → `Result<Propagator, OrbitError>`; timestamps/range → positions or track points | `new` and `position` return orbit/propagation errors; `ground_track` returns a vector and skips failed points. No I/O. Core track service and reference tests use it. |
| [`ServerConfig::load`, `from_sources`](../../crates/lavender-server/src/config.rs) | Optional TOML path, or TOML + explicit env iterator → validated config | File/parse/all-validation errors; `load` reads process environment/files. Server startup and config tests call it. |

## Frontend entry points

| API / file | Input → result | Failure behavior, effects and caller |
|---|---|---|
| [`createPlatform`](../../apps/web/src/platform/index.ts) | No input → `Promise<LavenderPlatform>` | Chooses web or lazy Tauri adapter once; dynamic import can reject. Composition root calls it. |
| [`LavenderPlatform`](../../apps/web/src/platform/types.ts) | Typed catalog/status/query/track operations → promises; subscribe → update/close handle | Failed calls reject with transport/API errors; feeds report error events. Screens use this contract, not raw HTTP/IPC. |
| [`createRuntime`](../../apps/web/src/runtime/createRuntime.ts) | Platform + optional timing options → runtime | `start` loads catalog/status, persists preferences, starts feeds; returns cleanup. Catalog errors update state/retry; last status is retained on failures. App composition owns cleanup. |
| [`FeedController`](../../apps/web/src/feeds/FeedController.ts) | Platform + stores; `start`, `attachGlobe`, `latest`, `stop` | Subscribes by enabled layer, updates viewport, retains selected records; errors become feed summaries. Runtime owns it. |
| [`GlobeEngine.applySnapshot`](../../apps/web/src/globe/GlobeEngine.ts) | `FeedSnapshot` → `void` | Changes Cesium primitives and requests rendering. FeedController calls it; engine reports rendering/context failures through globe listeners. |
| [`migrateWorkspaceDocument`, `encodeWorkspaceDocument`, `decodeWorkspaceDocument`](../../apps/web/src/state/workspaceDocument.ts) | Unknown document or typed workspace ↔ v2 document / base64url payload | Validates fields and bounded annotations; accepts strict v1 documents with empty annotations. Shared documents remain capped at 5,500 decoded bytes. |
| [`validateAnnotations`](../../apps/web/src/features/annotations/model.ts), [`geometryError`](../../apps/web/src/features/annotations/geometry.ts) | Unknown marks / validated points → bounded marks / validation message | Pure validation: 24 marks, 32 vertices each, 120-character labels, 500-character notes and 64 KiB device annotation payload. Invalid or ambiguous geometry is rejected; distances/areas are approximate. |
| [`surfaceArcPositions`](../../apps/web/src/globe/renderers/surfaceArc.ts) | Longitude/latitude pairs + remaining vertex budget → positions or `null` | Uses Cesium ellipsoid geodesics with raised chords; rejects near-antipodal or invalid segments and consumes budget only on success. Cyclone rendering uses a 100,000 expanded-vertex cap per apply. |

To add a provider, begin with `FeedProvider`, a parser and an existing provider
test. To change an analyst interaction, begin with the feature component and
`FeedController`, then follow the [architecture walkthrough](architecture.md).
