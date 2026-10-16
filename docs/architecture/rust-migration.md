# Lavender — Rust migration plan (Node backend → `lavender-core` / `lavender-server`)

> This is the full migration target and legacy-route inventory. Proposed modules, routes and error categories below are not a claim of implementation. Read [the current migration table](../development/rust-rewrite.md), [API reference](../development/reference.md) and [roadmap](../roadmap.md) for implemented behavior and remaining tasks.


| | |
|---|---|
| Status | v1, 2026-10-11 |
| Implementation notes updated | 2026-10-14; target mapping below remains the full migration inventory |
| Inputs | `docs/audit/upstream-backend.md` (route table §2.0, routes §2.1–2.22, background work §4, secrets §5, MCP/voice §6, tests §7, mapping §8, risks §9); `docs/audit/upstream-layers.md`; `docs/audit/upstream-ui-and-systems.md`; `docs/audit/data-sources-and-licensing.md`; ADRs 0001–0008; `docs/architecture/overview.md` |
| Companion | `docs/audit/feature-parity-matrix.md` (F-ids referenced below) |
| Phases | P1 Foundations · P2 First vertical slices · P3 Feature wave A · P4 Feature wave B · P5 Automation & deferred · P6 Desktop readiness · P7 Optimisation & hardening · P8 Final verification & legacy removal · Roadmap · — (removed) |

## Purpose

Implemented operations now include
`GET /api/v1/aircraft/{hex}/track` (bounded observed session history, no upstream
fetch), `GET /api/v1/satellites/{norad}/passes?observer=lat,lon&hours=24`
(bounded geometric prediction) and explicit `places=true` on `/api/v1/search`.
The core operations are `aircraft_track`, async `satellite_passes` and async
`search`; Tauri exposes matching commands. Limits/error behavior are documented
in [the current API reference](../development/reference.md).

`feed:cyclones` is also implemented through the existing snapshot/subscription
operations, with `providers/cyclones.rs` and strict status/GIS parsing. It adds
typed storm, forecast, track and cone data without a cyclone-specific route.
Browser annotations use workspace v2 with strict v1 migration and local device
persistence; server-stored workspaces and registered native deep links remain
planned. The current public-address resolver is `fetch/dns.rs` over Tokio DNS;
the historical hickory/`PrivateOnly` design in B2 is not an installed dependency
or a claim that local receivers have migrated. Adapter executables now explicitly
install the redacting panic hook in `diagnostics.rs`.

GEV's backend is about 19,000 lines of Node.js. It runs as Connect middleware inside Vite's dev and preview servers and exposes 58 method+path endpoints on 39 mounts, plus a JSON 404 catch-all and a dev-only `/panel/*` mount (backend audit §0, §2.0). It has no production process.

This document says, for every one of those endpoints and every non-route component, where the behaviour goes in Lavender's Rust workspace (ADR 0002):

- a typed `/api/v1` route;
- a feed served by the hub (`feed:<id>`, ADR 0005);
- a `LavenderCore` method;
- or a deliberate removal.

It also covers what stays in Node or TypeScript, how logic is split between Rust and the TS renderer, how each provider is verified, and the main risks. It extends the route table in `overview.md` §2.4. Each route is added when its phase lands.

## Principles

1. **No Connect-routing quirks are reproduced.** Connect matched mounts as case-insensitive prefixes, also at a `.` boundary, and many handlers served any sub-path or any method (backend audit §1.6). Lavender routes are exact, case-sensitive and versioned under `/api/v1`. A wrong method returns 405 with `Allow`. Query strings go through typed extractors that reject unknown keys with 400. The legacy `/api/*` paths get no compatibility shim: the frontend is new, so nothing depends on them.
2. **Correct status codes, one error envelope.** Every error is `ApiError { code, message, retryable }` (overview §2.1) with the status below. Legacy oddities (backend audit §9.11) are dropped on purpose.
3. **Every upstream read is bounded.** It has a connect timeout, a total timeout, a streamed body cap (`read_capped`), per-host concurrency and a redirect policy. Defaults are a 5 s connect timeout, a 15 s total timeout and an 8 MiB body cap. Per-provider overrides are listed in §B. Every upstream that the backend audit lists as uncapped or missing a timeout gets both (§9.8).
4. **No URL is ever taken from request parameters.** Base URLs come from configuration. Clients send ids (feed, registry system, camera, tile z/x/y), never URLs or upstream coordinates for paid lookups. This removes the GBFS client-URL route (#23) and the Street View lookup at a client-chosen location (#49).
5. **Caches are bounded and rooted.** Memory caches have a maximum weight. Disk caches live only under the adapter-supplied `CacheDir` (`app_cache_dir()` on desktop), are namespaced per provider and schema version, written atomically and swept to a size cap. Nothing resolves against the working directory (backend audit §9.7, §9.12). Persistent state that must survive cache clearing, such as the TomTom daily budget counter, lives in the data dir.
6. **Secrets follow ADR 0006.** They enter through `SecretStore` (env and `*_FILE` on the server, OS keychain on desktop) and are held as `secrecy::SecretString`. They are never logged; `reqwest` errors are stripped of URLs, because FIRMS and TomTom carry keys in the URL. Secrets never reach a JS bundle. The web server exposes no route that writes secrets. Browser-usable tile keys are delivered at runtime through the catalog, and only when an operator marks them `public_*`.
7. **The licence gate follows ADR 0008.** Every provider registers a `SourceInfo`. In `licence_mode = commercial` the core refuses any source whose `commercial` is not `Allowed`, unless the operator has acknowledged it. `privacy: Sensitive` sources (CCTV, ALPR) are off by default in every mode. A refused source reports `disabledByLicence` and never touches its upstream.
8. **Live data is a feed, not a route.** Polled data becomes a hub feed: one refresh task per active (feed, region key), shared by all subscribers and cancelled when the last one leaves (ADR 0005). Provenance and freshness travel inside the snapshot (`source`, `observedAt`, `fetchedAt`, `freshness`, `notices`), not in `X-*` headers.
9. **Cancellation is end to end.** When the last waiter of a shared upstream operation leaves (HTTP client disconnect, WebSocket unsubscribe, Tauri window close), the upstream request is aborted. This preserves the waiter-refcount behaviour of weather, wind, cyclones and Mapillary (backend audit §3).
10. **Two transports, one core.** On desktop, every JSON route below is a Tauri command with the same request and response types (ADR 0007). Feed subscriptions use `tauri::ipc::Channel`. Binary and tile routes are served by a `lavender://` custom URI-scheme handler that calls the same core method, so Cesium can fetch them by URL.

### Status-code rules

| Situation | Legacy behaviour | Lavender |
|---|---|---|
| Malformed or out-of-range parameter | Mostly 400; terrain >2,000 points → 500; route validation → 200 `{ok:false}` | 400 `invalid_argument` |
| Unknown entity id (camera, grid, station) | Mixed: 404, or a Street View fallback | 404 `not_found` |
| Wrong method | Many routes accept any method | 405 with `Allow` |
| Request body too large / wrong content type | 413 / 415 on some routes | 413 `payload_too_large` / 415 `unsupported_media_type` |
| Valid request with no possible answer (no route, leg too long) | 200 `{ok:false}` | 422 `unprocessable` |
| Client over its rate limit | 429 with `Retry-After` | 429 `rate_limited` with `Retry-After` |
| Server capacity full (operation queue, waiter cap) | 429 `weather_busy` / `cyclone_busy` | 503 `busy` with `Retry-After` |
| Server-side quota exhausted (TomTom daily budget, voice budget) | 429 `budget` | Serve stale with a notice; otherwise 503 `quota_exhausted` with `Retry-After` |
| Provider has no key | 503 `no_key`, 200 `configured:false`, or 503 with a normal body | **Feeds:** 200 snapshot with `freshness: unconfigured`. **Other resources:** 503 `provider_unconfigured`, `retryable:false` |
| Provider refused by licence policy | n/a | **Feeds:** 200 snapshot with `freshness: disabledByLicence`. **Other resources:** 403 `disabled_by_licence` |
| Upstream failed, older data available | 200 with `X-*-Stale` headers | **JSON:** 200 with `freshness: stale { ageS }`. **Binary/tiles:** 200 with an `Age` header |
| Upstream failed, nothing to serve | 502 / 503 / 200 `unavailable:true` | 502 `upstream_failed` or 504 `upstream_timeout`; a feed that has never succeeded returns 503 `feed_unavailable` (`retryable:true`) |
| Internal fault | 500 with varying bodies | 500 `internal` (no detail; logged with a request id) |

---

## A. Route-by-route mapping

All 58 legacy endpoints from `docs/audit/upstream-backend.md` §2.0 are listed, plus the catch-all (C1) and the dev-only panel mount (C2). Handler paths are relative to `products/lavender/`. Core modules are under `lavender_core::providers::` unless written in full. A "Removed — folded into X" row ships in X's phase. A row removed with no replacement has phase "—".

### A.1 Mapping table

| # | Legacy method+path | Legacy handler | Target | Core module | Behaviour changes vs legacy | Phase |
|---|---|---|---|---|---|---|
| 1 | any `/api/flights` | `legacy/server/providers/aircraft/opensky.js:383` | `feed:aircraft` | `aircraft::{adsb_lol, opensky}` | GET only. Snapshot is bbox-filtered, not a global OpenSky passthrough. adsb.lol is primary (regional keys); OpenSky is optional and licence-gated. `X-OpenSky-*` / `X-Flight-*` headers move into `source`, `freshness`, `notices` and `/status`. OpenSky body capped. Items are normalised `Aircraft` (SI units), not 18-field state vectors | P2 |
| 2 | any `/api/flights/track?icao24=` | `legacy/server/providers/aircraft/tracks.js:68` | `GET /api/v1/aircraft/{hex}/track` | `aircraft::tracks` | Path parameter `^[0-9a-f]{6}$`. Source chosen by registry (OpenSky only if licensed, else the adsb.lol trace). Upstream errors are no longer cached for 60 s (short separate negative cache). Typed `Track` body; 404 when no track exists | P3 |
| 3 | any `/api/military` | `legacy/server/providers/aircraft/adsb-lol.js:71` | `feed:aircraft` with `filters.military = only` | `aircraft::adsb_lol` | Merged into the aircraft feed as a `military` flag. Timeout and body cap added (legacy had neither). `Retry-After` cooldown kept (5–120 s). A cold-cache upstream failure becomes `degraded` freshness instead of relayed upstream status | P2 |
| 4 | any `/api/military/track?hex=` | `legacy/server/providers/aircraft/tracks.js:96` | `GET /api/v1/aircraft/{hex}/track` | `aircraft::tracks` | Same resource as #2. Non-ICAO addresses accepted as `^~?[0-9a-f]{6}$` (legacy allowed `~` anywhere). The undocumented tar1090 trace path sits behind a provider flag | P3 |
| 5 | any `/api/adsbdb/route/:callsign` | `legacy/server/providers/aircraft/enrichment.js:111` | `GET /api/v1/aircraft/{hex}/info?callsign=` | `aircraft::enrichment` | Type and route returned together. Licence-gated (`RequiresAgreement`): 403 `disabled_by_licence` in commercial mode. Bounded memory cache and a capped disk cache under `CacheDir` (legacy maps and `.gev-cache/adsbdb.json` grew without bound). Registration suppressed for general aviation by default (LADD/PIA) | P3 |
| 6 | any `/api/adsbdb/type/:hex` | `legacy/server/providers/aircraft/enrichment.js:111` | `GET /api/v1/aircraft/{hex}/info` | `aircraft::enrichment` | As #5. One typed resource replaces the path switch on `route`/`type` | P3 |
| 7 | any `/api/vessels?maxRows=` | `legacy/server/providers/vessels/ais-live.js:74` | `feed:vessels` | `ais::{stream, store}` | bbox is applied (R-tree); legacy ignored it. `maxRows` replaced by server caps per view span. Keyless → `unconfigured` snapshot, not 503 with a body. Watchdog fields (`silentForMs`, `reconnectAttempt`, …) move to `/status`. Requests no longer drive the watchdog: the ingest task starts with the first subscriber and stops after a grace period | P3 |
| 8 | any `/api/vessels/track?mmsi=` | `legacy/server/providers/vessels/ais-live.js:83` | `GET /api/v1/vessels/{mmsi}/track` | `ais::store` | Validation `^\d{5,10}$` kept; errors use `ApiError` instead of `{error, samples:[]}`. Body states source and retention | P3 |
| 9 | any `/api/celestrak/:group` | `legacy/server/providers/space/celestrak.js:70` | `feed:satellites`; ground tracks via `GET /api/v1/satellites/{norad}/track` | `space::celestrak` (+ `lavender_geo::sgp4`) | Clients no longer name arbitrary groups; the group set is configured. OMM JSON replaces TLE text (TLE cannot carry catalogue numbers above 99999). Body cap added. `x-tle-cache` becomes freshness. Elements are delivered; the frontend propagates positions | P2 |
| 10 | GET `/api/launches` | `legacy/server/providers/space/launch-library.js:92` | `feed:launches` | `space::launch_library` | Normalised `Launch` items instead of an LL2 passthrough. Payload→NORAD matches are included, which removes the browser download of `/api/celestrak/active`. `X-GEV-Cache` becomes freshness. Disk cache under `CacheDir` | P3 |
| 11 | GET `/api/weather/manifest?product=` | `legacy/server/providers/weather.js:473` | `feed:weather-radar`, `feed:weather-satellite`, `feed:weather-lightning` | `weather::nowcoast` | One feed per product instead of `?product=`. Upstream failure becomes `stale`/`unavailable` freshness, not a 200 manifest with `unavailable:true`. Manifest lists advertised times and the tile URL template | P3 |
| 12 | GET `/api/weather/tile` | `legacy/server/providers/weather.js:484` | `GET /api/v1/tiles/weather/{product}/{time}/{z}/{x}/{y}.png` | `weather::nowcoast` | Path parameters replace the query allowlist. Kept as a tile route, although nowCOAST is public, for three reasons: NOAA asks for limited request rates, so one shared byte-bounded cache serves all viewers; `time` must be an advertised time; and the global IR mosaic is sliced server-side. A full queue returns 503 `busy` (legacy 429) | P3 |
| 13 | GET `/api/weather/image` | `legacy/server/providers/weather.js:453` | Removed — folded into #12 (mosaic sliced into tiles; filtered-alpha variant via `?alpha=filtered`) | `weather::nowcoast` | The browser no longer slices 2048×1024 images or runs the IR alpha pass on the main thread | P3 |
| 14 | GET `/api/wind`, `/api/wind/manifest` | `legacy/server/providers/wind.js:187` | `feed:wind` with `filters: {model, overlay}` | `wind::{gfs, ifs}` | Unavailable and stale states become freshness. `gridUrl` points to `/api/v1/wind/grids/{id}.bin`. Invalid model or overlay → 400 | P3 |
| 15 | GET `/api/wind/status` | `legacy/server/providers/wind.js:212` | Removed — folded into `/api/v1/status` and feed freshness | `lavender_core::health` | — | P3 |
| 16 | GET `/api/wind/grid/:id.bin` | `legacy/server/providers/wind.js:166` | `GET /api/v1/wind/grids/{id}.bin` | `wind` + `lavender_core::grib` | Float32 is explicitly **little-endian** (legacy used host endianness). Layout kept (U, V, optional scalar; dimensions in the manifest) so the TS renderer ports unchanged. `ETag` is the grid id; `immutable` caching kept; unknown id → 404 | P3 |
| 17 | GET `/api/cyclones` | `legacy/server/providers/cyclones.js:373` | `feed:cyclones` | `cyclones::nhc` | No "path must be exactly `/`" check (typed route). Failure becomes freshness, not 200 `unavailable:true`. More than 32 waiters → 503 `busy` (legacy 429). Validation rules kept exactly | P2 |
| 18 | any `/api/firms` | `legacy/server/providers/firms.js:177` | `feed:fires` | `fires::firms` | GET only. bbox and LOD aggregation (cells by view span, top-N) replace whole-world JSON. Sources fetched concurrently with body caps (legacy: sequential, uncapped). Keyless → `unconfigured` (legacy 503 `no_key`). Partial success → `degraded` with a notice | P2 |
| 19 | any `/api/firms/status` | `legacy/server/providers/firms.js:191` | Removed — folded into `/api/v1/status` (key presence, last fetch, transactions used/limit) | `lavender_core::health` | Reports presence only, never the key | P2 |
| 20 | GET `/api/fire-perimeters` | `legacy/server/providers/firePerimeters.js:191` | `feed:fire-perimeters` | `perimeters::wfigs` | bbox filter and per-zoom simplification. Items include a verified InciWeb link. `X-Data-Stale` becomes freshness. The per-IP limit moves to the shared limiter | P3 |
| 21 | GET `/api/fire-perimeters/inciweb/index` | `legacy/server/providers/firePerimeters.js:195` | Removed — core-internal (InciWeb catalogue cached 1 h and joined into #20 items) | `perimeters::inciweb` | The browser no longer matches incident names | P3 |
| 22 | GET `/api/fire-perimeters/inciweb/publication/:id` | `legacy/server/providers/firePerimeters.js:199` | Removed — core-internal link verification | `perimeters::inciweb` | Scrape stays bounded (2 MiB, 15 s, one same-host redirect, https upgrade) | P3 |
| 23 | GET `/api/gbfs/:encodedUpstreamUrl` | `legacy/server/providers/gbfs.js:118` | `feed:bikeshare` with `filters.system = <registry id>` | `bikeshare::gbfs` | **The client no longer supplies a URL**: a registry id resolves to a configured `gbfs.json`. Discovery plus per-feed `ttl` caching (legacy had no cache). v2/v3 normalisation; bbox filter | P4 |
| 24 | any `/api/tomtom/status` | `legacy/server/providers/traffic.js:182` | Removed — folded into `/api/v1/status` (key presence, daily budget used/cap) | `lavender_core::health` | — | P4 |
| 25 | any `/api/tomtom/flow/:z/:x/:y.pbf` | `legacy/server/providers/traffic.js:194` | `GET /api/v1/tiles/traffic/{z}/{x}/{y}` | `traffic::{tomtom, roads}` | Returns compact roads-with-flow (OpenFreeMap roads matched to TomTom flow in the core) instead of raw TomTom MVT; the key never leaves the core. The daily budget is persisted in the data dir. Over budget: stale tile, else 503 `quota_exhausted`. Disk cache swept (legacy never swept) | P4 |
| 26 | GET `/api/transit/feeds` | `legacy/src/sources/transitService.js:242` | Removed — folded into `/api/v1/catalog` (feeds as sources and layer options) | `transit` + `lavender_core::registry` | Licence and attribution come from the registry | P4 |
| 27 | GET `/api/transit/vehicles/:feedId` | `legacy/src/sources/transitService.js:268` | `feed:transit` | `transit::gtfs_rt` | Background polling per active (feed, region) instead of request-driven fetches. bbox filtering; feeds activate by view proximity. `X-GEV-Cache` / `X-Transit-*` headers become freshness and health. Upstream backoff becomes `degraded`, not 503 | P4 |
| 28 | GET `/api/transit/trail/:feedId/:vehicleId` | `legacy/src/sources/transitService.js:250` | `GET /api/v1/transit/{feedId}/vehicles/{vehicleId}/trail` | `transit::history` | Same ≤32 KiB contract, typed. 404 for feeds without retention | P4 |
| 29 | any `/api/terrain/heights?points=` | `legacy/server/providers/terrain.js:159` | `POST /api/v1/terrain/heights` | `terrain::reearth` | JSON body. Latitude and longitude range-checked (legacy did not). More than 2,000 points → 413 (legacy 500). Bounded caches; body cap. A point without a height still → 502. Response adds geoid and ground values | P3 |
| 30 | GET `/api/google/nearby-places` | `legacy/server/providers/places/google.js:66` | Deferred: `GET /api/v1/places/nearby` | `places::google` | Only if a GMP review passes (F-077). Timeout and body cap added. Keyless → 503 `provider_unconfigured` (legacy 200 `configured:false`) | P5 |
| 31 | GET `/api/google/text-search` | `legacy/server/providers/places/google.js:180` | `GET /api/v1/search` (Google place provider) | `lavender_core::search` + `places::google` | Enabled only while the Google basemap is active (GMP). Timeout added. Results normalised into `SearchResults.places` | P3 |
| 32 | any `/api/route` | `legacy/server/providers/places/routes.js:229` | `GET /api/v1/routes?profile&coords&steps` | `routing::osrm` | GET only. Validation errors → 400; "no route" or a leg over the limit → 422 (legacy 200 `{ok:false}`). Base URL configurable (self-hosted OSRM/Valhalla). Pacer and bounded queue kept | P4 |
| 33 | GET `/api/geocode` | `legacy/server/providers/regional/place.js:191` | `GET /api/v1/search?q=` | `lavender_core::search` + `geocode::nominatim` | `SearchResults` replaces the Google-geocoder-shaped body. Endpoint configurable (self-hosted). The pacer applies per upstream | P2 |
| 34 | GET `/api/mapillary/status` | `legacy/server/providers/mapillary/routes.js:90` | Removed — catalog `configured` flag | `lavender_core::registry` | — | P4 |
| 35 | GET `/api/mapillary/tiles/coverage/:z/:x/:y` | `legacy/server/providers/mapillary/routes.js:91` | `GET /api/v1/tiles/mapillary/{z}/{x}/{y}.pbf` | `mapillary` | Same zoom set, layer stripping, caches and upstream holds; 204 on empty kept. The same-site gate becomes an origin check plus rate limit | P4 |
| 36 | GET `/api/overpass/status` | `legacy/server/providers/overpass.js:79` | Removed — folded into `/api/v1/status` | `lavender_core::health` | — | P4 |
| 37 | POST `/api/overpass` | `legacy/server/providers/overpass.js:91` | Removed as a raw-QL endpoint. Typed core queries replace it: installations (#38), `GET /api/v1/geometry/resolve`, and the `get_map_features` command (P5) | `overpass` (sanitizer in `lavender_domain::overpass_ql`) | Browsers can no longer send QL. The sanitizer is ported with its test corpus and applied to every generated query. Failover, cooldowns and caches kept; disk caches swept (legacy never swept) | P4 |
| 38 | GET `/api/military-installations` | `legacy/server/providers/military-installations.js:85` | `feed:military-installations` | `installations` + `overpass` | bbox comes from the subscription (≤10° span, no antimeridian crossing, as legacy). Without Overpass, the OpenFreeMap tier is decoded in the core (legacy: in the browser). Names R-tree for wide views. `X-Military-Installations` becomes freshness | P4 |
| 39 | GET `/api/regional-brief` | `legacy/server/providers/regional/briefing.js:73` | `GET /api/v1/regions/brief?lat&lon` | `regional` | Google News removed. The free Open-Meteo endpoint is replaced by a configurable licensed weather provider. The regex RSS parser is gone. Status values map to freshness plus per-part status | P4 |
| 40 | GET `/api/weather-effects` | `legacy/server/providers/regional/weather-effects.js:46` | Removed (cockpit clouds removed, F-138) | — | — | — |
| 41 | GET `/api/radio/stations` | `legacy/server/providers/radio/catalog.js:346` | `feed:radio-stations` | `radio` | bbox filtering. DNS pinning, health policy and catalogue generations kept | P5 |
| 42 | POST `/api/radio/click/:uuid` | `legacy/server/providers/radio/catalog.js:374` | `POST /api/v1/radio/stations/{uuid}/click` | `radio` | Same 204 fire-and-forget; the id must be in the served catalogue | P5 |
| 43 | any `/api/cctv/sources` | `legacy/server/providers/cctv.js:144` | `feed:cctv` | `cctv::catalog` | Served by bbox (legacy sent the whole ~4,000-camera catalogue). Off until both operator and user enable it (`Sensitive`); unknown-terms packs default off. Ground heights come from open terrain, not the Google-derived sidecar | P4 |
| 44 | any `/api/cctv/health` | `legacy/server/providers/cctv.js:181` | Removed — folded into `feed:cctv` (per-camera `status`) | `cctv::catalog` | No separate 7 s poll | P4 |
| 45 | any `/api/cctv/stream/:id` | `legacy/server/providers/cctv.js:190` | `GET /api/v1/cctv/{id}` | `cctv` | GET only. Returns the media kind and Lavender URLs. Unknown id → 404 | P4 |
| 46 | GET `/api/cctv/media/:id` (non-HLS relay, or HLS playlist with `?lease=`) | `legacy/server/providers/cctv.js:204` | `GET /api/v1/cctv/{id}/media` (non-HLS) and `GET /api/v1/cctv/{id}/hls/playlist.m3u8?lease=` | `cctv::{media, hls}` | Redirects pinned to the registered origin (legacy followed any, §9.9). Live bodies get a maximum session duration in addition to the 30 s idle deadline. Per-user concurrent-stream cap. Range sanitiser kept | P4 |
| 47 | GET `/api/cctv/media/:id/seg_:n.ts` | `legacy/server/providers/cctv.js:248` | `GET /api/v1/cctv/{id}/hls/seg_{n}.ts?session&lease` | `cctv::hls` | Same in-memory segments, caps and leases | P4 |
| 48 | DELETE `/api/cctv/media/:id?lease=` | `legacy/server/providers/cctv.js:223` | `DELETE /api/v1/cctv/{id}/hls/leases/{lease}` | `cctv::hls` | Lease moves into the path; still 204 | P4 |
| 49 | any `/api/cctv/frame/:id` | `legacy/server/providers/cctv.js:462` | `GET /api/v1/cctv/{id}/frame` | `cctv::media` | Street View fallback removed, together with the client `lat/lon/heading/fov/pitch` parameters. Unknown id → 404. The SVG placeholder moves to the client. Rate-limited (legacy was ungated, §9.3). NSW frames through the Open Data Hub API, without User-Agent spoofing | P4 |
| 50 | GET/HEAD `/api/local-receivers/aircraft` | `legacy/server/providers/local-receivers.js:507` | `feed:local-adsb` | `local_receivers` | Pushed instead of polled every 1 s. DNS pinning to private addresses kept. Beast and SBS sockets added. Feed URLs are still never returned | P5 |
| 51 | GET/POST `/api/realtime/token?auth=&tier=` | `legacy/server/providers/openai/realtime.js:130` | `POST /api/v1/voice/sessions` | `openai::realtime` | API-key lane only (`auth=oauth` removed). Authentication required. Budget, token TTL and idle timeout enforced server-side. Tool list generated from the command registry. `X-GEV-Voice-*` headers move into the response body | P5 |
| 52 | GET `/api/realtime/oauth-status` | `legacy/server/providers/openai/realtime.js:56` | Removed (Codex OAuth dropped, F-160) | — | — | — |
| 53 | POST `/api/realtime/oauth-login` | `legacy/server/providers/openai/realtime.js:82` | Removed (no process spawning) | — | — | — |
| 54 | POST `/api/realtime/debug-log` | `legacy/server/providers/openai/debug-log.js:115` | `POST /api/v1/voice/diagnostics` | `lavender_core::diagnostics` | Keys, bearer tokens, `ek_` secrets and data URLs redacted server-side (legacy redacted only on the client, §9.10). Retention period. Log in the data dir, not the source root | P5 |
| 55 | POST `/api/openai/hud-summary` | `legacy/server/providers/openai/hud-summary.js:31` | Removed (F-161) | — | — | — |
| 56 | GET `/api/setup/status` (dev only) | `legacy/server/standalone/key-setup.js:248` | Removed on web (catalog `configured` flags serve the UI). Desktop: Tauri command `provider_key_status` (presence only) | `lavender_core::secrets` | Never returns values (kept) | P6 |
| 57 | POST `/api/setup/keys` (dev only) | `legacy/server/standalone/key-setup.js:256` | Removed on web. Desktop: Tauri commands `provider_key_set` / `provider_key_clear` writing the OS keychain | `lavender_core::secrets` (keyring store in `apps/desktop`) | No `.env` rewriting and no server restart; affected providers reload. Validation rules kept (allowlisted names, ≤512 printable characters) | P6 |
| 58 | POST `/mcp` | `legacy/server/mcp/plugin.js:73` | `lavender-mcp` crate: stdio binary plus optional `POST /mcp` | `lavender-mcp` over `LavenderCore` | Bearer authentication added (even locally). Tools allow-listed from the command registry. No `panel_request` tunnel. Per-tool audit log and rate limits | P5 |
| C1 | any `/api/*` unmatched | `legacy/server/standalone/api-not-found.js:4` | Unknown `/api/v1/*` → 404 `ApiError{code:"not_found"}`; any other `/api/*` → 404 | — (router fallback) | Exact, case-sensitive matching; `Cache-Control: no-store` kept | P1 |
| C2 | GET `/panel/*` (dev only) | `legacy/build/panel.js:58` | Removed (MCP Apps panel deferred, F-164) | — | — | — |

### A.2 New routes with no legacy endpoint

| Route | Purpose | Replaces | Phase |
|---|---|---|---|
| `GET /healthz` | Liveness, no dependencies | — | P1 |
| `GET /api/v1/status` | `SystemStatus`: per-source health, key presence, quotas, licence mode, versions | #15, #19, #24, #34, #36 and every `X-*` cache/provider header | P1 |
| `GET /api/v1/catalog` | Layers, sources (attribution, licence, privacy), imagery and terrain descriptors, operator-marked public keys | #26, #34, Vite `define` key injection (`legacy/build/vite.js:107-114`) | P1 |
| `GET /api/v1/feeds/{feed}` and `GET /api/v1/stream` (WebSocket) | Snapshot (with `ETag`) and multiplexed subscriptions for every feed in §A.3 | Per-layer client polling | P1 |
| `GET /api/v1/satellites/{norad}/track` | Ground track and positions from SGP4 in the core | Per-frame browser work for tracks | P2 |
| `GET /api/v1/satellites/{norad}/passes` | Pass prediction for an observer point | Headless `legacy/src/data/satellitePass.js` | P3 |
| `GET /api/v1/imagery/google/token` | Short-lived Google Map Tiles token, budgeted | The dormant client path `/api/google/tiles-token` (`legacy/src/maps/googleTokens.js`) | P3 |
| `GET /api/v1/regions/at`, `GET /api/v1/regions/lookup` | Point-in-polygon and name lookup over Natural Earth, Census and neighbourhood data | ~8.7 MB browser download (`legacy/src/data/naturalEarthRegions.js`, `adminBoundaries.js`) | P3 |
| `GET /api/v1/nearby?kind&id&radiusKm` | Proximity roster over hub stores | Military awareness reading sibling layers (`legacy/src/layers/awareness/`) | P4 |
| `GET /api/v1/geometry/resolve?q&kind` | Name → geometry for annotations and framing | Browser-side resolver egress (`legacy/src/annotations/resolver.js`) | P4 |
| `GET /api/v1/imagery/recent?bbox&days` | CMR granule search, day grouping, Worldview presence probe | Browser-direct `cmr.earthdata.nasa.gov` and `wvs.earthdata.nasa.gov` (`legacy/src/layers/recentImagery/catalog.js`, `model.js`) | P4 |

Browser-direct data calls become feeds as well: USGS (`legacy/src/layers/earthquakes/source.js:3`) becomes `feed:earthquakes`; bundled datacenters, dams and cables become static feeds. Map tiles (GIBS, Google 3D, ion, Esri, OSM/XYZ, Re:Earth terrain mesh, MapillaryJS imagery) remain the only browser egress, limited to catalog-listed hosts (ADR 0008).

### A.3 Feed catalogue

| `FeedId` | Provider module(s) | Scope | Default cadence | Notes | Phase |
|---|---|---|---|---|---|
| `aircraft` | `aircraft::adsb_lol` (regional + `/v2/mil`), `aircraft::opensky` (evaluation) | Regional (quantised point-radius); OpenSky global | 10 s adsb.lol; adaptive 9–300 s OpenSky | Civil and military merged by hex | P2 |
| `satellites` | `space::celestrak` | Global | Elements checked every 2 h (CelesTrak updates each GP set at most once per cycle) | Elements, not positions | P2 |
| `earthquakes` | `earthquakes::usgs` | Global | 60 s | Was browser-direct | P2 |
| `fires` | `fires::firms` | Global, aggregated per view | Upstream 30 min; LOD per subscription | Key-gated | P2 |
| `cyclones` | `cyclones::nhc` | Global | 5 min | Last-good ≤12 h | P2 |
| `vessels` | `ais::{stream, store}` | Global store, bbox views | View snapshot every 5 s; ingest continuous | Licence `Unknown` | P3 |
| `launches` | `space::launch_library` | Global | 15 min | Payload matching | P3 |
| `weather-radar`, `weather-satellite`, `weather-lightning` | `weather::nowcoast` | Global | 120 s (radar, IR); 600 s (lightning) | Manifests; tiles via route | P3 |
| `wind` | `wind::{gfs, ifs}` | Global | 1 h | Manifest; grids via route | P3 |
| `fire-perimeters` | `perimeters::{wfigs, inciweb}` | Global, bbox views | 5 min | Simplified per zoom | P3 |
| `datacenters`, `dams` | `infrastructure` (static) | Global, bbox views | Static | ODbL | P3 |
| `military-installations` | `installations`, `overpass` | Regional (≤10° span) | On view change; 5 min fresh | Three tiers in the core | P4 |
| `submarine-cables` | `infrastructure` (static) | Global | Static | Licence-gated | P4 |
| `transit` | `transit::gtfs_rt` | Regional (per feed) | 15 s | Polls only while subscribed | P4 |
| `bikeshare` | `bikeshare::gbfs` | Regional (per system) | Per feed `ttl` (status ≈60 s) | Registry ids | P4 |
| `cctv` | `cctv::catalog` | Global catalogue, bbox views | Catalogue 15 min; status 30 s | `Sensitive`, off by default | P4 |
| `local-adsb` | `local_receivers` | Local | 1 s | Desktop-first | P5 |
| `radio-stations` | `radio` | Global, bbox views | 45 min | Deferred feature | P5 |

---

## B. Non-route backend components

Size: **S** ≤ 1 week, **M** 1–2 weeks, **L** 2–4 weeks, **XL** more than 4 weeks or research-dependent (one engineer, including tests). Risk is the chance that parity or the schedule slips.

### B.1 Components carried into Rust

| # | Component | Legacy evidence | Rust design | Crates | Size | Risk | Phase |
|---|---|---|---|---|---|---|---|
| B1 | Fetcher (egress policy) | `legacy/src/sources/httpBody.js:6-109`, `legacy/server/providers/common/http.js:17-53`, per-route timeouts (backend audit §3) | `lavender_core::fetch::Fetcher`: per-provider `HostPolicy` (allowlist, HTTPS only); redirect policy none or same-origin-pinned with a hop limit; connect and total timeouts; `read_capped(resp, max, cancel)` over `chunk()` that rejects oversized `Content-Length` first; per-host `Semaphore`; jittered retries for idempotent GETs; Lavender User-Agent with contact; URLs stripped from errors | reqwest (rustls, gzip, stream), tokio, bytes | M | Low | P1 |
| B2 | DNS-checked resolver | `legacy/server/providers/radio/transport.js:13-145` (public-only), `legacy/server/providers/local-receivers.js:142-268` (private-only) | `reqwest::dns::Resolve` over hickory with two policies: `PublicOnly` (radio, operator-configured upstreams such as Overpass and CCTV source files) and `PrivateOnly` (local receivers). Every resolved address is checked and the connection uses the checked address | hickory-resolver, ipnet | M | Medium (IPv6 and mapped-address edge cases) | P1 (`PublicOnly`), P5 (`PrivateOnly`) |
| B3 | Memory caches | Module-global maps in most `legacy/server/providers/*` | moka `future::Cache` with a weigher, maximum capacity and TTL; `try_get_with` gives single-flight loading; negative caching explicit and short | moka | S | Low | P1 |
| B4 | Single-flight with waiter-refcount cancellation | `legacy/server/providers/weather.js:267-312`, `legacy/server/providers/wind.js:38-210`, `legacy/server/providers/cyclones.js:320-354`, `legacy/server/providers/mapillary/tiles.js:444-493`, `legacy/src/sources/httpBody.js:115-126` | `flight::Group<K, V>`: a `Shared` future, an `Arc` waiter count and a `CancellationToken`. Each waiter holds a drop guard (an Axum handler future is dropped on client disconnect); the last guard cancels the upstream. Caps on distinct in-flight keys and waiters per key return `busy` | futures, tokio-util | M | Medium (cancel safety) | P1 |
| B5 | Disk cache (`CacheDir`) | `.gev-cache/` files resolved against the working directory (backend audit §4.3) | Rooted at the adapter-supplied directory; namespaced per provider and schema version; temp file plus rename; size cap with LRU sweep (on write, rate-limited); corrupt entries ignored; never stores secrets or Google content | tempfile, tokio::fs, sha2 (key hashing) | M | Low | P1 |
| B6 | Feed hub | None upstream (each layer polls its own route) | (feed, region key) → refresh task plus `tokio::sync::watch`; ref-counted subscriptions; per-subscriber bbox filtering; cancellation on the last unsubscribe (with a grace period for stores such as AIS); caps on total tasks and on subscriptions per connection | tokio, tokio-util | L | Medium | P1 |
| B7 | Health and status | `X-*` headers and per-route status endpoints | Per-source success, failure and latency ring; `Freshness` derivation; quota counters (FIRMS transactions, TomTom budget, OpenSky credits) | — | S | Low | P1 |
| B8 | Source registry and licence gate | `legacy/DATA_SOURCES.md`, `legacy/src/data/dataCredits.js` | Static `SourceInfo` per provider; `licence_mode`; `licence_acknowledged`; privacy class; a unit test fails when attribution or licence is missing | — | S | Low | P1 |
| B9 | Inbound gates and limiters | `legacy/build/allowedHosts.js:42-93`, `legacy/src/localRequestGate.mjs:28-154`, `legacy/src/sources/rateLimit.js:1-31`, `legacy/server/providers/common/rate-limit.js:21-62` | Tower layers: host check first; origin / same-site check on cost-bearing routes and the WebSocket; sliding-window log limiter per key with a global backstop and a 2,000-key cap, hand-rolled to keep the legacy semantics (GCRA behaves differently); the client IP is the socket address unless a trusted proxy is configured | tower, axum | S | Low | P1 |
| B10 | Outbound pacers and budget governors | Nominatim pacer `legacy/server/providers/regional/place.js:25-43`; OSRM gate `legacy/server/providers/places/routes.js:41-98`; transit admission `legacy/src/data/transitProxy.js:56-67`; TomTom budget `legacy/server/providers/traffic.js:46-102`; OpenSky credits `legacy/server/providers/aircraft/opensky.js:82-88`, `:545-559` | `Pacer` (minimum interval, bounded queue, abandon on cancel); `Admission` (per key plus global); `Budget` (UTC-day counter persisted in the data dir); adaptive TTL from credit headers; cooldown ladders | tokio::time, jiff | M | Medium (budget persistence) | P1 (framework); with each provider |
| B11 | Secret store | `.env` via `loadEnv` (`legacy/server/standalone/vite.config.js:14-17`); key setup | `SecretStore` trait: `EnvSecretStore` (env plus `*_FILE`), `MemorySecretStore` (tests), keyring store (desktop); `SecretKey` enum; `secrecy::SecretString`; redaction in tracing | secrecy; keyring (desktop) | S | Low | P1 (server), P6 (keychain) |
| B12 | Configuration | Every `.env` variable copied into `process.env`; lazy reads at request time | TOML plus `LAVENDER__SECTION__KEY` overrides, validated at startup with all errors reported together; unknown keys rejected; base-URL overrides used by tests | serde, toml | S | Low | P1 |
| B13 | Static SPA hosting and security headers | `legacy/build/vite.js:20-47`, `:95-106` | `ServeDir` with `index.html` fallback and immutable hashed assets; CSP built from the catalog's tile hosts; XFO, nosniff, Referrer-Policy, Permissions-Policy | tower-http | S | Low | P1 |
| B14 | Background tasks and shutdown | AIS watchdog tick, 15 s disk flushes, 60 s transit prune, HLS polling, shutdown hooks (backend audit §4.1) | Every task holds a child `CancellationToken` of the core root; write-behind flushes on shutdown; graceful drain in the server and on Tauri exit | tokio-util | S | Low | P1 |
| B15 | OpenSky token cache and credit governor | `legacy/server/providers/aircraft/opensky.js:93-216`, `:393-559` | Client-credentials token cached until expiry − 60 s, refresh coalesced; adaptive TTL tiers from `X-Rate-Limit-Remaining`; cooldown 30 s–30 min; serve stale | reqwest | S | Low | P2 |
| B16 | Aircraft merge and normalisation | `legacy/src/data/adsbLolFallback.js:40-101`, `legacy/src/sources/live/aircraft.js`, `legacy/src/layers/aircraft/classification.js` | Merge by hex across adsb.lol regional, `/v2/mil` and OpenSky; newest fix wins; `military` flag; SI units; classification for icons and queries | — | M | Low | P2 |
| B17 | OMM element store and SGP4 | `legacy/server/providers/space/celestrak.js`, `legacy/src/sources/tle.js`, `legacy/src/data/satellitePass.js` | OMM JSON parsing; dedup by NORAD id; classification port; ground tracks, passes and GMST in `lavender-geo` over the `sgp4` crate | sgp4 | M | Medium (parity with satellite.js) | P2 (tracks), P3 (passes) |
| B18 | AISStream ingest task | `legacy/server/providers/vessels/ais-live.js:178-407`, `legacy/src/data/aisStreamAdapter.js:64-580`, `legacy/src/data/aisWatchdog.js:36-108` | One WebSocket per key. The watchdog is ported as an I/O-free state machine on a monotonic clock. 401/403/429 are classified from the upgrade response; connections are aborted rather than closed; frames over 1 MB rejected; backoff 5 / 15 / 60 / 300 s, then 15 min; hourly probe after auth failure | tokio-tungstenite (rustls) | L | High (undocumented upstream, no published terms) | P3 |
| B19 | AIS store | `legacy/server/providers/vessels/ais-store.js:2-258` | Hash map of latest positions plus an `rstar` R-tree bulk-loaded on a fixed tick; time-bucketed eviction at 30 min instead of an O(n) prune on every message; per-MMSI fixed ring buffers (64 samples, ≥30 s and ≥25 m apart); ≤50k vessels | rstar, parking_lot | M | Medium (throughput) | P3 |
| B20 | WMS capability parser, PNG validator, mosaic slicer | `legacy/server/providers/weather.js:82-235`, `:320-334`, `:525-577` | quick-xml with DTD and entities disabled; limits (512 KiB, ≤2,048 layers, depth ≤16, ≤512 times, last 26 within 24 h); PNG content type and IHDR size checked; global mosaic sliced into tiles; filtered IR alpha | quick-xml, png / image | M | Low | P3 |
| B21 | GRIB2 decode and resample | `legacy/server/providers/wind/decode.js:1-51`, `legacy/server/providers/wind/grid.js:12-153`, `legacy/server/providers/wind/gfs.js:44-210`, `legacy/server/providers/wind/ifs.js:6-86` | `.idx` / `.index` parsing; byte-range GETs (206 required, exact length, `redirect` none); decode GFS templates 5.0 / 5.2 / 5.3 and ECMWF packing (possibly CCSDS 5.42); bilinear resample to 1° (360×181) with longitude wrap; K→°C, Pa→hPa; little-endian output. A spike with real GFS and IFS messages decides between a pure-Rust decoder and an ecCodes FFI fallback behind a feature flag | grib (pure Rust) or eccodes (FFI) | XL | High | P3 (spike in P1) |
| B22 | InciWeb scraper | `legacy/server/providers/firePerimeters.js:88-148` | Catalogue POST (4 MiB, 20 s, 1 h TTL); publication HTML (2 MiB, 15 s) with one same-host redirect and https upgrade; `og:updated_time` and "Date of Origin" read with an HTML parser rather than regexes | scraper | S | Medium (markup changes) | P3 |
| B23 | Regions service | `legacy/src/data/naturalEarthRegions.js`, `legacy/src/data/adminBoundaries.js`, `legacy/src/data/neighborhoodPolygons.js` | Bundled datasets loaded lazily; `rstar` over polygon bounding boxes, then `geo` point-in-polygon; `decodeRing` port; name index | geo, rstar | M | Low | P3 |
| B24 | Static datasets | `legacy/src/data/local_data/datacenters/`, `dams/`, `osm_military_names/`, `telegeography_submarine_cables/` | Loaded once (embedded or from the resource dir) with a bbox index; registry gate for cables; ODbL offer metadata | rstar | S | Low | P3 (datacenters, dams), P4 (names, cables) |
| B25 | Workspace document store | `legacy/src/director/document.js`, `documentFields.js`, `cameraDocument.js` | Serde types with `deny_unknown_fields`; size, depth and node limits; versioned migrations; an unreadable document is never overwritten; SQLite on desktop, `SettingsStore` on the server | rusqlite (bundled), serde | M | Low | P3 |
| B26 | GTFS-RT decoding and transit polling | `legacy/src/data/gtfsRealtime.js:62-274`, `legacy/src/data/transitProxy.js:24-310`, `legacy/src/data/transitFeeds.js` | prost types from the official `.proto`; byte cap before decoding, then entity (≤50k) and string (≤256) caps; differential feeds refused; conditional requests (ETag / Last-Modified); hop-by-hop redirect validation (≤3, same https origin); `ET-Client-Name` for Entur | prost | M | Low | P4 |
| B27 | Transit history store | `legacy/src/sources/transitHistoryStore.js:4-479` | Byte-accounted ring buffers: 15 min, ≤128 fixes, 16 MiB per feed, 64 MiB per process, 15k keys; 60 s prune | — | M | Low | P4 |
| B28 | MVT decoding and road/flow matching | `legacy/src/sources/openFreeMap.js`, `legacy/src/data/flowMatch.js`, `legacy/src/data/tomtomTiles.js`, `legacy/src/sources/militaryTileGeometry.js` | Decode Mapbox Vector Tiles (OpenFreeMap roads and military land use, TomTom flow); port `isDrivableOpenMapRoad`, `clipTileLine`, flow→road matching (~100 m hash grid, 35 m snap, 30° bearing) and hybrid de-duplication; compact output per tile | prost (vector_tile.proto), geo | L | Medium | P4 |
| B29 | Mapillary tile cache and layer stripping | `legacy/server/providers/mapillary/tiles.js:54-493`, `legacy/server/providers/mapillary/trim.js:25-43`, `legacy/server/providers/mapillary/constants.js` | Re-encode MVT without the `image` layer; memory LRU 96 MiB plus 1 GiB disk cache with sweep; 401/403 hold 5 min, 429 hold ≤10 min; ≤6 concurrent upstream requests; redirects pinned to the tile origin | prost | M | Low | P4 |
| B30 | Overpass QL sanitizer and transport | `legacy/server/providers/overpass/query.js:36-149`, `legacy/server/providers/overpass/transport.js:19-204`, `legacy/server/providers/overpass/cache.js:18-127`, `legacy/server/providers/overpass/geometry.js:9-101` | Sanitizer as a pure module in `lavender-domain`, ported with its full test corpus; typed query templates (installations, outlines, features); failover with per-endpoint cooldown (`Retry-After` or 30 s·2ⁿ, ≤5 min); URL userinfo becomes Basic auth and is stripped from URLs and logs; Douglas–Peucker simplification; disk cache swept | regex, geo | L | High (security boundary) | P4 |
| B31 | CCTV catalogue loaders | `legacy/server/providers/cctv/sources.js:102-1865`, `legacy/server/providers/cctv/catalog.js:24-287`, `legacy/server/providers/cctv/constants.js` | One loader per pack behind a trait (14 packs); per-pack kill switch and cap; round-robin overall cap; DATEX2 XML; TxDOT base64 decoding pinned to its host; 15 min TTL; serve stale on an empty refresh; NSW through the Open Data Hub API | quick-xml, base64, csv | L | Medium (14 upstream formats) | P4 |
| B32 | CCTV media relay | `legacy/server/providers/cctv/media.js:111-560`, `legacy/server/providers/cctv/range.js:42-81` | Streaming pass-through (`Body::from_stream`) with a 15 s header deadline, 30 s idle deadline and a maximum session duration; Range sanitising (≤64 MiB span); declared-length cap; redirects pinned to the registered origin; client disconnect cancels upstream; per-user stream cap | axum, reqwest (stream) | L | High (resource exhaustion) | P4 |
| B33 | HLS relay | `legacy/server/providers/cctv/stream.js:4-348` | Session puller per upstream (≤2 sessions, ≤8 leases each, 2 s poll, 15 s lease idle, ≤12 segments / 24 MiB, playlist 256 KiB, segment 4 MiB, MPEG-TS only, no KEY / MAP / BYTERANGE, same-origin segment URIs); rewritten playlist | m3u8-rs | XL | High | P4 |
| B34 | Regional brief aggregation | `legacy/server/providers/regional/briefing.js:22-153`, `legacy/server/providers/regional/news.js:60-110`, `legacy/server/providers/regional/weather.js:6-25` | Place from the regions service; weather behind a provider trait (Open-Meteo paid, MET Norway or NWS); GDELT JSON (Google News removed); 0.1° key, 5 min, stale ≤1 h; per-part status | reqwest, serde | M | Low | P4 |
| B35 | Radio Browser directory | `legacy/server/providers/radio/catalog.js:47-394`, `legacy/server/providers/radio/transport.js` | Mirror discovery (6 h) through the `PublicOnly` resolver; 9 searches with concurrency 3; health policy (≥5 good queries, ≥375 stations) before replacing a warm catalogue; stale ≤7 days | hickory-resolver | M | Low | P5 |
| B36 | Local receivers and Beast/SBS ingest | `legacy/server/providers/local-receivers.js:29-521`, `legacy/src/sources/adsbRecords.js` | `aircraft.json` polling (2 s timeout, 2 MiB) through the `PrivateOnly` resolver; new Beast (binary) and SBS-1 (text) TCP readers with Mode-S/CPR decoding (evaluate an existing crate before writing one); jump rejection ported | tokio (TCP) | M | Medium | P5 |
| B37 | Voice session minting and cost ledger | `legacy/server/providers/openai/realtime.js:130-363`, `legacy/src/voice/voiceCost.js` | Session config built from the command registry; ephemeral secret mint; per-user budget ledger persisted in the data dir; token TTL and idle timeout; conservative estimator bound to the served model | reqwest | M | Medium (cost) | P5 |
| B38 | Diagnostics and audit sink | `legacy/server/providers/openai/debug-log.js:7-157` | Append-only JSONL in the data dir with rotation and a retention period; server-side redaction of keys, bearer tokens, `ek_` secrets and data URLs; content omitted by default; audit hook on the `LavenderCore` facade (captures, MCP calls) | tracing | S | Low | P5 |
| B39 | Command registry and MCP server | `legacy/src/tools/catalog.js`, `legacy/src/tools/schema.js`, `legacy/src/tools/queries/`, `legacy/server/mcp/`, `legacy/src/tools/mcp/protocol.js` | Registry schemas via schemars with handlers in the core; `lavender-mcp` crate (stdio plus optional HTTP) with bearer auth, allow-listed tools, per-tool audit and rate limits; protocol versions 2025-11-25 / 2025-06-18 / 2025-03-26 | rmcp, schemars | XL | Medium | P5 |

### B.2 Components not carried

| Component | Legacy evidence | Disposition | Phase |
|---|---|---|---|
| Key-setup writer and server restart | `legacy/server/standalone/key-setup.js:94-337`, `legacy/server/standalone/key-setup-hardening.mjs` | Removed (ADR 0006). Desktop uses keychain commands; the admission-gate rules (`legacy/src/keySetupCore.mjs:239-338`) stay documented as the reference for any future administrative endpoint | — |
| Codex OAuth login spawner | `legacy/server/providers/openai/codex-auth.js:11-360` | Removed (licensing §1.4) | — |
| HUD summary LLM call | `legacy/server/providers/openai/hud-summary.js:31-98` | Removed (F-161) | — |
| Google Street View fallback | `legacy/server/providers/cctv.js:91-130`, `:468-531` | Removed (F-058) | — |
| Google-derived CCTV ground heights | `legacy/server/providers/cctv/groundHeights.js:6-40` | Replaced by a generator over open terrain (F-060) | P4 |
| MCP panel key file | `legacy/server/mcp/panelKey.js:9-117` | Removed; MCP uses a bearer token from `SecretStore` | — |
| Embed-framing CSP rewrite | `legacy/build/embed-framing.js:24-72` | Deferred with embed mode (F-115) | Roadmap |
| Debug log in the source root | `legacy/server/providers/openai/debug-log.js:79-80` | Replaced by B38 in the data dir | P5 |

---

## C. Node code that is not migrated to Rust

| Code | What it does | Disposition | Why |
|---|---|---|---|
| Vite dev server and build (`legacy/vite.config.js`, `legacy/build/vite.js` build half) | Bundling, HMR, dev proxy | Stays Node in `apps/web` (Vite dev server on `:5410` proxies `/api` to `lavender-server` on `:7410`) | Frontend build tooling. Production serves `dist/` from the Rust binary, so there is no Node at runtime (overview §6) |
| `vite-plugin-cesium` (`legacy/build/vite.js`) | Copies Cesium static assets | Replaced by an explicit copy step for `@cesium/engine` (`Build/Workers`, `Build/ThirdParty`, `Source/Assets`) that fails the build when one is missing | ADR 0003 |
| HTML template expansion (`legacy/build/application-html.js`) | Splices `<!-- gev:template -->` partials | Not migrated | React components replace the templates |
| MCP panel build (`legacy/build/panel.js`) | Static `dist/panel` for MCP Apps | Not migrated | Panel deferred (F-164) |
| Test runner (`legacy/scripts/run-unit-tests.mjs`, `node:test` suites) | Unit tests | Vitest for TS, `cargo test` for Rust; the legacy suites are case sources (§E) | One runner per language |
| Pure browser-side maths (frame-rate or UI-local) | `legacy/src/data/motionModel.js`, `legacy/src/data/aircraftIcons.js`, `legacy/src/layers/satellites/orbits.js` (`orbitFrameModelMatrix`, per-frame satellite.js), `legacy/src/layers/wind/` (`sampleWind`, `bakeWindStreamlines`), `legacy/src/annotations/drawMode.js`, `legacy/src/data/labelArbiter.js`, `legacy/src/overlays/worldOverlayDraw.js`, `legacy/src/layers/streetLevel/` (`tileMath`, `groundCast`, `meshSampler`), `legacy/src/data/cctvLod.js`, `legacy/src/data/contactPlayback.js`, `legacy/src/data/trafficQueue.js` (dot platoons), `legacy/src/locations.js` (place framing), `legacy/src/data/layerState.js` and `legacy/src/view/index.js` (link codec), `legacy/src/cockpitMath.js`, `legacy/src/data/localGeojsonLod.js`, `legacy/src/layers/weather/clock.js`, `legacy/src/layers/weather/imageryOrder.js` | Ported to TS (ADR 0004 "almost verbatim") | Runs per frame or per interaction against the scene; an IPC or HTTP round trip would be slower than the computation; no I/O, no secrets |
| Logic kept in both languages on purpose | Coordinate parsing and formatting (DD, DMS, MGRS); geoid undulation; SGP4; distance and bearing | Rust is authoritative (search, queries, tracks); TS gives instant UI feedback | One shared test corpus is run by both `cargo test` and Vitest (§E) |
| Voice client (`legacy/src/voice/realtimeConnection.js` and siblings) | WebRTC session with OpenAI | Stays browser-side TS in P5 | WebRTC lives in the WebView; only minting, budget and diagnostics are in Rust (B37, B38) |
| Launchers (`legacy/scripts/dev-fresh.sh`, `legacy/scripts/dev-secure.sh`, `legacy/scripts/pinokio-start.mjs`, `legacy/pinokio/`) | Keychain/`.env` launchers, Pinokio | Removed | Replaced by `lavender-server` configuration and the Tauri app (F-188) |
| `legacy/scripts/precompute-cctv-heights.mjs` | Headless Chrome sampling Google 3D Tiles | Replaced by an offline Rust generator over open terrain | The output breaches GMP terms (licensing B11) |
| `legacy/tools/*.mjs` | Google tile stitching, pano utilities, puppeteer render | Removed | Likely breaches provider terms (F-166) |
| QA instrumentation (`legacy/src/layers/transit/qaMetrics.js`, traffic timing, `legacy/src/overlays/worldOverlayAllocation.worker.mjs`) | Script-only metrics and an allocation benchmark | Not migrated; the P7 performance harness replaces them | Test code must not ship (F-187) |
| MCP stdio process (`legacy/server/mcp/stdio.js`) | Separate Node process calling the app over HTTP | Replaced by the Rust `lavender-mcp` binary (B39) | Tools call the core directly |
| Browser SDR (`legacy/src/sdr/`) | WebUSB demodulation and Mode-S decoding | Not migrated (F-066 removed, F-067 Roadmap) | WebUSB is unavailable in WKWebView and WebKitGTK |

---

## D. Shared-logic split per provider

Rule: normalisation, validation, aggregation, joins and anything that needs a key, a cache or a cross-viewer view move into Rust. Per-frame presentation maths stays in the TS renderer. The frontend receives typed domain objects (ADR 0007), never raw upstream payloads.

| Provider / feature | Moves into Rust (core) | Stays in the TS renderer | Kept in both, cross-tested |
|---|---|---|---|
| Aircraft (adsb.lol, OpenSky, tracks, adsbdb) | readsb and OpenSky state-vector parsing (`legacy/src/sources/live/aircraft.js`), unit normalisation, civil/military merge, classification (`legacy/src/data/aircraftClass.js`), route plausibility (`legacy/src/data/routePlausible.js`), track normalisation, enrichment join, LADD suppression | Motion model and display delay (`legacy/src/data/motionModel.js`), icon SVGs and rotation stabilisation, model eligibility (`selectModelEligible`), trails | Distance and bearing |
| Local receivers | `normalizeDump1090Aircraft`, `mergeLocalAdsbRecords`, `localAdsbFixIsPlausible` (`legacy/src/sources/adsbRecords.js`), Beast/SBS decoding | Rendering through the aircraft engine | — |
| Vessels (AISStream) | Message adapter, watchdog, store, `normalizeVesselObservation` (knots → m/s, heading vs course), retention | AIS type colours (`legacy/src/data/vesselLabels.js`), rotation and occlusion | — |
| Satellites (CelesTrak) | OMM parsing, dedup, classification (`legacy/src/data/satelliteClass.js`), ground tracks, passes (`legacy/src/data/satellitePass.js`), payload matching for launches | Per-frame propagation of the visible subset, GMST ring alignment | SGP4 (Rust `sgp4` vs satellite.js) |
| Launches (LL2) | `normalizeRocketLaunches`, payload → NORAD matching | Path reconstruction and replay maths (`legacy/src/layers/launches/paths.js`, P5) | — |
| Earthquakes (USGS) | Snapshot validation (per-row rejection), magnitude filter | Depth colour, symbol size | — |
| Weather imagery (nowCOAST) | Capabilities validation, manifest, time snapping, tile fetch, mosaic slicing, filtered IR alpha (`legacy/src/layers/weather/infraredAlpha.js`) | Observation clock, imagery ordering, shell material for 3D tiles | — |
| Wind (GFS/IFS) | GRIB fetch, decode, resample, unit conversion, grid packaging | `sampleWind`, streamline baking, palettes, renderer | — |
| Cyclones (NHC) | Strict validation (`validateCycloneSnapshot` rules), geometry budgets | Label layout (`legacy/src/layers/cyclones/labels.js`) | — |
| Fires (FIRMS) | CSV parsing (`legacy/src/data/firmsCsv.js`), `adaptFirmsRecords` (confidence, time, sensor), 24 h filter, LOD cell aggregation, top-N | LOD band choice from camera altitude, horizon cull, sprites | — |
| Fire perimeters (WFIGS, InciWeb) | Record normalisation (`legacy/src/layers/perimeters/records.js`), per-zoom simplification, InciWeb name matching and link verification | Polygon and fire-line rendering, cards | — |
| Recent imagery (CMR, Worldview) | `parseCmrUmm`, `groupGranulesByDay`, `rankLatest`, presence probe | Box drawing, GIBS tile URL templates (from catalog), split/swipe | Box maths (antimeridian) |
| Traffic (TomTom, OpenFreeMap) | MVT decoding, `isDrivableOpenMapRoad`, `clipTileLine`, `flowMatch`, hybrid de-dup, tile maths (`legacy/src/data/tomtomTiles.js`), budget | Footprint tile selection (`convexFootprint`), dot simulation and platoons, heat-line styling | Tile maths (z/x/y bounds) |
| Transit (GTFS-RT) | Protobuf decoding, feed registry, history store, bbox filter, movement plausibility at ingest | Playback interpolation (`legacy/src/data/contactPlayback.js`), billboards, trails | — |
| Bikeshare (GBFS) | Discovery, v2/v3 normalisation, station merge (`legacy/src/sources/gbfsStations.js`), bbox | Availability colour and size | — |
| Directions (OSRM) | Request validation (leg and total caps), OSRM call, step normalisation (`legacy/src/data/routeSteps.js` `instructionFor`) | Distance and duration formatting (locale), `stepIndexAtDistance`, route preview | — |
| CCTV | Pack loaders, `normalizeFeedType` (once; legacy had two copies), health, media relay, HLS | Frustum maths (Roadmap), LOD selection (`legacy/src/data/cctvLod.js`), video element | — |
| Street level (Mapillary) | Coverage tile proxy, layer stripping, Graph API proxy | Coverage MVT decoding (small tiles), `tileMath`, `groundCast`, `meshSampler`, viewer host | — |
| Military installations (Overpass, OpenFreeMap, names) | QL templates and sanitizer, MVT decoding and fragment merge (`legacy/src/sources/militaryTileGeometry.js`), names R-tree, `normalizeMilitaryInstallations` | Rendering and selection | — |
| Regions and briefing | Point-in-polygon, ring decoding, name lookup, `regionalModel` (`legacy/src/data/regionalModel.js`) | Optional outline rendering, brief card | — |
| Radio (Radio Browser) | `normalizeRadioBrowserStation`, curation, public-HTTPS stream checks (`legacy/src/sources/radioBrowser.js`) | Playback state machine, screen-space clustering | — |
| Search and geocoding | Coordinate parsing, geocoder chain, `normalizeGooglePlace` / reverse, Photon type mapping, live-entity search | Place framing (`geocodeNavigationMode`, `placeFramingViewport`, `regionFramingPlan`) | DD / DMS / MGRS parsing and formatting |
| Terrain heights and datum | Heights proxy, per-point cache, geoid in responses | Status-bar MSL, ground service rules | Geoid undulation |
| Annotations | Geometry resolution service, workspace persistence | `drawMode` geometry, renderers | Great-circle length and spherical area (`lavender-geo` and `drawMode`) |
| Analyst / tools | Query execution over hub stores, command handlers | Command palette and result cards | — |

---

## E. Verification strategy per provider

### E.1 Test layers

1. **Recorded-fixture unit tests.** Pure parse, validate and normalise functions run against files in `crates/lavender-core/tests/fixtures/<provider>/`, seeded from the legacy fixtures and from inline payloads extracted from legacy tests (backend audit §7). Assertions are ported from the named legacy suites.
2. **wiremock contract tests.** Provider plus `Fetcher` against a mock server, through configured base-URL overrides. They prove the behaviour that matters in production: status handling, body caps, timeouts, redirect refusal or pinning, `Retry-After` and cooldown ladders, single-flight coalescing, last-waiter cancellation, stale serving, and licence-gate refusal (no request made).
3. **Server integration tests.** The Axum router with providers wired to wiremock, over HTTP and WebSocket: status-code table, `ApiError` shape, origin checks, rate limits, subscription caps.
4. **Live smoke (manual, keyed).** A per-provider checklist run with real keys against production upstreams, outside CI. Results are recorded in the progress log. Responses whose licence forbids storage (TomTom, Google, Mapillary imagery) are never saved as fixtures.
5. **Cross-language corpus.** Shared JSON vectors (coordinates, SGP4 states, geoid samples, great-circle measurements) run by both `cargo test` and Vitest.

### E.2 Per provider

| Provider | Recorded fixtures to seed Rust tests | wiremock contract tests | Live smoke (manual, keyed) |
|---|---|---|---|
| adsb.lol (aircraft, `/v2/mil`, traces) | adsb.lol cases in `legacy/src/tooling/liveProviders.test.mjs`, `legacy/src/data/adsbLolFallback.test.mjs`; capture a `/v2/lat/lon/dist` and a `/v2/mil` response | Regional key quantisation; 429/5xx cooldown clamped 5–120 s; body cap; merge with `/v2/mil`; trace 404 | Austin and a busy hub at the default cadence; counts against adsb.lol's own map |
| OpenSky | OpenSky cases in `legacy/src/tooling/liveProviders.test.mjs` (token sharing, adaptive cache, Retry-After bounds) | OAuth token reuse and coalesced refresh; adaptive TTL tiers; cooldown 30 s–30 min; commercial mode makes zero requests | With evaluation credentials only; credit headers observed |
| adsbdb | `legacy/src/tooling/spaceProviders.test.mjs`, `legacy/src/proxyErrorResponses.test.mjs` adsbdb cases | Positive and negative caching, bounded size, licence-gate refusal | Only if permission is obtained (Q9) |
| Local receivers | `legacy/src/data/fixtures/adsb-austin-dump1090-aircraft.json`, `adsb-austin-capture-timed.json`; `legacy/src/data/localReceiversProxy.test.mjs`, `legacy/src/sources/adsbRecords.test.mjs` | Private-only resolver refuses public addresses; 2 s timeout; 2 MiB cap; no redirects | A real dump1090/readsb on the LAN; Beast/SBS socket |
| AISStream | `legacy/src/data/aisStreamAdapter.test.mjs`, `aisStreamSentinels.test.mjs`, `aisWatchdog.test.mjs`; **capture a real message stream** (none exists upstream) | WebSocket mock: auth failure, 429 on upgrade, silence → recycle, backoff ladder, oversized frame | 10 min with a real key; vessel counts and store memory |
| CelesTrak | `legacy/src/tooling/spaceProviders.test.mjs` celestrak cases; **capture OMM JSON** for stations, GNSS and active groups | Single-flight per group; stale on error; group not configurable from requests; body cap | Daily refresh within CelesTrak's usage policy |
| Launch Library 2 | `legacy/src/data/rocketLaunches.test.mjs`; capture a `2.3.0/launches` page | 15 min cache; 12 MiB cap; token header when configured; stale on error | Anonymous rate (15/h) not exceeded over a day |
| USGS earthquakes | `legacy/src/data/earthquakes.test.mjs`; capture `all_day.geojson` | Bad row dropped with a notice; 60 s cadence shared across subscribers | Feed age < 2 min |
| nowCOAST (radar, IR, lightning) | Inline capabilities XML and PNG cases from `legacy/src/data/weatherProxy.test.mjs` (33 tests); `legacy/src/layers/weather/rasterTiles.test.mjs`, `infraredAlpha.test.mjs` | Entity/DTD refusal; time not advertised → 400; queue caps 8 / 120 / 96 → 503; last-waiter cancellation; IHDR mismatch rejected | Each product renders over CONUS; latest time within 15 min |
| GFS / IFS wind | `.idx` / `.index` cases from `legacy/src/data/windGfs.test.mjs`, `windIfs.test.mjs`; grid cases from `windGrid.test.mjs`, `windProxy.test.mjs`; **add real GRIB2 messages** (GFS UGRD/VGRD 10 m, TMP 2 m, PRMSL; IFS 10u/10v/2t/msl) | 206 required with exact length; range cap 8 MiB; 40 s budget; scalar failure keeps wind; little-endian output | Latest cycle decoded for both models; values spot-checked against a reference viewer |
| NHC cyclones | Inline NHC JSON and GIS GeoJSON from `legacy/src/data/cyclonesProxy.test.mjs` | Strict validation; waiter cap → 503; last-good ≤12 h; failure → freshness | During an active storm: track and cone vs nhc.noaa.gov |
| FIRMS | `legacy/src/data/fixtures/firms-viirs-noaa20-sample.csv`, `firms-csv-cases.json`; `legacy/src/data/firmsProxy.test.mjs` | Partial failure → degraded; body caps per source; key never in logs; keyless → unconfigured | Global counts vs FIRMS map; transaction quota reported |
| WFIGS / InciWeb | Inline WFIGS GeoJSON and InciWeb HTML from `legacy/src/data/firePerimetersProxy.test.mjs`, `legacy/src/layers/perimeters/source.test.mjs` | Paging past the transfer limit (≤5); 16 MiB per page; single pinned redirect; stale serving | Fire season: perimeters and InciWeb links verified |
| TomTom + OpenFreeMap | `legacy/src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf` (test-only, © TomTom; licensing B16 suggests a synthetic replacement), `ofm-austin-12-935-1686.pbf`, `ofm-austin-14-3743-6745.pbf`, `traffic-road-access.json`; `legacy/src/data/tomtomTiles.test.mjs`, `flowMatch.test.mjs` | Daily budget persisted and rolled over at UTC midnight; over budget → stale or 503; disk sweep; key absent → 503 `provider_unconfigured` | Austin downtown at z12–z14; budget counter increments |
| GTFS-RT transit | Protobufs built with `PbfWriter` in `legacy/src/data/gtfsRealtime.test.mjs`; `legacy/src/data/transitProxy.test.mjs`, `legacy/src/tooling/transitProvider.test.mjs`, `legacy/server/providers/transitHistory.test.mjs`; capture one binary per feed | 304 handling; differential refusal; redirect pinning; cooldown ladder; entity and string caps; history admission | Each of the 7 feeds once; Entur `ET-Client-Name` accepted |
| GBFS bikeshare | `legacy/src/data/gbfsProxy.test.mjs`, `legacy/src/layers/bikeshare/proxyRoute.test.mjs`; capture one v2 and one v3 system | Registry id only; redirects refused; 5 MiB cap; `ttl`-driven cache | Three systems across operators |
| Re:Earth terrain heights | `legacy/src/data/terrainHeightsProxy.test.mjs`; terrain cases in `legacy/src/tooling/environmentProviders.test.mjs` | 64-point chunking; request-order reconstruction; `Retry-After` retries inside 10 s; range check; >2,000 → 413 | Known summits within tolerance |
| Geocoding (Nominatim, Photon, Google) | `legacy/src/nominatimGeocode.test.mjs`, `legacy/src/tooling/nominatimSearchRoute.test.mjs`, `legacy/src/keylessGeocoder.test.mjs`, `legacy/src/search/placeSearch.test.mjs`; Google results are **not** stored as fixtures | Nominatim pacer ≥1.1 s with a queue of 4; abandon on cancel; 2 MiB cap; Google provider active only with the Google basemap | Self-hosted Nominatim/Photon; Google only on a billed account |
| Google Places | `legacy/src/tooling/placeProviders.test.mjs`, `legacy/src/googlePlacesCoordinates.test.mjs`, `googlePlacesKeyless.test.mjs`, `googleServerKey.test.mjs` | Timeout and body cap (legacy had none); rate limit; keyless → 503 | Billed key, Google basemap active |
| OSRM | OSRM cases in `legacy/src/tooling/placeProviders.test.mjs`; `legacy/src/data/directions.test.mjs`, `routeSteps.test.mjs` | 1 req/s gate with queue ≤8; upstream 429 relayed with `Retry-After`; leg and total caps → 422 | Self-hosted OSRM or Valhalla |
| Mapillary | `legacy/src/tooling/mapillaryProvider.test.mjs` (40 tests) | Zoom allowlist; key-rejected and 429 holds; layer stripping; memory and disk TTL; sweep | Coverage over a known city; token never visible to the browser |
| Overpass and installations | `legacy/src/data/overpassProxy.test.mjs`, `legacy/src/overpassProxy.test.mjs`, `legacy/src/overpassOffload.test.mjs`, `legacy/src/data/installationProxy.test.mjs` (sanitizer corpus ported verbatim); `legacy/src/data/fixtures/ofm-camp-mabry-12-935-1685.pbf` | Sanitizer refusals not cached; failover and cooldown; credentials stripped from logs; zero egress when unconfigured; fuzzing of the sanitizer | Against a self-hosted Overpass; a 10°×10° installation query |
| Regional brief (regions, GDELT, weather) | `legacy/src/data/local_data/natural_earth/*.json`, `us_census_counties/`, `neighborhoods/`; `legacy/src/data/regionalProxy.test.mjs`, `legacy/src/data/naturalEarthRegions.test.mjs` | Coalescing on the 0.1° key; per-part failure → partial; no Google News host reachable | GDELT headlines for a busy region; licensed weather provider |
| Radio Browser | Rows from `legacy/src/data/radioProxy.test.mjs` | Public-only resolver; mirror fallback; health policy before replacing a warm catalogue | Directory builds ≥375 stations |
| CCTV (catalogue, frames, HLS) | `legacy/config/cctv_sources.tallinn.json`, `cctv_sources.warendorf.json` (not Shinjuku: unlicensed promo clips, licensing B13); per-agency inline catalogues from `legacy/src/data/cctvCalgary.test.mjs`, `cctvDriveBcSource.test.mjs`, `cctvEstonia.test.mjs`, `cctvFintraffic.test.mjs`, `cctvNswSource.test.mjs`, `cctvTxdotSource.test.mjs`, `cctvVegvesen.test.mjs`, `cctvWarendorf.test.mjs`; `cctvMediaRange.test.mjs`, `cctvHlsStream.test.mjs`, `cctvProxy.test.mjs`, `cctvCatalogCap.test.mjs` | Origin-pinned redirects; Range sanitising; header and idle deadlines; maximum duration; lease and session caps; unknown id → 404; off by default | Only packs with confirmed terms (Q16); TfL, Ontario 511, Fintraffic, Calgary, Vegvesen first |
| OpenAI Realtime | Token-mint and debug-log cases in `legacy/src/tooling/localServices.test.mjs`; `legacy/src/voice/realtimeBackend.test.mjs` | Budget refusal; auth required; redaction; upstream errors not passed through | Billed key; one session within budget |
| NASA CMR / Worldview / GIBS | Recent-imagery in-dir tests (`legacy/src/layers/recentImagery/`); **capture** CMR UMM pages and Worldview `Data-Present` headers | `CMR-Search-After` paging ≤2,000 records; presence-probe caching | A box over a recent clear-sky day |
| Google Map Tiles token | — (no stored Google responses) | Token mint budget; key never returned unless operator-marked public | Billed key; 3D Tiles render with credits |
| Static datasets (datacenters, dams, names, cables) | `legacy/src/data/local_data/datacenters/`, `dams/`, `osm_military_names/` (`names.sha256`), `telegeography_submarine_cables/` (test-only use) | bbox queries; licence gate on cables; checksums | — |

### E.3 Fixtures to capture (absent upstream)

Real GFS and IFS GRIB2 messages with their `.idx` / `.index` files; an AISStream message capture (positions, static data, error frames); CelesTrak OMM JSON; a USGS `all_day.geojson`; an LL2 page; CMR UMM pages and Worldview headers; one GTFS-RT binary per feed; one GBFS v2 and one v3 system; Nominatim, Photon, OSRM and GDELT responses; a Mapillary coverage tile (test-only, CC BY-SA). The inline payloads inside legacy tests (WMS capabilities, NHC JSON, WFIGS GeoJSON, InciWeb HTML, CCTV agency catalogues, Radio Browser rows) are extracted into fixture files. Each fixture records its source, retrieval date and licence. Google and TomTom content is not stored.

---

## F. Risk register (top 12)

| # | Risk | Likelihood | Impact | Mitigation | Phase |
|---|---|---|---|---|---|
| 1 | **GRIB2 decoding parity.** Pure-Rust decoders may lack GFS complex packing with spatial differencing or ECMWF CCSDS packing; there is no real GRIB fixture upstream (backend audit §9.4) | High | High (wind layer) | Spike in P1 with real GFS and IFS messages; decide pure Rust vs ecCodes FFI behind a feature flag; compare grids with legacy output on the same messages; cache decoded grids so decoding is rare | P1 spike, P3 |
| 2 | **Licence-restricted defaults** (OpenSky, AISStream, CelesTrak, adsbdb, Open-Meteo, Google News, public OSM services) block a commercial release | High | High | Registry gate with commercial-safe defaults (ADR 0008 §4); every restricted provider configurable and off in commercial mode; open questions tracked in spec §8 and licensing §8 | P1 onward |
| 3 | **CCTV media relay and HLS exhaust resources** (unbounded live bodies, session memory, redirect SSRF) | Medium | High | Origin-pinned redirects; maximum session duration; per-user and global stream caps; segment memory caps; CCTV off by default; load tests in P7 | P4, P7 |
| 4 | **Overpass sanitizer port loses a refusal** (a regex-based security boundary) | Medium | High | Raw QL no longer client-reachable; port the corpus verbatim; fuzz the sanitizer; templates are the only producers of QL | P4 |
| 5 | **AIS firehose throughput** (legacy prunes O(n) on every message) | Medium | Medium | Time-bucketed eviction; bulk R-tree rebuild on a tick; benchmarks at 50k vessels and peak message rates; memory budget on desktop | P3, P7 |
| 6 | **Feed hub correctness** (region-key churn, slow consumers, cancellation races, task leaks) | Medium | High | `watch` latest-value semantics; caps on tasks and subscriptions; deterministic tests with paused Tokio time; cancellation property tests; leak checks in CI | P1 |
| 7 | **SSRF through operator-configured URLs** (Overpass upstreams, local receivers, CCTV source files, XYZ imagery) | Medium | High | Allowlists derived from configuration; `PublicOnly` / `PrivateOnly` resolver policies; redirect pinning; no URL from request parameters (principle 4) | P1, P4, P5 |
| 8 | **Quota and cost overrun** (TomTom budget, Google tiles, FIRMS transactions, OpenAI) | Medium | Medium | Server-enforced budgets persisted in the data dir; per-IP limits; authentication on voice; quotas visible in `/status`; alerts later | P1, P3–P5 |
| 9 | **No-SLA upstreams** (adsb.lol, Re:Earth, OpenFreeMap, DeFlock, CelesTrak) fail or start requiring keys | Medium | Medium | Serve stale with honest freshness; configurable base URLs for self-hosted mirrors; health in `/status`; contact operators for production use (adsb.lol) | P2 onward |
| 10 | **Desktop WebView differences** (WebGL2 on WebKitGTK, HLS/MSE without GStreamer plugins, no WebUSB, custom-protocol tile fetching, CSP generation) | Medium | Medium | Custom protocol for tiles proven in P3; validation matrix in P6; hls.js fallback; features that cannot work on a platform are disabled and labelled, never broken | P3, P6 |
| 11 | **Rust↔TS divergence in duplicated logic** (SGP4, MGRS/DMS, geoid, measurements) | Medium | Low | Shared vector corpus run by both test suites; Rust declared authoritative for queries and exports | P2 onward |
| 12 | **Secret leakage** through keys in URLs (FIRMS path, TomTom query), logs, error bodies or the bundle | Low | High | `secrecy` types; errors stripped of URLs; tracing redaction; bundle scan for key-like strings; no build-time key injection; keychain on desktop | P1, P6 |
