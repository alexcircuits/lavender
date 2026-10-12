# Upstream backend audit — God's Eye View (Node) → Lavender (Rust)

Scope: every server-side module of the upstream app in `legacy/` — `legacy/server/**`
(providers, mcp, standalone), `legacy/build/*.js`, `legacy/vite.config.js`, the
`legacy/src/**` modules the server imports, plus `legacy/.env.example`,
`legacy/SECURITY.md` and the relevant `legacy/docs/*.md`.

Method: static read of the source (no `npm install`, nothing executed). Every claim
cites `legacy/<path>:<line>`. Where upstream docs and code disagree, the code wins
and the disagreement is listed in §9.

Size: `legacy/server` + `legacy/build` is ~19,000 lines across 104 files; the
largest single module is the CCTV catalog loader `legacy/server/providers/cctv/sources.js`
(1,865 lines).

---

## 0. One-paragraph summary

There is **no standalone backend process**. All server behaviour is Connect
middleware registered by Vite plugins (`configureServer` / `configurePreviewServer`)
and runs inside `vite` (dev), `vite preview` (built assets) or a programmatic Vite
**dev** server started by the Pinokio launcher. A plain `vite build` yields static
files with no `/api` at all. The backend exposes **58 method+path endpoints** on
**39 middleware mounts** (plus a `/api/*` JSON 404 catch-all and a dev-only `/panel/*`
static mount). It is a key-brokering proxy over ~45 public upstreams with in-memory
and on-disk (`.gev-cache/`) caches, per-IP rate limiters, single-flight coalescing,
serve-stale fallbacks, one persistent upstream WebSocket (AISStream), an in-process
HLS re-packager, an eccodes-WASM GRIB2 decoder, a protobuf GTFS-RT decoder, an MCP
server (stdio + stateless HTTP), and a credential-writing settings endpoint that
rewrites `.env` and restarts the server.

---

## 1. Hosting model

### 1.1 How the backend runs

| Mode | Command | What hosts `/api` | Evidence |
|---|---|---|---|
| Dev | `npm run dev` → `vite` | Vite dev server + all provider plugins + key-setup + `/panel/*` | `legacy/package.json:40`; `legacy/vite.config.js:1`; `legacy/server/standalone/vite.config.js:13-31` |
| Preview | `npm run preview` → `vite preview` | Vite preview server serving `dist/` + provider plugins (`configurePreviewServer`), **no** key-setup | `legacy/package.json:45`; `legacy/server/standalone/key-setup.js:245-247` |
| Build | `npm run build` → `vite build` | **Nothing.** Static assets only; the build helper "never load[s] environment or providers" | `legacy/package.json:43`; `legacy/build/vite.js:49` |
| Panel build | `npm run build:panel` | Static `dist/panel` for the MCP Apps panel | `legacy/package.json:44`; `legacy/build/panel.js:11-33` |
| Pinokio | `node scripts/pinokio-start.mjs` | A Vite **dev** server via `createServer({server:{host:'127.0.0.1',port,strictPort:true}})` | `legacy/scripts/pinokio-start.mjs:28-52`; `legacy/pinokio/start.js:1-43` |
| macOS launchers | `scripts/dev-fresh.sh`, `scripts/dev-secure.sh` | `npm run dev -- --host --port` after resolving keys from Keychain/.env | `legacy/scripts/dev-fresh.sh:68`, `legacy/scripts/dev-fresh.sh:426`; `legacy/scripts/dev-secure.sh:203` |
| MCP stdio | `npm run mcp` → `node server/mcp/stdio.js` | Separate Node process; tools call a running dev/preview server's `/api` over HTTP | `legacy/package.json:54`; `legacy/server/mcp/stdio.js:95-113`; `legacy/server/mcp/services.js:13-37` |

So "production" for upstream is effectively `vite preview` or Pinokio's dev server.
`legacy/SECURITY.md:109` states the Vite server is a development/preview server only.

### 1.2 Configuration loading

- `loadEnv(mode, root, '')` (empty prefix = **every** variable in `.env*`) is copied
  into `process.env` unless already set: `legacy/server/standalone/vite.config.js:14-17`.
- Providers read `process.env` lazily at request time, deliberately, because modules are
  imported before `loadEnv` runs: `legacy/server/providers/openai/rate-limit.js:15-30`,
  `legacy/server/providers/places/google.js:12-36`,
  `legacy/server/providers/vessels/ais-live.js:207-244`.
- Browser-exposed values are passed explicitly via Vite `define`:
  `legacy/server/standalone/vite.config.js:24-26` → `legacy/build/vite.js:107-114`.

### 1.3 Ports, binding, host checks

- Defaults: `host='localhost'`, `port=4173` (`legacy/build/vite.js:56-57`, `legacy/build/vite.js:88-90`);
  overridable by `HOST`/`PORT` (`legacy/server/standalone/vite.config.js:27-28`; `legacy/.env.example:121-129`).
- Pinokio pins `127.0.0.1` and a Pinokio-supplied port (`legacy/pinokio/start.js:9-10`,
  `legacy/scripts/pinokio-start.mjs:12-18`, `legacy/scripts/pinokio-start.mjs:42-48`).
- Host-header allowlist: `localhost`, `127.0.0.1` + `GEV_ALLOWED_HOSTS` (comma list; suffix
  `.x` and `*` entries ignored) — `legacy/build/allowedHosts.js:12`, `legacy/build/allowedHosts.js:23-29`.
  Rule: any IPv4/IPv6 literal, `localhost`, `*.localhost`, configured server host, or listed
  name — `legacy/build/allowedHosts.js:42-61`.
- `hostCheckPlugin` is `enforce:'pre'` so it runs **before every provider middleware**
  (Vite's own check runs after plugin middleware); refusal = `403 text/plain "Blocked
  request. This host is not allowed."` — `legacy/build/allowedHosts.js:70-93`.
- `server.fs.deny` hides `.env`, `.env.*`, `*.{crt,pem}`, `**/.git/**`, `**/ENVIRONMENT`
  from Vite's file server — `legacy/build/vite.js:95-97`.

### 1.4 Response headers / CSP

- Every dev **and** preview response gets `X-Frame-Options: DENY`,
  `Content-Security-Policy: <BROWSER_CSP>`, `X-Content-Type-Options: nosniff`
  (`legacy/build/vite.js:42-47`, `legacy/build/vite.js:102`, `legacy/build/vite.js:106`).
- CSP: `default-src 'self'`; `script-src 'self' 'unsafe-eval' blob:` + youtube/facebook/twitter;
  `connect-src 'self' blob: data: https: wss: ws:`; `img-src`/`media-src` any `https:`;
  `frame-ancestors 'none'`, etc. — `legacy/build/vite.js:20-39`.
- Embed mode (`?embed=1` on `/` or `*.html`) may be framed only when
  `GEV_EMBED_FRAME_ANCESTORS` is set; the plugin intercepts `setHeader` to swap only the
  `frame-ancestors` directive (or drop framing headers for `*`) — `legacy/build/embed-framing.js:24-72`.
- Key-setup responses additionally set `Cache-Control: no-store`, XFO DENY and
  `frame-ancestors 'none'` — `legacy/server/standalone/key-setup.js:74-84`.
- No CORS headers are emitted anywhere (same-origin app); no TLS termination — protocol is
  inferred from `req.socket.encrypted` (`legacy/server/providers/common/same-site.js:18`).

### 1.5 Middleware order (this is the routing table's precedence)

`host-check` → `vite-plugin-cesium` → `application-component-templates` → 28 provider
plugins → `local-mcp` → `api-not-found` → `embed-framing` → `panel-build`
(`legacy/build/vite.js:62-70`, `legacy/server/standalone/vite.config.js:19-23`).
Provider order (`legacy/server/providers/local.js:34-65`): opensky, celestrak, tomtom, firms,
launches, terrain, adsbdb, overpass(+route), military-installations, regional-brief, geocode,
weather-effects, cctv, radio, gbfs, local-receivers, transit, adsb.lol military, AIS vessels,
track backfill, openai, google places, wind, weather, cyclones, mapillary, fire-perimeters,
key-setup.

`api-not-found` answers any unmatched `/api/*` with `404 {"error":"Unknown API route"}`
and `Cache-Control: no-store`, in dev and preview, before Vite's SPA HTML fallback —
`legacy/server/standalone/api-not-found.js:2-16`.

### 1.6 Routing semantics the Rust server must reproduce (or consciously change)

- Connect `use(prefix, fn)` is a **prefix** mount that strips the prefix from `req.url`;
  it matches **case-insensitively** and also when the next character is `.`
  (the upstream itself documents this: `legacy/src/tools/queries/panelRequest.js:27-35`).
- Ordering hacks rely on `next()`: `/api/flights` hands non-`/` paths to the later
  `/api/flights/track` mount (`legacy/server/providers/aircraft/opensky.js:385-387`);
  `/api/military` does the same for `/api/military/track`
  (`legacy/server/providers/aircraft/adsb-lol.js:73-75`); local-receivers falls through on
  subpaths (`legacy/server/providers/local-receivers.js:507-513`).
- Many handlers ignore the remaining path, so `/api/<mount>/anything` is served by the
  mount handler: realtime/openai routes (`legacy/server/providers/openai.js:27-49`),
  Google routes (`legacy/server/providers/places/google.js:66`, `:180`), `/api/route`
  (`legacy/server/providers/places/routes.js:229`), `/api/regional-brief`,
  `/api/weather-effects`, `/api/geocode`, `/api/military-installations`, `/api/launches`,
  `/api/terrain/heights`, `/api/vessels` (any non-`/track` subpath returns rows,
  `legacy/server/providers/vessels/ais-live.js:83-111`), `/api/firms` (any non-`/status`
  subpath returns fires, `legacy/server/providers/firms.js:187-220`).
- Method handling is inconsistent: some routes accept **any** method (flights, military,
  tracks, adsbdb, vessels, celestrak, firms, tomtom, terrain, route, cctv sources/health/
  stream/frame); others return 405 with or without `Allow`.

### 1.7 Dev-only extras

- `/panel/*` serves `dist/panel` from disk in dev with path-traversal guard and a fixed
  MIME map — `legacy/build/panel.js:35-90`.
- HTML template expansion (`<!-- gev:template name -->`, allowlisted names) at transform
  time — `legacy/build/application-html.js:3-39`. Build-time concern, not a runtime route.
- Vite's own HMR WebSocket and module serving (not part of the app contract).

---

## 2. Route table (the contract)

### 2.0 Master index (58 endpoints)

| # | Method | Path | Handler | Group |
|---|---|---|---|---|
| 1 | any | `/api/flights` | `legacy/server/providers/aircraft/opensky.js:383` | aircraft |
| 2 | any | `/api/flights/track?icao24=` | `legacy/server/providers/aircraft/tracks.js:68` | aircraft |
| 3 | any | `/api/military` | `legacy/server/providers/aircraft/adsb-lol.js:71` | aircraft |
| 4 | any | `/api/military/track?hex=` | `legacy/server/providers/aircraft/tracks.js:96` | aircraft |
| 5 | any | `/api/adsbdb/route/:callsign` | `legacy/server/providers/aircraft/enrichment.js:111` | aircraft |
| 6 | any | `/api/adsbdb/type/:hex` | `legacy/server/providers/aircraft/enrichment.js:111` | aircraft |
| 7 | any | `/api/vessels?maxRows=` | `legacy/server/providers/vessels/ais-live.js:74` | vessels |
| 8 | any | `/api/vessels/track?mmsi=` | `legacy/server/providers/vessels/ais-live.js:83` | vessels |
| 9 | any | `/api/celestrak/:group` | `legacy/server/providers/space/celestrak.js:70` | space |
| 10 | GET | `/api/launches` | `legacy/server/providers/space/launch-library.js:92` | space |
| 11 | GET | `/api/weather/manifest` | `legacy/server/providers/weather.js:473` | weather |
| 12 | GET | `/api/weather/tile` | `legacy/server/providers/weather.js:484` | weather |
| 13 | GET | `/api/weather/image` | `legacy/server/providers/weather.js:453` | weather |
| 14 | GET | `/api/wind` and `/api/wind/manifest` | `legacy/server/providers/wind.js:187` | wind |
| 15 | GET | `/api/wind/status` | `legacy/server/providers/wind.js:212` | wind |
| 16 | GET | `/api/wind/grid/:id.bin` | `legacy/server/providers/wind.js:166` | wind |
| 17 | GET | `/api/cyclones` | `legacy/server/providers/cyclones.js:373` | cyclones |
| 18 | any | `/api/firms` | `legacy/server/providers/firms.js:177` | fires |
| 19 | any | `/api/firms/status` | `legacy/server/providers/firms.js:191` | fires |
| 20 | GET | `/api/fire-perimeters` | `legacy/server/providers/firePerimeters.js:191` | fire perimeters |
| 21 | GET | `/api/fire-perimeters/inciweb/index` | `legacy/server/providers/firePerimeters.js:195` | fire perimeters |
| 22 | GET | `/api/fire-perimeters/inciweb/publication/:id` | `legacy/server/providers/firePerimeters.js:199` | fire perimeters |
| 23 | GET | `/api/gbfs/:encodedUpstreamUrl` | `legacy/server/providers/gbfs.js:118` | gbfs |
| 24 | any | `/api/tomtom/status` | `legacy/server/providers/traffic.js:182` | traffic |
| 25 | any | `/api/tomtom/flow/:z/:x/:y.pbf` | `legacy/server/providers/traffic.js:194` | traffic |
| 26 | GET | `/api/transit/feeds` | `legacy/src/sources/transitService.js:242` | transit |
| 27 | GET | `/api/transit/vehicles/:feedId` | `legacy/src/sources/transitService.js:268` | transit |
| 28 | GET | `/api/transit/trail/:feedId/:vehicleId` | `legacy/src/sources/transitService.js:250` | transit history |
| 29 | any | `/api/terrain/heights?points=` | `legacy/server/providers/terrain.js:159` | terrain |
| 30 | GET | `/api/google/nearby-places` | `legacy/server/providers/places/google.js:66` | places |
| 31 | GET | `/api/google/text-search` | `legacy/server/providers/places/google.js:180` | places |
| 32 | any | `/api/route` | `legacy/server/providers/places/routes.js:229` | routing |
| 33 | GET | `/api/geocode` | `legacy/server/providers/regional/place.js:191` | geocode |
| 34 | GET | `/api/mapillary/status` | `legacy/server/providers/mapillary/routes.js:90` | mapillary |
| 35 | GET | `/api/mapillary/tiles/coverage/:z/:x/:y` | `legacy/server/providers/mapillary/routes.js:91` | mapillary |
| 36 | GET | `/api/overpass/status` | `legacy/server/providers/overpass.js:79` | overpass |
| 37 | POST | `/api/overpass` | `legacy/server/providers/overpass.js:91` | overpass |
| 38 | GET | `/api/military-installations` | `legacy/server/providers/military-installations.js:85` | installations |
| 39 | GET | `/api/regional-brief` | `legacy/server/providers/regional/briefing.js:73` | regional |
| 40 | GET | `/api/weather-effects` | `legacy/server/providers/regional/weather-effects.js:46` | regional |
| 41 | GET | `/api/radio/stations` | `legacy/server/providers/radio/catalog.js:346` | radio |
| 42 | POST | `/api/radio/click/:uuid` | `legacy/server/providers/radio/catalog.js:374` | radio |
| 43 | any | `/api/cctv/sources` | `legacy/server/providers/cctv.js:144` | cctv |
| 44 | any | `/api/cctv/health` | `legacy/server/providers/cctv.js:181` | cctv |
| 45 | any | `/api/cctv/stream/:id` | `legacy/server/providers/cctv.js:190` | cctv |
| 46 | GET | `/api/cctv/media/:id` (non-HLS relay, or HLS playlist with `?lease=`) | `legacy/server/providers/cctv.js:204` | cctv |
| 47 | GET | `/api/cctv/media/:id/seg_:n.ts?session=&lease=` | `legacy/server/providers/cctv.js:248` | cctv |
| 48 | DELETE | `/api/cctv/media/:id?lease=` | `legacy/server/providers/cctv.js:223` | cctv |
| 49 | any | `/api/cctv/frame/:id` | `legacy/server/providers/cctv.js:462` | cctv |
| 50 | GET/HEAD | `/api/local-receivers/aircraft` | `legacy/server/providers/local-receivers.js:507` | local receivers |
| 51 | GET/POST | `/api/realtime/token?auth=&tier=` | `legacy/server/providers/openai/realtime.js:130` | openai |
| 52 | GET | `/api/realtime/oauth-status` | `legacy/server/providers/openai/realtime.js:56` | openai |
| 53 | POST | `/api/realtime/oauth-login` | `legacy/server/providers/openai/realtime.js:82` | openai |
| 54 | POST | `/api/realtime/debug-log` | `legacy/server/providers/openai/debug-log.js:115` | openai |
| 55 | POST | `/api/openai/hud-summary` | `legacy/server/providers/openai/hud-summary.js:31` | openai |
| 56 | GET | `/api/setup/status` (dev only) | `legacy/server/standalone/key-setup.js:248` | key setup |
| 57 | POST | `/api/setup/keys` (dev only) | `legacy/server/standalone/key-setup.js:256` | key setup |
| 58 | POST | `/mcp` (GET etc. → 405) | `legacy/server/mcp/plugin.js:73` | mcp |
| — | any | `/api/*` unmatched → 404 JSON | `legacy/server/standalone/api-not-found.js:4` | catch-all |
| — | GET | `/panel/*` (dev only, static) | `legacy/build/panel.js:58` | panel |

Streaming summary: **no SSE and no server-side WebSocket endpoint**. Long-lived
responses are only `/api/cctv/media/:id` (chunked pass-through of MJPEG/MP4/other live
bodies) and HLS playlists/segments the browser polls. Voice uses WebRTC browser↔OpenAI
directly with the minted ephemeral secret
(`legacy/src/voice/realtimeBackend.js:5-6`, `legacy/src/voice/realtimeBackend.js:72-92`).

---

### 2.1 Aircraft (OpenSky, adsb.lol, adsbdb)

**#1 `/api/flights`** — `legacy/server/providers/aircraft/opensky.js:381-729`
- Upstream: `GET https://opensky-network.org/api/states/all?extended=1`
  (`opensky.js:30-31`); OAuth token `POST https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token`
  client_credentials (`opensky.js:119-195`); regional fallback
  `GET https://api.adsb.lol/v2/lat/{lat}/lon/{lon}/dist/250` with `lat`,`lon` rounded to 0.25°
  (`opensky.js:270-330`, URL at `:290`).
- Env: `OPENSKY_AUTH_MODE` (oauth default | basic | auto | anon; invalid → oauth, `:93-96`, `:203-216`),
  `OPENSKY_CLIENT_ID`, `OPENSKY_CLIENT_SECRET` (`:128-130`), `OPENSKY_USERNAME`,
  `OPENSKY_PASSWORD` (`:469-471`). Auto mode retries with Basic on 401/403 (`:509-523`).
- Auth/gate: none (no same-site gate). Host check only.
- Validation: optional `lat∈[-90,90]`, `lon∈[-180,180]` for the fallback anchor (`:259-268`); no method check.
- Timeouts: 10 s per attempt, one retry on throw (`:28-29`, `:41-57`); token request 10 s (`:140`); fallback 10 s (`:286-287`).
- Body caps: **OpenSky body uncapped** (`upstream.text()`, `:48`); fallback capped 8 MiB (`:104`, `:300-303`).
- Cache: module-global single body; base TTL 9 s (`:27`) adapted from `X-Rate-Limit-Remaining`
  (>2400 → 9 s, >1200 → 30 s, >400 → 90 s, else 300 s; `:82-88`, `:659-662`). Only 2xx cached (`:644-664`).
  Fallback cache: 80 anchors × 12 s, LRU-ish (`:98-103`, `:310-314`), single-flight per anchor.
  Token cached until `expires_in − 60 s` (`:122`, `:166-169`), coalesced refresh (`:126`, `:133-194`).
- Rate limiting / credit governor: on 429 a cooldown from `X-Rate-Limit-Retry-After-Seconds`
  bounded 30 s…30 min (default 2 min) (`:545-559`); during cooldown no upstream calls, stale body served
  or synthetic 429 (`:393-467`). If the snapshot `time` is >120 s old the adsb.lol regional fallback is
  preferred (`:105-108`, `:350-364`, `:400-411`, `:525-544`).
- Response: OpenSky JSON body passthrough `{time, states:[[...18 fields]]}`; fallback is normalised into
  the same 18-element state-vector shape (`legacy/src/data/adsbLolFallback.js:40-101`). Headers:
  `X-Feed-Source`, `X-OpenSky-Cache` (HIT/MISS/STALE/COOLDOWN/INFLIGHT), `X-OpenSky-Auth`,
  `X-OpenSky-Auth-Mode-Requested`, `X-OpenSky-Auth-Mode-Used`, `X-OpenSky-Auth-Reason`,
  optional `X-OpenSky-Stale-Seconds`, `X-OpenSky-Retry-After-Seconds` (`:231-257`); fallback adds
  `X-Flight-Source: adsb.lol`, `X-Flight-Coverage`, `X-Flight-Count` (`:332-348`).
- Errors: upstream status relayed with sanitised JSON `{error}` for 401/403 (`:589-625`); stale body
  on throw (`:678-698`); else regional fallback; else `502 {"error":"OpenSky proxy error"}` (`:711-720`).

**#2 `/api/flights/track?icao24=<hex6>`** — `legacy/server/providers/aircraft/tracks.js:68-94`
- Upstream: `GET https://opensky-network.org/api/tracks/all?icao24=…&time=0` with shared OAuth bearer if
  available (`tracks.js:82-88`).
- Validation: `/^[0-9a-f]{6}$/` else 400 (`:74-81`). Timeout 12 s (`:41`); body cap 5 MiB via
  `readCappedResponseText` (`:18`, `:43-46`).
- Cache: 60 s per key, max 200 entries, oldest evicted; **errors (incl. 502 too-large) are cached too**
  (`:16-28`, `:58-60`).
- Response: upstream JSON passthrough or `{error:"Track source HTTP <status>"}` with upstream status;
  `502 {"error":"OpenSky track fetch failed"}` on throw (`:89-93`).

**#3 `/api/military`** — `legacy/server/providers/aircraft/adsb-lol.js:17-162`
- Upstream: `GET https://api.adsb.lol/v2/mil` (`:101-103`) — **no timeout, body uncapped**.
- Cache: 12 s (`:27`). On 429/5xx: cooldown from `Retry-After` (seconds or date) clamped 5 s…120 s,
  defaults 30 s (429) / 15 s (5xx) (`:28-52`, `:113-131`); serve stale with
  `X-ADS-B-Upstream-Status`, `X-ADS-B-Retry-After-Seconds`.
- Response: adsb.lol JSON passthrough; headers `X-Feed-Source: adsb.lol`, `X-Feed-Cache`
  (HIT/MISS/STALE/NONE), `X-Feed-Age-Ms` (`:54-68`). Cold-cache failure relays upstream status/body
  (`:132-145`) or `503 {"error":"adsb.lol upstream cooling down"}` with `Retry-After` (`:92-99`);
  throw → stale or `502 {"error":"ADS-B proxy error"}` (`:146-154`).

**#4 `/api/military/track?hex=`** — `legacy/server/providers/aircraft/tracks.js:96-120`
- Upstream: `GET https://adsb.lol/data/traces/{hex[-2:]}/trace_full_{hex}.json` (`:113`), undocumented
  tar1090 trace (`:10-13`). Validation `/^[0-9a-f~]{6,7}$/` (`:102`). Same cache/caps as #2.

**#5/#6 `/api/adsbdb/route/:callsign`, `/api/adsbdb/type/:hex`** — `legacy/server/providers/aircraft/enrichment.js:10-147`
- Upstream: `https://api.adsbdb.com/v0/callsign/{cs}` / `.../v0/aircraft/{hex}`, 8 s timeout (`:79-83`);
  `res.json()` uncapped.
- Validation: callsign `/^[A-Z0-9]{2,8}$/` (upper-cased), hex `/^[0-9a-f]{6}$/`; other kinds → 404 (`:118-135`).
- Cache: 24 h TTL per key including **negative cache for null/404** (`:11`, `:89-96`); persisted to
  `.gev-cache/adsbdb.json` (cwd-relative) by a 15 s flush interval (`:12`, `:27-36`); in-flight dedupe (`:73-107`).
  **Maps grow without bound.**
- Response: `{found:true, airline, origin:{code,name,lat,lon}, destination:{…}}` or
  `{found:true, typeCode, typeName, registration}` or `{found:false}` (`:41-68`, `:126-133`);
  errors `400/404/500` JSON (`:136-139`).

### 2.2 Vessels (AISStream)

**#7 `/api/vessels?maxRows=`** — `legacy/server/providers/vessels/ais-live.js:72-153`
- Upstream: persistent WebSocket `wss://stream.aisstream.io/v0/stream` (overridable `AISSTREAM_URL`)
  (`:17`, `:241`), subscription `{APIKey, BoundingBoxes, FilterMessageTypes}` (`:370-382`), defaults
  worldwide bbox + 5 message types (`:18-30`). Uses the `ws` package (not Node's WebSocket) for
  `terminate()` semantics (`:178-205`).
- Env: `AISSTREAM_API_KEY`, `AISSTREAM_BOUNDING_BOXES` (JSON), `AISSTREAM_MESSAGE_TYPES` (CSV or JSON),
  `AISSTREAM_SILENCE_TIMEOUT_MS` (0 = off), `AISSTREAM_URL` (`:220-244`, `:384-407`).
- Validation: `maxRows` clamped 1…50,000 (`:111-116`; `legacy/server/providers/vessels/ais-store.js:2`).
- Store: see §4. Rows older than 30 min dropped (`ais-store.js:3`, `:221-229`).
- Response: `{rows:[{lat,lon,name,mmsi,imo,type,destination,speed,course,heading,last_position_UTC,last_position_epoch}], source, status, error, refreshing, newestPositionAt, lastMessageAt, silentForMs, reconnectAttempt, nextAttemptAt, staleAfterMs, watchdog}`
  (`ais-live.js:124-141`; row fields `ais-store.js:67-85`). Status **503 when no key** (body still JSON)
  (`ais-live.js:121`). `X-Feed-Source: AISStream`, `Cache-Control: no-store`. Throw → `502 {error, rows:[]}` (`:142-152`).
- Every request also drives the watchdog `ensure()` (`:77`, `:295-310`).

**#8 `/api/vessels/track?mmsi=`** — `legacy/server/providers/vessels/ais-live.js:83-109`
- Validation `/^\d{5,10}$/` else `400 {error, samples:[]}`. Response
  `{mmsi, samples:[{lat,lon,t}], source:"AISStream (accumulated since server start)", retainedSec:1800}`
  from in-memory ring buffers (64 samples, ≥30 s and ≥25 m apart; `ais-store.js:8-10`, `:115-181`).

### 2.3 Space (CelesTrak, Launch Library 2)

**#9 `/api/celestrak/:group`** — `legacy/server/providers/space/celestrak.js:23-139`
- Upstream: `https://celestrak.org/NORAD/elements/gp.php?GROUP=<group>&FORMAT=tle`
  (`legacy/src/data/spaceProviderRequests.js:2-7`) with a contact User-Agent (required by CelesTrak, `celestrak.js:55-60`); 20 s timeout; body uncapped.
- Validation: group `/^[a-z0-9-]+$/i` else `400 text/plain "invalid group"` (`:71-78`).
- Cache: memory + `.gev-cache/celestrak-<group>.json`, TTL 6 h, single-flight per group, body must
  contain a `^1 ` TLE line (`:24-67`, `:91-119`).
- Response: `text/plain` TLE text, header `x-tle-cache` HIT/MISS/STALE-ERROR/NONE/ERROR; 502/500 plain text on failure (`:79-131`).

**#10 `/api/launches`** — `legacy/server/providers/space/launch-library.js:21-144`
- Upstream: `https://ll.thespacedevs.com/2.3.0/launches/?net__gte=<now-30d>&net__lte=<now>&limit=100&mode=detailed`
  (`legacy/src/data/spaceProviderRequests.js:9-17`), optional `Authorization: Token $LL2_API_TOKEN`
  (`launch-library.js:12-18`); 20 s timeout; 12 MiB cap (`:23`, `:72-76`).
- Cache: 15 min memory + `.gev-cache/launch-library-2-v2.3.json` (≤24 MiB on read) (`:9`, `:24-58`); coalesced.
- Response: LL2 JSON passthrough (`results[]` required), `Cache-Control: public, max-age=900` on 200,
  `X-GEV-Cache` HIT/MISS/INFLIGHT/STALE-ERROR/NONE (`:60-67`). Non-GET 405. Failure → stale or
  upstream status / 502 with `{"error":"Launch Library 2 unavailable"}` (`:112-130`).

### 2.4 Observed weather imagery (NOAA nowCOAST WMS)

`legacy/server/providers/weather.js:238-610`, mount `/api/weather`.
- Upstream base `https://nowcoast.noaa.gov/geoserver/observations/` (`:4`); products `lightning`,
  `radar`, `clouds`, `clouds-regional` → fixed service/layer/style (`:6-53`).
  Capabilities `…/{service}/ows?service=WMS&version=1.3.0&request=GetCapabilities` (`:351`);
  GetMap WMS 1.1.1 EPSG:4326 PNG with fixed layer/style/bbox/time (`:546-560`). `redirect:'error'` (`:317`).
- Common validation: GET only; path ∈ {/manifest,/tile,/image} else 404; `req.url ≤ 512` chars; only
  allow-listed, non-repeated query keys (`:432-449`); `product` must be known (`:450-452`).
- **#11 `/manifest?product=`**: returns `{schemaVersion:1, product, title, coverage, description, source, attribution, bounds, times[≤13], latest, time, observedAt, fetchedAt, stale, unavailable, reason, tileSize:256, maxLevel:6, tilingScheme:'geographic', tileTemplate, imageUrl, imageSize}` (`:382-417`); upstream failure still 200 with `unavailable:true` (`:473-483`).
- **#12 `/tile?product&time&z&x&y[&size=256|512|1024]`**: z 0…6, geographic 2×1 root (`:201-219`, `:484-491`);
  `time` must be an exact advertised ISO time within 24 h (`:492-501`).
- **#13 `/image?product&time[&size=WxH][&bbox=w,s,e,n]`**: sizes 1024x512/2048x1024/4096x2048 ≤ product max;
  bbox snapped to 0.25°, 2:1 aspect, inside product bounds (`:56-58`, `:175-198`, `:502-509`).
- Validation of upstream: capabilities ≤512 KiB, no DOCTYPE/ENTITY, ≤2048 Layer tags, depth ≤16,
  exactly one ISO8601 time dimension, ≤512 times, keep last 26 within 24 h (`:82-170`); PNG must be
  `image/png` with IHDR matching requested size (`:221-235`, `:320-334`); image cap 16 MiB, tile cap
  `max(1 MiB, w²·4+64 KiB)` (`:60`, `:525-531`).
- Concurrency/cancellation: shared operation per key, ≤8 active, ≤120 operations / ≤96 pending else
  `429 weather_busy` + `Retry-After: 2`; an operation aborts only when its last waiter disconnects;
  12 s operation timeout (`:241`, `:252-312`, `:418-425`).
- Cache: metadata TTL 120 s (lightning 600 s), stale metadata usable ≤1 h, retry gate 30 s
  (`:17`, `:338-370`); tiles/images LRU ≤128 entries / ≤16 MiB, 24 h; failure memo 30 s per key
  (`:371-381`, `:536-577`). Stale metadata never triggers new GetMap (`:542`).
- Response: `image/png`, `Cache-Control: public, max-age=86400, immutable` (`:579-584`); errors JSON
  `{error:<code>}` with 400/429/503 (`:585-596`).

### 2.5 Wind & model fields (GFS / IFS GRIB2)

`legacy/server/providers/wind.js:7-227`, mount `/api/wind`.
- Query: `model ∈ {gfs, ifs}` (default gfs), `overlay ∈ {none, temperature, pressure}` (`:157-165`); GET only.
- Upstreams:
  - GFS: `https://noaa-gfs-bdp-pds.s3.amazonaws.com/gfs.{YYYYMMDD}/{HH}/atmos/gfs.t{HH}z.pgrb2.0p25.f{FFF}`
    and its `.idx` (`legacy/server/providers/wind/catalog.js:1-7`; `legacy/server/providers/wind/gfs.js:185-186`);
    cycle = latest 6-hourly run ≥5 h old, forecast step nearest now (hourly ≤f120, 3-hourly ≤f384)
    (`catalog.js:10-34`). Byte-range GETs for `UGRD`/`VGRD` @10 m, optional `TMP` @2 m or `PRMSL`
    (`gfs.js:44-70`, `:198-210`).
  - IFS: `https://data.ecmwf.int/forecasts/{date}/{HH}z/ifs/0p25/oper/{date}{HH}0000-{step}h-oper-fc.grib2`
    + `.index` JSON-lines (`legacy/server/providers/wind/ifs.js:6`, `:30-66`); params `10u`,`10v`,`2t`,`msl` (`:73-86`).
- Limits: index ≤2 MiB, range ≤8 MiB, 206 required (200 only if start=0), exact length,
  `redirect:'error'` (`gfs.js:73-119`); optional scalar has its own 12 s budget and failure keeps wind (`gfs.js:122-153`).
- Decode: `@meri-imperiumi/eccodes-wasm` lazily loaded, per-message ≤8 MiB, ≤2 M points
  (`legacy/server/providers/wind/decode.js:1-51`); bilinear resample to 1° grid 360×181 with
  lon wrap, K→°C and Pa→hPa (`legacy/server/providers/wind/grid.js:12-153`).
- Refresh: single-flight per `model:overlay`, 40 s timeout, abort when last waiter leaves, 60 s
  retry gate, TTL 1 h, grid validation (finite, ≤1 M points) (`wind.js:11-13`, `:38-154`, `:189-210`).
- **#14 `/` or `/manifest`**: `{schemaVersion, model, cycle:{date,hour,forecastHour,runIso,validIso}, overlay?, scalar?:{kind,units,level}, scalarError?, fetchedAt, level, units, grid:{nx,ny,lo1,la1,dx,dy}, stale, unavailable, reason, gridUrl:"/api/wind/grid/<id>.bin?model=…"}` (`wind.js:99-123`); unavailable manifest otherwise (`:21-30`).
- **#15 `/status`**: same minus `gridUrl` (`:212-215`).
- **#16 `/grid/<id>.bin?model&overlay`**: `application/octet-stream`, concatenated **Float32 (host
  endianness)** U, V, [scalar]; `Cache-Control: public, max-age=3600, immutable`; last 2 generations
  kept per slot; unknown id → `404 {"error":"unknown_grid"}` (`:126-130`, `:166-186`).
- Other paths → 404 JSON; non-GET 405; bad model/overlay 400 (`:160-165`, `:187-188`).

### 2.6 Tropical cyclones (NHC / CPHC)

**#17 `/api/cyclones`** — `legacy/server/providers/cyclones.js:251-416`
- Upstream: `https://www.nhc.noaa.gov/CurrentStorms.json` (≤128 KiB) then three ArcGIS GeoJSON
  queries on `mapservices.weather.noaa.gov/.../NHC_tropical_weather_summary/MapServer/{5,6,7}/query`
  (points ≤512 KiB/500, track ≤512 KiB/32, cone ≤2 MiB/32) (`:3-13`, `:279-315`); `redirect:'error'`, UA set (`:259-278`).
- Validation: request must be exactly `/` (no query) else 400 (`:389-390`); GET only.
  Upstream schema validated strictly (ids `al|ep|cp\d{6}`, times within 12 h, geometry budgets ≤25k points,
  matching advisory numbers) (`:15-133`, `:135-248`).
- Cache: 5 min; retry gate 60 s; whole refresh 12 s; ≤32 waiters else `429 cyclone_busy` +
  `Retry-After: 2`; last-good served ≤12 h with `stale:true` (`:320-354`, `:391-401`).
- Response: `{schemaVersion, source, attribution, coverage, fetchedAt, stale, unavailable, reason, storms:[{id,name,classification,basin,position:{longitude,latitude},positionAt,advisoryNumber,issuedAt,windKt,pressureHpa,movement,advisoryUrl,outlookUrl,geometryStatus,geometryAdvisoryNumber,forecastPoints,track,cone}]}`
  (`:104-131`, `:355-372`). Failures still 200 with `unavailable:true`.

### 2.7 Active fires (NASA FIRMS)

**#18 `/api/firms`, #19 `/api/firms/status`** — `legacy/server/providers/firms.js:29-267`
- Upstream: `https://firms.modaps.eosdis.nasa.gov/api/area/csv/{FIRMS_MAP_KEY}/{SOURCE}/world/2` for
  `VIIRS_NOAA20_NRT`, `VIIRS_NOAA21_NRT`, `VIIRS_SNPP_NRT`, `MODIS_NRT`, **sequentially**, 60 s each,
  **body uncapped** (`:32-37`, `:84-91`, `:99-121`); key status
  `…/mapserver/mapkey_status/?MAP_KEY=` 10 s, cached 5 min (`:140-174`).
- Env: `FIRMS_MAP_KEY` (in URL path — never logged, `:80-83`).
- Cache: 30 min memory + `.gev-cache/firms.json`, single-flight, partial success allowed; rows re-filtered
  to trailing 24 h at serve time (`:30`, `:38-77`, `:127-137`, `:220-255`;
  `legacy/src/data/firmsCsv.js:73-127`, `:168`).
- Response `/`: `{fetchedAt, stale, ttlMs, sources:[{source,count,ok}], count, fires:[{lat,lon,frp,confidence,brightness,brightnessTi5,daynight,acqDate,acqTime,satellite,instrument}]}`;
  keyless `503 {"error":"no_key"}`; total failure `502`; `/status` → `{hasKey,lastFetch,count,stale,ttlMs,transactions:{used,limit}|null}` (`:191-218`).

### 2.8 Fire perimeters (NIFC WFIGS, InciWeb)

`legacy/server/providers/firePerimeters.js:50-232`, mount `/api/fire-perimeters`, GET only (`405` with `Allow: GET`).
- **#20 `/`**: WFIGS ArcGIS `…/WFIGS_Interagency_Perimeters_Current/FeatureServer/0/query` with fixed
  outFields, `maxAllowableOffset=0.001`, GeoJSON; follows `exceededTransferLimit` up to 5 pages,
  16 MiB/30 s each (`:12-45`, `:73-86`). Normalised rows
  `{stableId,name,acres,containedPct,state,category,discoveredTime,updatedTime,cause,behavior,personnel,county,costToDate,complexity,complexName,polygons}`
  (`legacy/src/layers/perimeters/records.js:42-94`); response `{fetchedAt, rows, stale?}`. TTL 5 min.
- **#21 `/inciweb/index`**: `POST https://inciweb.wildfire.gov/api/single-publication/` body `{"title":""}`,
  4 MiB/20 s, TTL 1 h, array passthrough (`:88-101`, `:195-198`).
- **#22 `/inciweb/publication/:id`** (`/^\d{1,9}$/`): fetches `https://inciweb.wildfire.gov/node/{id}` HTML,
  follows **one** same-host redirect manually with path regex + https upgrade, scrapes
  `og:updated_time` and "Date of Origin" → `{createdMs, changedMs}`; 2 MiB/15 s; TTL 30 min; ≤256 entries (`:104-148`, `:199-206`).
- Rate limit: 60/min/IP, 1200 global (`:57`, `:208`) → `429 {"error":"rate_limited"}` + `Retry-After: 60`.
- Coalescing; ≤256 distinct in-flight else 429; stale served with `X-Data-Stale: true`; failure
  `502 {"error":"fire_perimeters_unavailable"}` (`:150-221`).

### 2.9 Bike share (GBFS)

**#23 `/api/gbfs/<percent-encoded https URL>`** — `legacy/server/providers/gbfs.js:116-249`
- The **client supplies the upstream URL**, constrained by: https only, host allowlist
  (`gbfs.lyft.com`, `gbfs.bluebikes.com`, `gbfs.bcycle.com`, `gbfs.biketownpdx.com`,
  `gbfs.cogobikeshare.com`, `*.publicbikesystem.net`) and path ending `station_information.json` or
  `station_status.json` (`legacy/src/data/gbfsSource.js:2-38`; `gbfs.js:164-196`). Query string is passed through.
- Redirects refused (`redirect:'manual'` → 502), 12 s timeout, 5 MiB streaming cap (`gbfs.js:12-14`, `:46-104`).
- No cache server-side; `Cache-Control: public, max-age=300` for station_information, `no-store` for status
  (`gbfsSource.js:49-54`); headers `X-GBFS-Upstream`, `X-GBFS-Cache: MISS`; upstream status and content-type relayed.
- Errors: 400/403/405 JSON; timeout 504; redirect/too-large/other 502 (`gbfs.js:206-241`).

### 2.10 Traffic (TomTom)

`legacy/server/providers/traffic.js:43-274`, mount `/api/tomtom`.
- **#25 `/flow/{z}/{x}/{y}.pbf`**: upstream
  `https://api.tomtom.com/traffic/map/4/tile/flow/relative/{z}/{x}/{y}.pbf?key=$TOMTOM_API_KEY`, 15 s,
  body uncapped (`:141-153`). z 8…16, x/y in range (`legacy/src/data/tomtomTiles.js:17-51`).
  Keyless → `503 {"error":"no_key"}` (`:206-209`).
- Cache: memory ≤256 tiles + disk `.gev-cache/tomtom/flow-z-x-y.pbf` (mtime = age; **never swept**), TTL 120 s,
  single-flight, serve stale on error (`:44-139`, `:211-262`).
- Budget governor: persistent UTC-day counter `.gev-cache/tomtom/budget.json`, cap
  `TOMTOM_DAILY_TILE_BUDGET` (default 6000/day); attempts count; over budget → stale tile or
  `429 {"error":"budget"}` (`:46-102`, `:225-233`).
- Response: `application/x-protobuf` (raw MVT), `Cache-Control: no-store`, `x-tomtom-cache`
  HIT/MISS/STALE-BUDGET/STALE-ERROR (`:168-176`); 404/400/502/500 JSON.
- **#24 `/status`**: `{hasKey, dailyCount, budget, date}` (`:182-192`).

### 2.11 Transit (GTFS-Realtime) and transit history

`legacy/server/providers/transit.js:5-24` adapts the fetch-style service
`legacy/src/sources/transitService.js:110-406`; mount `/api/transit`, GET only (405).
- Feeds: 7 registered keyless feeds, browser names an id never a URL — `mbta`, `capmetro-austin`,
  `metrotransit-msp`, `hsl-helsinki`, `ovapi-nl`, `entur-norway` (sends `ET-Client-Name`), `translink-seq`
  (`legacy/src/data/transitFeeds.js:131-265`, `:280-281`).
- **#26 `/feeds`**: `{feeds:[{id,name,operator,region,center,loadRadiusKm,license,licenseUrl,attribution,historyRetention}]}`,
  `Cache-Control: public, max-age=3600` (`transitService.js:242-247`; `transitFeeds.js:391-404`).
- **#27 `/vehicles/:feedId`**: fetch with conditional headers (ETag/Last-Modified), gzip, manual
  redirects only within the feed's https origin, ≤3 hops (`transitService.js:48-84`;
  `legacy/src/data/transitProxy.js:113-170`); 15 s timeout, 8 MiB cap (`transitProxy.js:24-32`);
  protobuf decoded server-side with ≤50k entities and ≤256-char strings, differential feeds refused
  (`legacy/src/data/gtfsRealtime.js:62-67`, `:195-274`; `transitProxy.js:242-265`).
  Response `{feedId,name,fetchedAt,feedTimestamp,version,entityCount,truncated,count,vehicles:[{id,lat,lon,bearing,speedMps,timestamp,timestampSource,routeId,tripId,directionId,label,stopId,status,occupancy}]}`.
  Headers `X-GEV-Cache` (HIT/MISS/INFLIGHT/STALE-ERROR/NONE), `X-Transit-Upstream`, `X-Transit-Contact`,
  `Cache-Control: public, max-age=15` (or no-store when stale) (`transitProxy.js:289-310`).
- Cache: 15 s fresh, ≤10 min stale-on-error, no disk (`transitProxy.js:24-26`, `:273-280`).
- Rate limiting: per-feed admission 8/min, 40/min global (`transitProxy.js:56-58`;
  `transitService.js:124-128`); failure cooldown ladder 5 s/15 s/60 s/300 s (`transitProxy.js:65-67`,
  `transitService.js:135-141`, `:312-321`). Unavailable → `503` with `Retry-After`, `X-Transit-Backoff`;
  failures 502/504 with `{error, feedId, retryInSec}` (`transitService.js:280-394`).
- **#28 `/trail/:feedId/:vehicleId`** (only feeds with `historyRetention`, i.e. `mbta`): served from the
  in-process history store, never upstream; 30/min per feed, 120 global → 429 `Retry-After: 60`
  (`transitService.js:114-118`, `:250-267`; `transitProxy.js:76-96`). Response
  `{version:1, feedId, vehicleId, oldestT, newestT, truncated, epochs, fixes}` ≤32 KiB
  (`legacy/src/sources/transitHistoryStore.js:429-468`).

### 2.12 Terrain heights (Re:Earth)

**#29 `/api/terrain/heights?points=lon,lat;…`** — `legacy/server/providers/terrain.js:24-219`
- Upstream: `https://terrain.reearth.land/heights.json?points=…` (`legacy/src/data/terrainHeightsProxy.js:125`),
  64 points per call sequentially (`terrain.js:35`, `:125-144`), 30 s first attempt then ≤3 jittered retries
  inside a 10 s budget honouring `Retry-After` (`terrainHeightsProxy.js:9-12`, `:111-185`). Body uncapped.
- Validation: finite numbers only — **no lat/lon range check** (`terrainHeightsProxy.js:24-42`);
  >2000 points → **500** (not 400) (`terrain.js:36`, `:177-182`); malformed → 400 (`:170-176`).
- Cache: per-point (5 dp key) 30-day TTL, memory **unbounded** + `.gev-cache/terrain-heights.json`
  (version 2) flushed every 15 s; legacy batch format migrated (`terrain.js:25-107`); single-flight per
  missing set (`:147-156`).
- Response: `{results:[{…ellipsoid…}]}` in exact request order, or `502` if any point lacks a height
  (`terrainHeightsProxy.js:191-278`). Content-Type JSON, no Cache-Control.

### 2.13 Places (Google) and routing (OSRM) and geocode (Nominatim)

**#30 `/api/google/nearby-places?lat&lon[&radiusM]`** — `legacy/server/providers/places/google.js:66-174`
- Same-site gate first (`:69`), GET only, keyless → `200 {configured:false,error:null,places:[]}` before
  rate limiting (`:80-88`; `legacy/server/providers/places/google-key.js:8-14`).
- Upstream: `POST https://places.googleapis.com/v1/places:searchNearby` with `X-Goog-Api-Key`,
  field mask, `maxResultCount:20`, `rankPreference:DISTANCE`, circle radius clamped 25…5000 m (default 250)
  (`:112-149`); `redirect:'error'`; **no timeout**, JSON uncapped.
- Key: `GOOGLE_MAPS_SERVER_API_KEY` else `GOOGLE_MAPS_API_KEY`
  (`legacy/scripts/google-server-key.mjs:2-5`; `google-key.js:24-26`).
- Rate limit: `GEV_RATELIMIT_GOOGLE_PER_MIN` default 120/min/IP (0 = off), global ×20
  (`:26-36`, `:103-110`) → `429 {error, places:[]}` + `Retry-After: 5`.
- Response: `{places:[{id,name,address,latitude,longitude,distanceM,primaryType,types}] (≤20, deduped, landmark-priority), error}`;
  `Cache-Control: private, max-age=300`; upstream non-OK relays status with Google's `error.message`;
  throw → 502 (`:150-173`; `legacy/src/data/placeProviderPayloads.js:21-62`).

**#31 `/api/google/text-search?q&lat&lon[&radiusM]`** — `google.js:180-295`
- As #30, `POST …/places:searchText`, `maxResultCount:5`, `locationBias` radius 50…50000 m (default 4000),
  field mask adds viewport (`:234-270`). Response adds `viewport:{low,high}` (`placeProviderPayloads.js:66-114`).
  Missing `q` → 400.

**#32 `/api/route?profile=foot|car|bike&coords=lon,lat;…[&steps=1]`** — `legacy/server/providers/places/routes.js:218-370`
- Upstream: `https://routing.openstreetmap.de/routed-{profile}/route/v1/{foot|driving|bike}/{coords}?overview=full&geometries=geojson&alternatives=false&steps=…`
  (`:166-168`, `:293-295`), 12 s, `redirect:'error'`, 8 MiB cap, JSON content-type required (`:155-216`).
- Validation: 2–12 coordinates in range, each leg ≤600 km, total ≤2500 km (`:251-290`); profile aliases
  (`legacy/src/data/placeProviderPayloads.js:117-125`).
- Rate limits: 60/min/IP, 200 global (`:105-109`); global outbound gate 1 req/s with queue ≤8 → 429
  (`:41-98`); upstream 429 → 429 with its Retry-After (`:182-188`, `:338-352`).
- Cache: 10 min, ≤200 entries; in-flight dedupe keyed by base+route+steps; stepless can ride a stepful flight (`:16`, `:300-362`).
- Response: `{ok:true, profile, distanceM, durationS, geometry:[[lon,lat]…], steps?, stepsTruncated?}`;
  **validation and "no route" errors are HTTP 200 `{ok:false,error}`** (`:230-233`, `:355`, `:365-368`).
- Installed by the Overpass plugin, not its own plugin (`legacy/server/providers/overpass.js:234`).

**#33 `/api/geocode?q[&bounds=swLat,swLng|neLat,neLng]`** — `legacy/server/providers/regional/place.js:183-263`
- Upstream: `https://nominatim.openstreetmap.org/search?format=jsonv2&q&addressdetails=1&limit=1&accept-language=en[&viewbox]`
  with identifying UA + Referer, `redirect:'error'`, 9 s / 2 MiB (`:15-19`, `:127-178`;
  `legacy/server/providers/regional/http.js:6`, `:26-46`).
- Limits: q 1–200 chars → else 400 (`:49`, `:205-215`); 30/min/IP, 90 global (`:184-188`); **process-wide
  pacer** ≥1.1 s between calls, ≤4 pending, abandon after 10 s or client disconnect (`:25-43`, `:85-110`, `:218-220`).
- Cache: 5 min, ≤80 queries, in-flight dedupe (`:45-47`, `:131-177`).
- Response: Google-geocoder-shaped `{status:"OK"|"ZERO_RESULTS", results:[{geometry:{location,bounds?,viewport?},formatted_address,types,address_components}]}`
  (`legacy/src/nominatimGeocode.js:124-147`); busy → 429 + `Retry-After: 5`; else 503 (`place.js:233-250`).

### 2.14 Street-level imagery (Mapillary)

`legacy/server/providers/mapillary/routes.js:84-95`; both routes are **same-site gated**.
- **#34 `/api/mapillary/status`**: `{configured:boolean}` (`:14-18`).
- **#35 `/api/mapillary/tiles/coverage/{z}/{x}/{y}`**: upstream
  `https://tiles.mapillary.com/maps/vtp/mly1_public/2/{z}/{x}/{y}?access_token=$MAPILLARY_CLIENT_TOKEN`
  (`legacy/server/providers/mapillary/tiles.js:327`); zooms 0–5 or 11–14 only (`legacy/server/providers/mapillary/constants.js:7-16`);
  manual redirects pinned to https tile origin ≤3 hops (`tiles.js:285-320`); 60 s; 48 MiB cap (`constants.js:42-47`);
  ≤6 concurrent upstream, rest queued (`constants.js:52`, `tiles.js:378-402`); the `image` layer is stripped from the MVT
  (`legacy/server/providers/mapillary/trim.js:25-43`).
- Cache: memory LRU 96 MiB (entries ≤48 MiB), disk `.gev-cache/mapillary/tiles/{layer}/{z}/{x}-{y}.pbf`
  24 h TTL with atomic temp+rename writes and a ≤1 GiB sweep at most every 15 min (`constants.js:19-46`, `tiles.js:112-282`);
  shared flight cancelled when last waiter leaves (`tiles.js:444-493`).
- Rate limit 600/min/IP, 2400 global (`constants.js:58`, `routes.js:85-89`); upstream 401/403 holds misses 5 min,
  429 holds per Retry-After ≤10 min (`constants.js:63-67`, `tiles.js:54-77`, `:338-353`).
- Response: `application/x-protobuf` 200 (204 when empty/404 upstream), `Cache-Control: public, max-age=3600`,
  `X-Gev-Cache: memory|disk|inflight|upstream`; errors 400/403(keyRejected)/429/502/503 JSON (`routes.js:21-78`).

### 2.15 Overpass (OSM) and military installations

**#36 `GET /api/overpass/status`** → `{configured: OVERPASS_UPSTREAMS non-empty}` (`legacy/server/providers/overpass.js:79-90`).

**#37 `POST /api/overpass`** — `legacy/server/providers/overpass.js:71-241`
- Body: `application/x-www-form-urlencoded` with exactly one `data=` Overpass QL, ≤24 KiB (413) (`:97-115`;
  `legacy/server/providers/overpass/constants.js:73`).
- QL sanitizer (security-critical): strips comments/literals, rejects `around` > 50 km, bbox span >12°,
  `foreach|complete|retro|compare|convert|make`, `poly:`, element-in-area scans, and any selector not
  bounded by around/bbox/is_in/area(id)/bounded set; clamps `[timeout:]` ≤30 s
  (`legacy/server/providers/overpass/query.js:36-149`; `constants.js:106-134`).
- Upstreams: **only** operator-configured `OVERPASS_UPSTREAMS` (≤8 http(s) URLs; URL userinfo becomes
  Basic auth and is stripped) — public mirrors are not used by default (`constants.js:10-38`;
  `legacy/server/providers/overpass/transport.js:109-204`, auth at `:131-140`). Unconfigured → stale or
  `503 {code:"OVERPASS_NOT_CONFIGURED"}` (`overpass.js:129-137`; `transport.js:66-76`).
- Failover with per-endpoint cooldowns (Retry-After or 30 s·2^n ≤5 min), 22 s timeout, 32 MiB cap,
  rate-limit/runtime-error body detection, `redirect:'error'` (`transport.js:19-61`, `:79-204`; `constants.js:66`, `:85`).
- Cache: memory 24 h ≤120 entries; disk `.gev-cache/overpass/<sha1>.json` 7 d (30 d for `is_in`/`pivot`
  "boundary" queries), **never swept**; only 2xx data cached; serve stale at any age on failure
  (`constants.js:48-69`; `legacy/server/providers/overpass/cache.js:18-127`; `overpass.js:156-231`).
- Rate limit: 90/min/IP, 300 global, consumed only on true upstream misses (`overpass.js:30-34`, `:138-155`);
  ≤6 concurrent upstream else `503` + `Retry-After: 2` (`:176-187`).
- Response: Overpass JSON (simplified by Douglas–Peucker 0.0004° when >1.5 MB and ≥1200 points per geometry,
  `legacy/server/providers/overpass/geometry.js:9-101`; `constants.js:89-100`), headers `X-Overpass-Cache`
  (HIT/MISS/INFLIGHT/DISK/STALE/DISABLED), `X-Overpass-Cached-At`, `Retry-After`, `Cache-Control: public, max-age=15` for data (`overpass.js:43-58`).

**#38 `GET /api/military-installations?south&west&north&east[&exact=1]`** — `legacy/server/providers/military-installations.js:36-232`
- Validation: all four required, non-dateline, ≤10°×10° (`legacy/server/providers/military-installations/query.js:48-65`);
  bbox snapped outward to 0.05° unless `exact=1` (`query.js:10-46`).
- Upstream: fixed QL `nwr["military"~"^(airfield|naval_base|range|barracks|base)$"]` + `landuse=military`, `out center tags geom 700`
  through the Overpass transport, 16 MiB cap (`military-installations.js:37-43`; `military-installations/constants.js:9-50`).
- Rate limit 90/min/IP, 300 global (`:28-32`); cache memory 5 min ≤80, disk 30 d (`.gev-cache/military-installations/<sha1>.json`),
  stale ≤1 h memory or any-age disk (`constants.js:9-40`; `legacy/server/providers/military-installations/cache.js:32-48`).
- Response: `{elements, saturated, elementCap:700, retrievedAt, status:"ready"|"cached"|"stale"}` + `X-Military-Installations`
  (HIT/DISK/MISS/INFLIGHT/STALE/STALE-DISK/DISABLED); failure `503 {error, reason, retryAfterMs}` (`:84-220`).

### 2.16 Regional briefing, weather effects

**#39 `GET /api/regional-brief?latitude&longitude`** — `legacy/server/providers/regional/briefing.js:22-153`
- Sources: offline Natural Earth polygon lookup (`legacy/server/providers/regional/place.js:113-117`;
  `legacy/src/data/naturalEarthRegions.js:126-129` loads `src/data/local_data/natural_earth/{regions,marine}.json`);
  Open-Meteo current weather `https://api.open-meteo.com/v1/forecast?...` 512 KiB (`legacy/server/providers/regional/weather.js:6-25`);
  news: Google News RSS `https://news.google.com/rss/search?q=…` then GDELT
  `https://api.gdeltproject.org/api/v2/doc/doc?...` fallback, 12 s each (`legacy/server/providers/regional/news.js:60-110`).
- Validation: both coordinates required and in range (`legacy/server/providers/regional/query.js:3-10`); GET only.
- Rate limit 30/min/IP, 90 global → 429 `Retry-After: 10` (`briefing.js:27-31`, `:79-86`).
- Cache key 0.1° grid, 5 min, ≤120 entries, stale ≤1 h, coalesced (`:11-15`, `:98-140`).
- Response: `{status:"ready"|"partial"|"cached"|"stale", retrievedAt, coordinates, place, placeStatus, weather:{observedAt,temperatureC,apparentTemperatureC,precipitationMm,cloudCoverPct,windKph,windDirectionDeg,visibilityM,weatherCode}, weatherStatus, newsStatus, newsQuery, newsSource, articles:[{title,url,domain,publishedAt,sourceCountry}]}`
  (`:53-66`; `legacy/src/data/regionalModel.js:91-119`); all-source failure → 503.

**#40 `GET /api/weather-effects?latitude&longitude`** — `legacy/server/providers/regional/weather-effects.js:30-128`
- Open-Meteo only; 45/min/IP, 120 global; 5 min cache ≤180, stale ≤30 min; response
  `{status, retrievedAt, coordinates, weather}` + `X-Weather-Effects` (`:6-20`, `:31-114`).

### 2.17 Radio (Radio Browser)

`legacy/server/providers/radio/catalog.js:47-394` (mounted at `/api/radio` by `legacy/server/providers/radio.js:9-19`). Not an audio relay — the browser plays stream URLs directly (`legacy/SECURITY.md:68`).
- **#41 `GET /api/radio/stations`**: builds a ≤750-station catalog from 9 Radio Browser searches
  (`/json/stations/search` with tags news/talk/weather/emergency/scanner/aviation/marine/traffic + broad),
  concurrency 3, mirrors discovered from `https://all.api.radio-browser.info/json/servers` (6 h) with fallback
  `de1/de2/nl1.api.radio-browser.info` (`catalog.js:100-303`; `legacy/server/providers/radio/constants.js:1-18`).
  Destinations allowlisted (`*.api.radio-browser.info`, specific paths), DNS resolved and **every address must be
  public**, TLS connection pinned to the validated address, redirects refused, 12 s / 4 MiB
  (`legacy/server/providers/radio/transport.js:4-145`; `catalog.js:64-98`).
  Cache 45 min, stale ≤7 d, health policy (≥5 successful queries, ≥375 stations) before replacing a warm catalog (`constants.js:1-10`, `catalog.js:226-334`).
  Response `{stations:[{id,name,lat,lon,streamUrl,homepage,tags,languages,state,country,countryCode,metadataTrust,codec,bitrate}], updatedAt, stale, degraded, degradedReason, coverage, acceptedGeneration, catalogInstance}`;
  failure `503` (`catalog.js:346-371`; `legacy/src/sources/radioBrowser.js:149-166`). Non-GET → 405 `Allow: GET`.
- **#42 `POST /api/radio/click/:uuid`**: id must be in the currently served catalog → `204` then fire-and-forget
  `GET /json/url/{uuid}` on a mirror; else 404 (`catalog.js:374-390`). Other paths → `404 {"error":"Unknown radio route"}`.

### 2.18 CCTV (catalog, frames, media relay, HLS)

`legacy/server/providers/cctv.js:36-569`, mount `/api/cctv`. Every request first resolves the merged catalog
(`cctv.js:138-141`).
- Catalog: 14 live open-data packs (Austin, Caltrans, TfL, Ontario 511, Fintraffic, DriveBC, TxDOT, Tallinn,
  Tarktee, Warendorf, NSW, Calgary, DelDOT, Vegvesen) each with env kill switch and per-pack cap, plus
  `CCTV_SOURCES_FILE` (default `config/cctv_sources.austin.json`, currently `[]`) and `CCTV_SOURCES_JSON`;
  live packs load only when file+env are empty (or `CCTV_FORCE_AUSTIN=1`); round-robin cap
  `CCTV_MAX_SOURCES` (default 4000, ceiling 5000); 15-min TTL, single-flight, serve stale on empty refresh
  (`legacy/server/providers/cctv/catalog.js:24-287`; `legacy/server/providers/cctv/constants.js:1-16`, `:279-284`;
  loaders `legacy/server/providers/cctv/sources.js:102-1865`). Upstream catalog URLs are in `constants.js:3-271`
  (e.g. `data.austintexas.gov`, `cwwp2.dot.ca.gov`, `api.tfl.gov.uk` + optional `TFL_APP_KEY` (`sources.js:326-333`),
  `511on.ca`, `tie.digitraffic.fi`, `www.drivebc.ca/api/webcams/`, `its.txdot.gov`, `tarktee.transpordiamet.ee`
  (DATEX2 XML), `data.livetraffic.com`, `data.calgary.ca`, `tmc.deldot.gov`, `ogckart-sn1.atlas.vegvesen.no`).
  Per-pack 15 s timeout; several loaders use uncapped `resp.json()`.
  Ground heights sidecar `src/data/local_data/cctv_ground_heights/…json` joined in (`legacy/server/providers/cctv/groundHeights.js:6-40`).
- **#43 `/sources`**: `{trimmedPacks:[{pack,available,served,region}], sources:[{id,pack,name,city,cityId,provider,lat,lon,headingDeg,headingConfidence,pitchDeg,fovDeg,rangeM,mountHeightM,groundElevationM,feedType,sourceKind,poseSource,license,credit,code,groundHeights}]}` — upstream URLs are **not** exposed (`cctv.js:144-179`).
- **#44 `/health`**: `{cameras:[{id,status,sourceKind,label,message,updatedAt}]}` (≤5000 entries) (`cctv.js:39-66`, `:181-188`).
- **#45 `/stream/:id`**: `{id, feedType, mediaUrl:"/api/cctv/media/<id>"|null, frameUrl:"/api/cctv/frame/<id>", provider, sourceKind}` (`cctv.js:69-82`, `:190-202`).
- **#46 `GET /media/:id`** non-HLS (`mp4`,`webm`,`mjpeg`,`image`): fetches the **registered** URL only,
  forwards a sanitised single `Range` (≤64 MiB span) (`legacy/server/providers/cctv/range.js:42-81`),
  15 s header deadline, cancels upstream on client disconnect, **follows redirects (fetch default)**
  (`legacy/server/providers/cctv/media.js:326-348`); rejects declared Content-Length >64 MiB, otherwise
  **pipes the body unbounded** (live MJPEG etc.) with a 30 s inter-chunk idle deadline; relays
  Content-Type/Length/Range/Accept-Ranges/Cache-Control + `X-CCTV-Source` (`media.js:111-231`;
  `constants.js:293-301`). Errors: upstream status JSON, 404 no URL, 504 timeout, 502 (`cctv.js:318-459`).
- **#46 `GET /media/:id?lease=<uuid>`** HLS: lease UUID required (`cctv.js:218-222`); only `http(s)…​.m3u8`
  (`:235-247`); in-process puller (≤2 sessions, ≤8 leases each, 2 s poll, 15 s lease idle, ≤12 segments /
  24 MiB per session, playlist 256 KiB, segment 4 MiB, redirects refused, same-origin segment URIs, MPEG-TS only,
  no KEY/MAP/BYTERANGE) (`legacy/server/providers/cctv/stream.js:4-348`); waits ≤12 s for 2 segments;
  returns a rewritten playlist pointing to #47 with `X-CCTV-Session` (`cctv.js:262-316`; `stream.js:295-320`); failure `503` + `Retry-After: 2`.
- **#47 `GET /media/:id/seg_<n>.ts?session&lease`**: `video/mp2t` from memory or 404 (`cctv.js:248-261`; `stream.js:321-328`).
- **#48 `DELETE /media/:id?lease=`**: releases lease → 204 (`cctv.js:223-228`).
- **#49 `/frame/:id[?label&city&lat&lon&heading&fov&pitch]`**: fallback chain: registered snapshot
  (TxDOT JSON-base64 decoder pinned to `its.txdot.gov` path, `media.js:356-426`; others same-origin
  redirects ≤2 hops, `image/*` only, 16 MiB, 8 s, NSW browser UA, `media.js:444-560`) →
  **Google Street View Static** `https://maps.googleapis.com/maps/api/streetview?size=960x540&location=…&key=<server key>` (8 s,
  uncapped) using **client-supplied lat/lon/heading/fov/pitch when the id is unknown** (`cctv.js:91-130`, `:468-531`) →
  synthetic SVG placeholder with XML-escaped label/city (`media.js:27-77`). All `Cache-Control: no-store`, `X-CCTV-Source`.
  **No same-site gate, no rate limit.**
- Catch: `500 {"error":"CCTV proxy error"}` (`cctv.js:557-561`); unknown path 404.

### 2.19 Local ADS-B receivers

**#50 `GET|HEAD /api/local-receivers/aircraft`** — `legacy/server/providers/local-receivers.js:29-521`
- Feeds only from `LOCAL_RECEIVER_FEEDS` (`1090=url,978=url`, ≤8): http(s), no credentials/query/fragment,
  host loopback/RFC1918/`localhost`/`*.local`, path ends `aircraft.json` (`:53-127`; `legacy/src/data/tapAddress.js`).
  Names are DNS-resolved per read and **every address must be local**; the socket is pinned (`agent:false`) (`:142-268`).
- 2 s timeout incl. DNS, 2 MiB cap, redirects refused, 1 s shared snapshot cache (`:31-37`, `:277-343`, `:458-466`).
- Response: unconfigured `{configured:false,feeds:[],records:[]}`; else
  `{configured:true, generatedAt, feeds:[{band,label,status:"live"|"stale"|"unreachable"|"invalid",aircraft,ageMs}], records:[{…callsign,category,onGround,lat,lon,altitudeFt,groundSpeedKt,trackDeg,verticalRateFpm,lastPositionAt,lastMessageAt,messageCount,rssiDbfs,band,source}]}`
  (`:349-456`; `legacy/src/sources/adsbRecords.js:242-262`). Feed URLs never returned. Error 502; 405 otherwise.

### 2.20 OpenAI (realtime voice, HUD summary, debug log)

Mounted by `legacy/server/providers/openai.js:18-64`; **all five routes are wrapped by the same-site gate**
(`legacy/server/providers/common/same-site.js:38-43`).

**#51 `GET|POST /api/realtime/token?auth=api-key|oauth&tier=standard|mini`** — `legacy/server/providers/openai/realtime.js:130-363`
- Upstream: `POST https://api.openai.com/v1/realtime/client_secrets`, 30 s, `redirect:'error'`,
  `OpenAI-Safety-Identifier: gev-local-dev` (`:132`, `:307-318`).
- Bearer: `OPENAI_API_KEY`, or with `auth=oauth` the ChatGPT access token read from Codex's
  `~/.codex/auth.json` (loopback clients only) (`:183-227`; `legacy/server/providers/openai/codex-auth.js:43-109`).
  `GEV_PREFER_CODEX_OAUTH=true` forbids the API-key lane (503 `CODEX_OAUTH_REQUIRED`) (`realtime.js:158-178`; `codex-auth.js:25-35`).
- Rate limit `GEV_RATELIMIT_OPENAI_PER_MIN` default 30/min/IP (global ×20) → 429 `Retry-After: 5`
  (`legacy/server/providers/openai/rate-limit.js:13-50`; `realtime.js:181`).
- Session config built server-side: model by tier (`OPENAI_REALTIME_MODEL`/`_MINI`, defaults from
  `legacy/src/voice/voiceCost.js:49-61`), voice, reasoning effort, context tokens 1000–12000, retention 0.1–1,
  transcription model (or `off`), semantic VAD, instructions, **tools = 30 app actions + voice-surface catalog tools**
  (`realtime.js:229-305`; `legacy/server/providers/openai/constants.js:5-20`; `legacy/server/standalone/voiceTools.js:14-18`).
- Response: OpenAI JSON passthrough (contains the **ephemeral** client secret) with
  `X-GEV-Voice-Tier`, `X-GEV-Voice-Model`, `X-GEV-Voice-Auth`, `X-GEV-Voice-Transcribe-Model`,
  `X-GEV-Voice-Tier-Fallback` (`:319-349`); upstream failure → upstream status with generic error; network → 502.

**#52 `GET /api/realtime/oauth-status`** — loopback socket only → `{available, error?, code?, reauthRequired?, pending?, loginFailed?}` (`realtime.js:56-80`; `codex-auth.js:241-266`).

**#53 `POST /api/realtime/oauth-login`** — loopback + same-origin; spawns `codex login` (or `CODEX_BIN`) detached with an
allowlisted environment, one attempt at a time, 120 s timeout; 200/202/503 (`realtime.js:82-128`; `codex-auth.js:111-227`, `:230-360`).

**#54 `POST /api/realtime/debug-log`** — JSON ≤8 MiB, 120/min/IP (400 global), appended to
`<sourceRoot>/.gev-logs/realtime-conversations.jsonl`, rotated at 32 MiB (one `.1` generation); transcript/text/tool
argument/result fields replaced by `[omitted N chars]` unless `GEV_VOICE_LOG_CONTENT=1`; server timestamp `loggedAt`
appended; 204 / 400 / 429 / 500 (`legacy/server/providers/openai/debug-log.js:7-157`).

**#55 `POST /api/openai/hud-summary`** — keyless → `200 {configured:false, code, error:null, summary:null}`
(`legacy/src/hudSummaryResponse.js:16-27`); else rate-limited (shared OpenAI limiter), body JSON ≤64 KiB, `POST https://api.openai.com/v1/responses` with
`OPENAI_HUD_SUMMARY_MODEL` (default `gpt-5-nano`), `max_output_tokens:100`, **no timeout**; response
`{summary: ≤5 words|null, error}`; failures generic (`legacy/server/providers/openai/hud-summary.js:31-98`).

### 2.21 Key setup (Provider Settings) — dev server only

`legacy/server/standalone/key-setup.js:73-344`; `apply: command==='serve' && !isPreview` (`:245-247`).
- Gate (`admitKeySetupRequest`): no proxy headers; launcher sharing off; **loopback socket**; Host must be
  `localhost|127.0.0.1|::1`; POST needs exact same Origin and `application/json` (415)
  (`legacy/src/keySetupCore.mjs:239-338`, `:128-131`).
- **#56 `GET /api/setup/status`**: `{keys:[{id,title,unlocks,getUrl,envVars,tier,clientExposed,set,managed:"file"|"external"|null}], setCount, total, store:"env-file"|"pinokio-environment"}` — presence only, never values (`key-setup.js:163-177`; `keySetupCore.mjs:379-399`).
- **#57 `POST /api/setup/keys`** body `{ENV_VAR: "value"|null}` ≤8192 chars, ≤16 keys, known names only,
  values printable ASCII ≤512, no `#"'$\`` chars (`key-setup.js:256-281`; `keySetupCore.mjs:18-21`, `:410-479`);
  refuses externally-managed keys (409) (`key-setup.js:287-294`); atomically rewrites `.env` (or `pinokio/ENVIRONMENT`
  under Pinokio) via 0600 temp + fsync + rename, refuses symlinks, Windows ACL hardening
  (`:94-124`, `:182-237`; `legacy/server/standalone/key-setup-hardening.mjs:1-222`); sets `process.env`;
  responds `{ok, saved, status, restarting:true}` and **restarts the Vite server** 250 ms later (`:319-337`).

### 2.22 MCP over HTTP

**#58 `POST /mcp`** — `legacy/server/mcp/plugin.js:62-145` → `legacy/src/tools/mcp/http.js:13-39`.
See §6. Local-only gate (`plugin.js:26-59`): loopback socket, no proxy headers, sharing off, Host must be a
loopback name **on the port the socket reached**, Origin (if present) same host. Body ≤1 MiB, 30 s body timeout
(408/413), client disconnect aborts the tool call. Non-POST → 405 `Allow: POST`; wrong content type 415;
unsupported `MCP-Protocol-Version` 400; notifications → 202.

---

## 3. Shared infrastructure

| Concern | Implementation | Notes for Rust |
|---|---|---|
| Body read caps (response) | `readResponseTextCapped` / `readResponseJsonCapped` / `readResponseBytesCapped`: reject on declared Content-Length, then streaming running cap, abort-signal aware, error code `RESPONSE_TOO_LARGE` — `legacy/src/sources/httpBody.js:6-109`; older variant `readCappedResponseText` returns `{tooLarge,text}` — `legacy/server/providers/common/http.js:17-53`; wind/weather byte reader `legacy/src/sources/windBody.js:2-40` | One `read_capped(resp, max, cancel)` helper over `reqwest::Response::chunk()` |
| Request body caps | `readRequestBodyCapped` (throws `BODY_TOO_LARGE`) and `readRequestBody` (drains past cap so an error response can still be sent) — `legacy/server/providers/common/request.js:2-45`; key-setup and MCP have their own (`legacy/server/standalone/key-setup.js:262-270`, `legacy/server/mcp/plugin.js:147-179`) | `axum::extract::DefaultBodyLimit` per route + custom drain |
| Single-flight | `coalesceProxyRequest(map,key,create)` → `{promise, shared}` — `legacy/src/sources/httpBody.js:115-126`; bespoke variants with waiter-refcount cancellation in weather (`legacy/server/providers/weather.js:267-312`), wind (`legacy/server/providers/wind.js:38-210`), cyclones (`legacy/server/providers/cyclones.js:320-354`), mapillary (`legacy/server/providers/mapillary/tiles.js:444-493`) | `futures::future::Shared` + `Arc` waiter counter + `CancellationToken` |
| Rate limiting | Sliding-window timestamp log per key + global backstop, map capped at 2000 keys — `legacy/src/sources/rateLimit.js:1-31`; cost limiter env parsing (unset/unreadable → default, `0` → off, fraction → 1) — `legacy/server/providers/common/rate-limit.js:21-52`; key = socket remote address only (never X-Forwarded-For) — `:60-62` | Hand-roll to match semantics (governor's GCRA differs) |
| Same-site gate | `admitSameSiteRequest`: refuse any proxy header (`forwarded, via, x-forwarded-*, x-real-ip, cf-connecting-ip, cf-ray`), refuse `Sec-Fetch-Site` other than `same-origin`/`none`, refuse Origin `null` or ≠ request authority — `legacy/src/localRequestGate.mjs:28-154`; middleware wrapper `legacy/server/providers/common/same-site.js:15-43`. Applied to: 5 OpenAI routes, 2 Google routes, 2 Mapillary routes only | Tower layer; pure fn is directly portable |
| Host check | `legacy/build/allowedHosts.js:42-93` | Tower layer applied first |
| Query parsing | `requiredFiniteQueryNumber` (blank/absent → null), `clampInt` — `legacy/server/providers/common/query.js:1-12` | |
| Geo | haversine km — `legacy/server/providers/common/geo.js:10-18`; equirectangular metres (AIS) — `legacy/server/providers/vessels/ais-store.js:184-189`; Douglas–Peucker — `legacy/server/providers/overpass/geometry.js:9-101` | `geo` crate |
| Source root | repo root resolved from module path — `legacy/server/providers/common/source-root.js:5-8` (used by CCTV catalog, debug log, key setup) — but most disk caches use `process.cwd()` | Single configured data dir |
| SSRF controls | Fixed upstream hosts almost everywhere; client-chosen URL only in GBFS (allowlist + path) `legacy/src/data/gbfsSource.js:2-38`; redirects refused or same-origin-pinned: weather/cyclones/wind/perimeters/places/route/geocode/HLS (`redirect:'error'`), transit hop-by-hop (`legacy/src/data/transitProxy.js:154-170`), CCTV frames same-origin ≤2 hops (`legacy/server/providers/cctv/media.js:444-478`), Mapillary tile origin (`legacy/server/providers/mapillary/tiles.js:293-320`), InciWeb manual redirect (`legacy/server/providers/firePerimeters.js:104-129`); DNS-pinned public-only (radio, `legacy/server/providers/radio/transport.js:13-145`) and local-only (receivers, `legacy/server/providers/local-receivers.js:142-268`); MCP `apiBase` derived from validated loopback Host (`legacy/server/mcp/plugin.js:67-71`, `:107-115`) | `reqwest::redirect::Policy::none()` + custom; `ClientBuilder::resolve_to_addrs` or a hickory resolver for pinning |
| Timeouts | Per-provider constants (listed per route); missing on `/api/military` upstream (`legacy/server/providers/aircraft/adsb-lol.js:101-103`), HUD summary (`legacy/server/providers/openai/hud-summary.js:57-72`), Google Places (`legacy/server/providers/places/google.js:118-149`) | Default client timeout + explicit per-call |
| Logging hygiene | Error bodies sanitised, upstream URLs/keys never logged (e.g. `legacy/server/providers/firms.js:80-83`, `legacy/server/providers/cctv.js:431-446`, `legacy/server/providers/overpass/transport.js:186-189`) | `tracing` with redaction |

---

## 4. Background work, persistent connections, stores

### 4.1 Timers and long-lived tasks

| Task | Cadence | Evidence |
|---|---|---|
| AISStream watchdog tick (drives connect/recycle/backoff) | every 15 s, unref'd, plus on each `/api/vessels` request | `legacy/server/providers/vessels/ais-live.js:48`, `:333-343`, `:77` |
| AISStream WebSocket (one per key) | persistent; report stale after 120 s silence, recycle at 2.5×; backoff 5 s/15 s/60 s/300 s then 15 min; auth failure probes hourly; rate-limit honours Retry-After; frames >1 MB rejected | `ais-live.js:35-48`, `:252-289`; `legacy/src/data/aisWatchdog.js:36-47`, `:93-108`; `legacy/src/data/aisStreamAdapter.js:64-98`, `:151`, `:283-580` |
| adsbdb disk flush | 15 s | `legacy/server/providers/aircraft/enrichment.js:27-36` |
| Terrain disk flush | 15 s | `legacy/server/providers/terrain.js:95-106` |
| Transit history prune sweep | 60 s | `legacy/src/sources/transitHistoryStore.js:476-479` |
| HLS session polling | 2 s per active session; lease idle 15 s; stop after 3 failures | `legacy/server/providers/cctv/stream.js:4-15`, `:149-163`, `:171-250` |
| Mapillary disk sweep | on write, ≤ once per 15 min | `legacy/server/providers/mapillary/tiles.js:220-282` |
| Codex login watchdog | 120 s | `legacy/server/providers/openai/codex-auth.js:230-297` |
| Key-setup restart | 250 ms after save | `legacy/server/standalone/key-setup.js:330-337` |

There are **no warmers**: every other cache (OpenSky, CelesTrak, LL2, FIRMS, CCTV catalog, radio catalog,
weather, wind, cyclones …) fills lazily on the first request. Shutdown hooks: AIS dispose on `httpServer close`
(`ais-live.js:156-175`, `:357-368`), HLS shutdown (`legacy/server/providers/cctv.js:133-135`), transit close
(`legacy/server/providers/transit.js:16-21`), Codex login dispose (`legacy/server/providers/openai.js:52-63`).

### 4.2 In-memory stores (process lifetime)

AIS: vessels ≤50k, static map, per-MMSI Float32 ring buffers (64 samples), pending first fixes; pruned on
**every ingested message** with an O(n) scan (`legacy/server/providers/vessels/ais-store.js:2-18`, `:94`, `:231-258`).
Transit history: byte-accounted store, 15 min retention, ≤128 fixes, 16 MiB/feed, 64 MiB/process, 15k keys
(`legacy/src/sources/transitHistoryStore.js:4-12`). HLS segments ≤2×24 MiB. Mapillary ≤96 MiB. Weather tiles ≤16 MiB.
Wind ≤2 models×3 overlays×2 generations (~1.5 MB each). Unbounded: adsbdb maps, terrain points, CelesTrak groups (regex-limited).

### 4.3 On-disk state (`.gev-cache/` relative to **cwd**, except where noted)

`adsbdb.json`, `celestrak-<group>.json`, `launch-library-2-v2.3.json`, `firms.json`, `terrain-heights.json`,
`tomtom/budget.json` + `tomtom/flow-*.pbf` (unswept), `overpass/<sha1>.json` (unswept),
`military-installations/<sha1>.json` (unswept), `mapillary/tiles/**` (swept ≤1 GiB), `mcp-panel-key`
(module-relative, `legacy/server/mcp/panelKey.js:9-11`); `.gev-logs/realtime-conversations.jsonl` (sourceRoot,
`legacy/server/providers/openai/debug-log.js:79-80`); credentials `.env` / `pinokio/ENVIRONMENT`
(sourceRoot, `legacy/server/standalone/key-setup.js:97-101`). Paths: `enrichment.js:12`, `celestrak.js:25`,
`launch-library.js:25-29`, `firms.js:38-39`, `terrain.js:26-27`, `traffic.js:45-46`,
`overpass/constants.js:63`, `military-installations/constants.js:36-40`, `mapillary/constants.js:19-24`.

---

## 5. Secrets and configuration

### 5.1 Environment variables read server-side

| Variable | Secret | Read at | Browser? | Settable in Provider Settings |
|---|---|---|---|---|
| `GOOGLE_MAPS_API_KEY` | key (client-exposed) | `legacy/server/standalone/vite.config.js:24` → define; server fallback `legacy/scripts/google-server-key.mjs:4` | **YES** (`legacy/build/vite.js:108`) | yes (`legacy/src/keySetupCore.mjs:36-44`) |
| `GOOGLE_MAPS_SERVER_API_KEY` | key | `legacy/scripts/google-server-key.mjs:4` | no (test-asserted, `legacy/src/googleServerKey.test.mjs`) | hidden entry (`keySetupCore.mjs:45-54`) |
| `CESIUM_ION_TOKEN` | token (client-exposed) | `vite.config.js:25` | **YES** (`build/vite.js:109`) | yes |
| `MAPILLARY_CLIENT_TOKEN` | token (client-exposed) | `vite.config.js:26`; `legacy/server/providers/mapillary/constants.js:70-72` | **YES** (`build/vite.js:111-113`) | yes |
| `OPENAI_API_KEY` | key | `legacy/server/providers/openai/realtime.js:134`; `hud-summary.js:39` | no (ephemeral secret only) | yes |
| `AISSTREAM_API_KEY` | key | `legacy/server/providers/vessels/ais-live.js:372` | no | yes |
| `FIRMS_MAP_KEY` | key | `legacy/server/providers/firms.js:51` | no | yes |
| `TOMTOM_API_KEY` | key | `legacy/server/providers/traffic.js:144` | no | yes |
| `OPENSKY_CLIENT_ID`/`_SECRET` | OAuth client | `legacy/server/providers/aircraft/opensky.js:128-129` | no | yes |
| `OPENSKY_USERNAME`/`_PASSWORD` | basic creds | `opensky.js:469-470` | no | no |
| `LL2_API_TOKEN` | token | `legacy/server/providers/space/launch-library.js:12` | no | yes |
| `TFL_APP_KEY` | key | `legacy/server/providers/cctv/sources.js:328` | no | no |
| `OVERPASS_UPSTREAMS` | may embed Basic creds | `legacy/server/providers/overpass/constants.js:34`; auth `legacy/server/providers/overpass/transport.js:131-140` | no | no |
| `CODEX_AUTH_JSON`, `CODEX_HOME`, `CODEX_BIN`, `GEV_PREFER_CODEX_OAUTH` | path to ChatGPT token / exec | `legacy/server/providers/openai/codex-auth.js:15-35`, `:116` | no | no |
| `OPENSKY_AUTH_MODE` | – | `opensky.js:389-391` | – | – |
| `OPENAI_REALTIME_MODEL`, `_MODEL_MINI`, `_VOICE`, `_REASONING_EFFORT`, `_CONTEXT_TOKENS`, `_CONTEXT_RETENTION`, `_TRANSCRIBE_MODEL`, `OPENAI_HUD_SUMMARY_MODEL` | – | `realtime.js:237-271`; `hud-summary.js:64-66` | – | – |
| `GEV_RATELIMIT_OPENAI_PER_MIN`, `GEV_RATELIMIT_GOOGLE_PER_MIN` | – | `legacy/server/providers/openai/rate-limit.js:26`; `legacy/server/providers/places/google.js:32` | – | – |
| `GEV_VOICE_LOG_CONTENT` | – | `legacy/server/providers/openai/debug-log.js:77` | – | – |
| `AISSTREAM_BOUNDING_BOXES`, `_MESSAGE_TYPES`, `_SILENCE_TIMEOUT_MS`, `_URL` | – | `ais-live.js:222-241`, `:373-380` | – | – |
| `TOMTOM_DAILY_TILE_BUDGET` | – | `traffic.js:62` | – | – |
| `LOCAL_RECEIVER_FEEDS` | – | `legacy/server/providers/local-receivers.js:30`, `:406` | – | – |
| `CCTV_*` (≈40 vars: `_SOURCES_FILE/_JSON`, `_MAX_SOURCES`, `_FORCE_AUSTIN`, `_PREFER_AUSTIN`, `_GROUND_HEIGHTS_FILE`, per-pack `_ENABLED`, `_MAX_SOURCES`, `_DISTRICTS`, `_URL`, `_ROWS_URL`, `_VEGVESEN_VIDEO`) | – | `legacy/server/providers/cctv/catalog.js:25`, `:34-101`, `:108`, `:133`, `:194-197`, `:240`; `legacy/server/providers/cctv/sources.js` (lines listed in §2.18); `legacy/server/providers/cctv/groundHeights.js:22` | – | – |
| `HOST`, `PORT`, `GEV_ALLOWED_HOSTS`, `GEV_EMBED_FRAME_ANCESTORS` | – | `legacy/server/standalone/vite.config.js:27-29`; `legacy/build/embed-framing.js:39` | – | – |
| `GEV_LAUNCHER`, `GEV_KEY_SETUP_EXTERNAL_KEYS`, `PINOKIO_SHARE_CLOUDFLARE/_LOCAL/_VAR` | – | `legacy/server/standalone/key-setup.js:24`, `:49-54`; `legacy/src/keySetupCore.mjs:208-217` | – | – |
| `VITE_AIS_LIVE_API_URL`, `VITE_AIS_LIVE_MAX_ROWS`, `VITE_AIS_LIVE_LABEL_MAX_ROWS` | – | client via Vite `import.meta.env` | **YES** (any `VITE_*`) | – |

### 5.2 Where secrets live and how key setup writes them

Sources, in precedence: shell env/Keychain (launchers export before Vite; `legacy/scripts/dev-fresh.sh:94-245`),
then `.env*` via `loadEnv` (`legacy/server/standalone/vite.config.js:14-17`), or under Pinokio its
`pinokio/ENVIRONMENT` applied before Vite (`legacy/scripts/pinokio-start.mjs:29-38`; `legacy/pinokio/_ENVIRONMENT:1-32`).
Codex's own `~/.codex/auth.json` is read (never written) for ChatGPT OAuth (`legacy/server/providers/openai/codex-auth.js:11-22`, `:43-109`).
Key setup writes only the store this launch owns, classifies boot-time values as external/read-only, writes atomically
with 0600/ACL hardening, then restarts Vite so the client defines are re-injected (`legacy/server/standalone/key-setup.js:17-54`, `:94-101`, `:148-162`, `:182-237`, `:319-337`).

### 5.3 What reaches the browser (flagged)

1. **`GOOGLE_MAPS_API_KEY`, `CESIUM_ION_TOKEN`, `MAPILLARY_CLIENT_TOKEN`** are compiled into the JS bundle by `define`
   (`legacy/build/vite.js:107-114`) — by design, documented (`legacy/.env.example:16-19`; `legacy/SECURITY.md:26-34`).
   The MCP panel build embeds the Google and Cesium keys into a page that runs on third-party MCP clients' sites
   (`legacy/SECURITY.md:104`).
2. **Server-side Google calls fall back to the browser-exposed `GOOGLE_MAPS_API_KEY`** when
   `GOOGLE_MAPS_SERVER_API_KEY` is unset (`legacy/scripts/google-server-key.mjs:2-5`), so the public key must be
   authorised for Places + Street View Static — and §2.18 #49 lets any page spend Street View quota.
3. **Any `VITE_*` variable** in `.env` is exposed by Vite's default `envPrefix` (no override in
   `legacy/build/vite.js:50-116`); only non-secret `VITE_AIS_LIVE_*` exist today (`legacy/.env.example:144-150`) and the
   file warns not to prefix secrets with `VITE_` (`legacy/.env.example:61`, `:151`).
4. The Realtime **ephemeral** client secret is returned to the browser by design (`legacy/server/providers/openai/realtime.js:345-349`).
5. No server-only secret was found in any JSON response; `/api/setup/status` returns presence only
   (`legacy/src/keySetupCore.mjs:379-399`), `/api/mapillary/status` returns a boolean
   (`legacy/server/providers/mapillary/routes.js:14-18`), CCTV/local-receiver payloads omit upstream URLs
   (`legacy/server/providers/cctv.js:145-171`; `legacy/server/providers/local-receivers.js:90-95`).

---

## 6. MCP server and OpenAI realtime/voice backend

### 6.1 MCP

- **Transports.** stdio: newline-delimited JSON-RPC, cancellation via `notifications/cancelled`, logs to stderr
  without arguments (`legacy/server/mcp/stdio.js:22-113`). HTTP: stateless "Streamable HTTP" subset — one JSON-RPC
  message per POST answered with JSON (no SSE, no sessions) (`legacy/src/tools/mcp/http.js:1-78`), mounted at `/mcp`
  on dev and preview (`legacy/server/mcp/plugin.js:62-145`).
- **Protocol.** Versions `2025-11-25`, `2025-06-18`, `2025-03-26`; methods `initialize`, `ping`, `tools/list`,
  `tools/call`, `resources/list`, `resources/read` (`legacy/src/tools/mcp/protocol.js:11-15`, `:72-130`).
  Results: summary text + JSON text + `structuredContent` + optional images.
- **Composition.** `createLocalMcpServer` composes `coreTools` with services pointed at a running app's `/api`
  (`legacy/server/mcp/server.js:27-53`; `legacy/server/mcp/services.js:20-64`; `legacy/src/tools/services.js:45-91`).
  Some tool sources fetch third parties directly from the MCP process (USGS earthquakes
  `legacy/src/layers/earthquakes/source.js:3`; NASA CMR `legacy/src/layers/recentImagery/catalog.js:16`;
  Worldview snapshots `legacy/src/layers/recentImagery/model.js:18`; ALPR tiles `legacy/src/layers/alpr/source.js:74-75`).
  Bundled data (`src/data/local_data/`) is read from disk (`services.js:10-55`).
- **Tools.** 40 defined (`legacy/docs/TOOLS.md:259-302`); MCP surface hides 10
  (`search_places`, `places_nearby`, `plan_route`, `get_weather`, `get_wind`, `get_regional_brief`,
  `find_radio_stations`, `get_bike_share`, `get_transit_vehicles`, `get_hud_caption`) → **30 MCP tools**
  (`legacy/src/tools/surfaces.js:16-49`). Includes `show_in_gods_eye_view` and the app-only `panel_request`.
- **Resource.** `ui://gods-eye-view/globe` MCP-Apps panel (`legacy/src/tools/globePanel.js:9`, `:124`); panel
  key per process or shared file `.gev-cache/mcp-panel-key` (`legacy/server/mcp/server.js:27-52`;
  `legacy/server/mcp/panelKey.js:76-117`). `panel_request` proxies arbitrary app paths (GET/HEAD/POST) in 512 KiB parts,
  ≤64 MiB, 6 concurrent / 256 queued, refusing `/api/(setup|realtime|openai)`, `/mcp`, `/@…`, `/__…`
  (`legacy/src/tools/queries/panelRequest.js:21-46`).
- **Security.** Local transport safety only, not auth (`legacy/SECURITY.md:98-105`; `legacy/docs/TOOLS.md:230-257`).

### 6.2 OpenAI realtime / voice

- The browser asks `/api/realtime/token` for an ephemeral client secret, then negotiates WebRTC SDP directly with
  `https://api.openai.com/v1/realtime/calls` (`legacy/src/voice/realtimeBackend.js:5-6`, `:72-100`). The server never
  touches audio.
- Server responsibilities: choose auth lane (API key vs Codex ChatGPT token), model tier, VAD/transcription/truncation
  settings, the system instructions (`legacy/server/providers/openai/instructions.js:1-70`), and the tool list:
  **30 app actions** (`legacy/server/providers/openai/toolDescriptions.js:6-623`, schemas from
  `legacy/src/voice/actionSchemas.js` via `legacy/server/providers/openai/tools.js:1-16`) plus the
  voice-surface catalog queries (`legacy/server/standalone/voiceTools.js:14-18`; 15 tools hidden from voice,
  `legacy/src/tools/surfaces.js:16-37`). Tool calls execute in the browser, not on the server (`legacy/docs/TOOLS.md:206-228`).
- Auxiliary: HUD summary via `/v1/responses` (§2.20 #55); diagnostics sink (#54). Client-side redaction of keys/bearers
  happens before posting (`legacy/src/voice/realtimeDiagnostics.js:54-65`); the server only strips speech/tool content.

---

## 7. Tests covering server code

The runner discovers `src/**/*.test.mjs` only (`legacy/scripts/run-unit-tests.mjs:25-39`);
`legacy/server/providers/transitHistory.test.mjs` runs because `legacy/src/tooling/transitProvider.test.mjs` imports it.
All are `node:test` suites with injected `fetchImpl`/fake middlewares — no network.

| Test file | Covers | Asserts (summary) |
|---|---|---|
| `legacy/src/tooling/liveProviders.test.mjs` | opensky, adsb-lol, tracks, ais-live | token sharing, adaptive cache, regional fallback, retries/timeouts, stale-on-429, Retry-After bounds, AIS ingest/track/dispose, oversized track → cached 502 |
| `legacy/src/tooling/environmentProviders.test.mjs` | terrain, traffic, firms, gbfs, local.js | composition mounts each provider once; terrain chunking/migration; TomTom keyless/budget/UTC rollover; FIRMS partial failure; GBFS guards |
| `legacy/src/tooling/localServices.test.mjs` | openai/*, key-setup, overpass, regional, military-installations | realtime config/tools/OAuth paths (loopback, pin, expiry, login lifecycle), debug-log omission/rotation/limits, key setup dev-only and root-scoped |
| `legacy/src/tooling/mediaProviders.test.mjs`, `legacy/src/data/cctvMediaRange.test.mjs`, `cctvProxy.test.mjs`, `cctvHlsStream.test.mjs`, `cctvCatalogCap.test.mjs`, `cctv{Calgary,DriveBc,Estonia,Fintraffic,GroundHeights,Nsw,Txdot,Vegvesen,Warendorf,DisplayCode}.test.mjs` | cctv/* | per-agency parsing & origin pins, round-robin cap, Range sanitising, header/idle deadlines, HLS limits/leases, sanitised health messages |
| `legacy/src/data/weatherProxy.test.mjs` (33 tests) | weather.js | capability parsing, tile math, fixed upstream params, coalescing/cancellation, stale policy |
| `legacy/src/data/windProxy.test.mjs`, `windGfs`, `windIfs`, `windGrid`, `windDecode` | wind/* | manifest/grid contract (Float32 U then V), TTL, backoff, overlays; idx/index parsing; resampling. **Decode test uses a mock eccodes; no real GRIB fixture** |
| `legacy/src/data/cyclonesProxy.test.mjs` | cyclones.js | strict NHC/GIS validation, TTL/cooldown/last-good, cancellation, caps |
| `legacy/src/data/firePerimetersProxy.test.mjs`, `legacy/src/layers/perimeters/source.test.mjs` | firePerimeters.js | paging, stale, caps, InciWeb redirects, per-client limit |
| `legacy/src/data/firmsProxy.test.mjs` | firms.js | ordering, partial failure, empty vs failed |
| `legacy/src/data/gbfsProxy.test.mjs`, `legacy/src/layers/bikeshare/proxyRoute.test.mjs` | gbfs.js | redirects refused, byte caps, timeouts |
| `legacy/src/data/radioProxy.test.mjs` | radio/* | normalisation, health policy, generations, method/route validation |
| `legacy/src/data/overpassProxy.test.mjs`, `legacy/src/overpassProxy.test.mjs`, `legacy/src/overpassOffload.test.mjs` | overpass/*, military-installations, regional | preflight order, simplification, refusals not cached, Retry-After, zero-egress default, credential hiding |
| `legacy/src/data/installationProxy.test.mjs` | military-installations/* | snapping, keys, disk TTL, stale |
| `legacy/src/data/regionalProxy.test.mjs`, `legacy/src/tooling/nominatimSearchRoute.test.mjs`, `legacy/src/search/geospatial.test.mjs` | regional/*, routes | coordinate validation, coalescing, Nominatim pacing/queue/abandon |
| `legacy/src/data/localReceiversProxy.test.mjs` | local-receivers.js | feed parsing, DNS pinning, null-body statuses, timeouts |
| `legacy/src/tooling/mapillaryProvider.test.mjs` (40 tests) | mapillary/* | address validation, key-rejected/429 holds, memory/disk TTL, sweep, layer stripping |
| `legacy/src/tooling/transitProvider.test.mjs` + `legacy/server/providers/transitHistory.test.mjs` | transit | 304 handling, differential refusal, cooldown, redirect pinning, history admission |
| `legacy/src/tooling/placeProviders.test.mjs`, `legacy/src/googlePlacesCoordinates.test.mjs`, `googlePlacesKeyless.test.mjs`, `googleServerKey.test.mjs` | places/* | projections, OSRM gate/queue/429, redirect refusal, same-site gate, key selection, browser defines exclude server key |
| `legacy/src/costEndpointThrottle.test.mjs`, `legacy/src/hudSummaryResponse.test.mjs` | rate limits, HUD | default caps match Pinokio, 429 contracts |
| `legacy/src/tooling/spaceProviders.test.mjs`, `legacy/src/proxyErrorResponses.test.mjs` | celestrak, launches, terrain, adsbdb | caches, sanitised errors |
| `legacy/src/codexOauthRealtime.test.mjs`, `legacy/src/keySetupCore.test.mjs`, `legacy/src/keySetupHardening.test.mjs`, `legacy/src/localRequestGate.test.mjs` | auth & gates | Codex token parsing, key-setup admission/validation/dotenv upsert, ACL hardening, same-site gate matrix |
| `legacy/src/tooling/allowedHosts.test.mjs`, `viteBuild.test.mjs`, `previewServing.test.mjs`, `panelBuild.test.mjs`, `applicationHtml.test.mjs` | build/* | host check first, CSP/embed framing, defines, dev vs preview hooks, `/api` 404 |
| `legacy/src/tools/localRoute.test.mjs`, `localServer.test.mjs`, `mcpPanelKey.test.mjs` | mcp/* | local-only gate, disconnect cancel, body timeout, stdio protocol, panel key sharing |
| Pure-module tests: `legacy/src/data/aisWatchdog.test.mjs`, `aisStreamAdapter.test.mjs`, `aisStreamSentinels.test.mjs`, `transitProxy.test.mjs`, `gtfsRealtime.test.mjs`, `terrainHeightsProxy.test.mjs`, `tomtomTiles.test.mjs`, `adsbLolFallback.test.mjs`, `naturalEarthRegions.test.mjs`, `tapAddress.test.mjs`, `legacy/src/nominatimGeocode.test.mjs`, `legacy/src/sources/adsbRecords.test.mjs` | src modules the server imports | state machines, decoders and validators |

**Reusable fixtures for Rust contract tests:** `legacy/src/data/fixtures/tomtom-flow-austin-12-935-1686.pbf`
(real TomTom MVT), `adsb-austin-dump1090-aircraft.json` (local receiver `aircraft.json`), `firms-viirs-noaa20-sample.csv` +
`firms-csv-cases.json` (FIRMS CSV), `ofm-*.pbf` / `osm-alpr-*.pbf` (MVT) (`legacy/src/data/fixtures/README.md`);
`legacy/config/cctv_sources.{tallinn,warendorf,shinjuku}.json` (CCTV packs);
`legacy/src/data/local_data/natural_earth/*.json` (regional place); inline payloads inside the tests above
(WMS capabilities XML, NHC JSON/GeoJSON, GFS `.idx`/IFS `.index` text, GTFS-RT built with `PbfWriter`, per-agency CCTV
catalogs incl. DATEX2 XML, Radio Browser rows, WFIGS GeoJSON, InciWeb HTML). Recommended: extract these into
`fixtures/` JSON/XML files and add **real** GFS/IFS GRIB2 messages and an AISStream message capture, which do not exist upstream.

---

## 8. Rust migration mapping

Workspace assumption: `lavender-domain` (types + pure validation/parsing), `lavender-core` (providers, cache,
scheduling, geo, decoders), `lavender-server` (Axum adapter), later `lavender-tauri`.

| Upstream module | Rust home | Suggested crates | Size | Risks / notes |
|---|---|---|---|---|
| Hosting: `server/standalone/vite.config.js`, `build/vite.js`, `build/allowedHosts.js`, `build/embed-framing.js`, `api-not-found.js` | `lavender-server` (router, tower layers, static `dist/`) | axum, tower-http (`ServeDir`, `SetResponseHeader`), tower | M | Reproduce host check before routing, CSP/XFO/nosniff, embed-mode CSP rewrite, `/api/*` JSON 404, SPA fallback. Decide on Connect quirks (case-insensitive prefix, `.` boundary, subpath leniency). Client keys must become **runtime config** (no Vite `define`). |
| `common/http.js`, `request.js`, `query.js`, `rate-limit.js`, `same-site.js`, `geo.js` + `src/sources/{httpBody,rateLimit}.js`, `src/localRequestGate.mjs` | gates/limiters in `lavender-domain` (pure) + `lavender-server` (layers); capped readers in `lavender-core::http` | reqwest (rustls, stream, gzip), bytes, geo | S | Keep exact limiter semantics (sliding log, global backstop, 2000-key cap, env parsing rules). |
| Single-flight + waiter-refcount cancellation (weather, wind, cyclones, mapillary, generic coalesce) | `lavender-core::flight` | futures `Shared`, tokio-util `CancellationToken`, dashmap | M | Cancel-when-last-waiter-leaves needs Axum disconnect detection (drop guard on handler future). |
| `aircraft/opensky.js`, `tracks.js`, `adsb-lol.js`, `enrichment.js`, `src/data/adsbLolFallback.js` | `lavender-core::providers::aircraft` | reqwest, serde_json, oauth2 (or manual) | M | Module-global state shared with tracks; credit governor; adsbdb persistence (unbounded today — cap it). |
| `vessels/ais-live.js`, `ais-store.js`, `src/data/{aisStreamAdapter,aisWatchdog}.js` | `lavender-core::providers::ais` (task + store) | tokio-tungstenite (rustls), tokio::time, parking_lot | L | One connection per key, abort-not-close semantics, failure classification (401/403/429 via upgrade response), monotonic clock, O(n) prune per message must be replaced (time-bucketed eviction). |
| `space/celestrak.js`, `launch-library.js` | `providers::space` | reqwest, serde | S | Disk cache format/location. |
| `weather.js` (nowCOAST WMS) | `providers::weather` | quick-xml (no entity expansion), png header check | M | Strict capability validation; shared op queue (8/120/96). |
| `wind.js`, `wind/{gfs,ifs,catalog,grid,decode}.js` | `providers::wind` + `lavender-core::grib` | `grib` crate (pure Rust; check templates 5.0/5.2/5.3/5.42), or `eccodes` FFI fallback; `ndarray` optional | XL | GRIB2 decode parity (GFS complex packing + spatial differencing; ECMWF open data packing may be CCSDS/AEC). No real GRIB fixture upstream. Float32 endianness of `/grid/*.bin` must be pinned (LE). |
| `cyclones.js` | `providers::cyclones` | serde_json, geojson | M | Strict validation rules are the contract. |
| `firms.js`, `src/data/firmsCsv.js` | `providers::firms` | csv | S | Add body caps (world VIIRS ≈131k rows). |
| `firePerimeters.js` | `providers::perimeters` | geojson, regex/scraper for InciWeb HTML | M | HTML scraping brittleness; manual redirect pinning. |
| `gbfs.js`, `src/data/gbfsSource.js` | `providers::gbfs` | reqwest | S | Only client-URL proxy; keep allowlist + no-redirect. |
| `traffic.js`, `src/data/tomtomTiles.js` | `providers::traffic` | reqwest, chrono (UTC day) | S | Persistent budget counter; add disk sweep. |
| `transit.js` + `src/sources/transitService.js`, `transitHistoryStore.js`, `src/data/{transitProxy,transitFeeds,gtfsRealtime}.js` | `providers::transit` (+ registry in domain) | prost + gtfs-realtime proto, reqwest | L | Hop-by-hop redirect validation, conditional requests, byte-accounted history store with process-wide cap, decoder string/entity caps. |
| `terrain.js`, `src/data/terrainHeightsProxy.js` | `providers::terrain` | reqwest, rand | S | Add lat/lon range checks and a memory cap; keep request-order reconstruction; 500-vs-400 quirk. |
| `places/google.js`, `google-key.js`, `places/routes.js`, `src/data/placeProviderPayloads.js`, `routeSteps.js` | `providers::places`, `providers::routing` | reqwest | M | Global 1 req/s OSRM gate with bounded queue; route errors are HTTP 200 `{ok:false}`. |
| `regional/*`, `src/data/naturalEarthRegions.js`, `src/nominatimGeocode.js`, `regionalModel.js` | `providers::regional` | geo + rstar (point-in-polygon), quick-xml/rss, reqwest | M | Global Nominatim pacer; Natural Earth data (~7 MB JSON) loaded at startup. |
| `mapillary/*` | `providers::mapillary` | reqwest, prost/quick-protobuf (layer strip), tempfile | M | Token in query string must never leave the tile origin; 1 GiB disk cache sweep. |
| `overpass.js`, `overpass/*`, `military-installations*` | `providers::overpass` (sanitizer in domain) | regex, sha1, geo (simplify) | L | QL sanitizer is a security boundary — port with its test corpus verbatim; failover/cooldown; disk caches unswept today. |
| `radio.js`, `radio/*`, `src/sources/radioBrowser.js` | `providers::radio` | hickory-resolver, reqwest `resolve_to_addrs`, ipnet | M | Per-request DNS pinning to validated public IPs. |
| `cctv.js`, `cctv/{catalog,sources,normalize,cap,constants,groundHeights}.js` | `providers::cctv::catalog` | serde, quick-xml (DATEX2), base64, csv | L | 14 agency loaders (~1.9k lines of parsing), env kill switches, round-robin cap. |
| `cctv/{media,range,stream}.js` | `providers::cctv::media` + `lavender-server` streaming bodies | axum `Body::from_stream`, reqwest bytes_stream, m3u8-rs | XL | Unbounded live MJPEG/MP4 pass-through with idle deadline and disconnect propagation; HLS re-packager with leases/sessions/segment memory; Street View fallback (gate it). |
| `local-receivers.js`, `src/data/tapAddress.js`, `src/sources/adsbRecords.js` | `providers::local_receivers` | hickory-resolver, hyper client with custom connector | M | DNS pin to RFC1918; Tauri desktop is the natural home. |
| `openai.js`, `openai/{realtime,hud-summary,debug-log,rate-limit,constants,instructions,tools,toolDescriptions}.js` | `providers::openai` + `lavender-server` routes | reqwest, serde_json; schemas as JSON assets | L | Tool schemas/instructions are shared with the browser action runner (`src/voice/actionSchemas.js`) — generate one JSON artifact consumed by both. |
| `openai/codex-auth.js` | `providers::openai::codex` | tokio::process, base64, dirs | M | Spawns external `codex login`; reads a third-party credential file; terms risk (see §9). Consider dropping. |
| `standalone/key-setup.js`, `key-setup-hardening.mjs`, `src/keySetupCore.mjs` | `lavender-core::secrets` + server routes; in Tauri → OS keychain | keyring, tempfile, windows-acl / windows crate | L | Today writes `.env` and restarts the HTTP server to re-inject client keys; Rust needs live reload of config and a runtime `/config` for client-exposed keys. |
| `mcp/{plugin,server,services,stdio,panelKey}.js`, `src/tools/**` | `lavender-mcp` crate (stdio bin + Axum route) | rmcp (or hand-rolled JSON-RPC), schemars | XL | The 40 tools live in JS that is shared with the browser (`src/tools/queries/**`, `src/layers/**` sources); porting means re-implementing ~19 query modules or keeping a JS runtime. `panel_request` is a generic app-path proxy. |
| `build/panel.js`, `build/application-html.js` | build tooling (stays JS/Vite for the frontend) | – | S | Not a runtime concern for Rust; `/panel/*` static serving in dev only. |

---

## 9. Surprises / risks

1. **No production server exists.** `/api` lives only inside Vite dev/preview; Pinokio "production" is a Vite *dev* server (`legacy/scripts/pinokio-start.mjs:41-50`). Lavender's Axum server is the first real backend; every quirk of Connect routing (prefix, case-insensitive, `.` boundary, subpath leniency, any-method routes) is implicit contract (§1.6).
2. **Client keys are build-time constants.** Vite `define` bakes Google/Cesium/Mapillary keys into the bundle (`legacy/build/vite.js:107-114`), which is why key setup restarts the server (`legacy/server/standalone/key-setup.js:314-337`). Rust must serve these at runtime.
3. **Ungated cost endpoint:** `/api/cctv/frame/:unknownId?lat&lon` calls Google Street View Static with the server key, with no same-site gate and no rate limit (`legacy/server/providers/cctv.js:91-130`, `:468-531`); a cross-site `<img>` to `localhost` passes the host check. Similar quota-draining exposure for TomTom tiles and OpenSky credits (ungated, though cached/budgeted).
4. **GRIB2 via eccodes WASM** (`legacy/server/providers/wind/decode.js:4`) with no real GRIB fixture in the test suite (`legacy/src/data/windDecode.test.mjs` uses a mock). Highest technical risk.
5. **ChatGPT OAuth borrows Codex credentials** from `~/.codex/auth.json` and spawns `codex login` (`legacy/server/providers/openai/codex-auth.js:11-227`); upstream itself warns this is undocumented by OpenAI and may breach terms (`legacy/SECURITY.md:78`).
6. **AIS store prunes all 50k vessels on every message** (`legacy/server/providers/vessels/ais-store.js:94`, `:231-258`) — O(n) per frame on a firehose; do not port literally.
7. **Unbounded growth:** adsbdb maps + disk file (`legacy/server/providers/aircraft/enrichment.js:13`, `:89-96`), terrain point cache (`legacy/server/providers/terrain.js:39`), unswept disk caches for TomTom tiles, Overpass and installations (§4.3).
8. **Uncapped upstream reads / missing timeouts** despite the documented "response-size caps and timeouts" (`legacy/SECURITY.md:72`): OpenSky global snapshot (`legacy/server/providers/aircraft/opensky.js:48`), adsb.lol `/v2/mil` (no timeout, `legacy/server/providers/aircraft/adsb-lol.js:101-105`), FIRMS (`legacy/server/providers/firms.js:86-88`), TomTom (`legacy/server/providers/traffic.js:150`), CelesTrak (`legacy/server/providers/space/celestrak.js:63`), Street View (`legacy/server/providers/cctv.js:124`), Google Places / HUD summary (no timeouts, `legacy/server/providers/places/google.js:118`, `legacy/server/providers/openai/hud-summary.js:57`).
9. **CCTV media relay follows redirects** from registered (and operator-supplied file/env) URLs with no origin pin (`legacy/server/providers/cctv/media.js:343`), unlike the frame path's same-host rule (`media.js:444-478`).
10. **Doc/code drift:** SECURITY.md says the server's debug log strips keys and bearer tokens (`legacy/SECURITY.md:75`); the server only omits speech/tool content (`legacy/server/providers/openai/debug-log.js:33-73`) — key redaction is client-side (`legacy/src/voice/realtimeDiagnostics.js:54-56`). `.env.example` documents `OPENSKY_CREDENTIALS_FILE` (`legacy/.env.example:117-118`) which only launcher scripts read (`legacy/scripts/dev-fresh.sh:153`), while the server's `OPENSKY_USERNAME/PASSWORD` are undocumented there.
11. **Status-code oddities that clients may depend on:** `/api/route` validation/no-route → HTTP 200 `{ok:false}` (`legacy/server/providers/places/routes.js:230-233`); terrain "too many points" → 500 (`legacy/server/providers/terrain.js:177-182`); `/api/vessels` keyless → 503 with a normal body (`legacy/server/providers/vessels/ais-live.js:121`); cyclones/weather manifests return 200 `unavailable:true` on upstream failure.
12. **Disk caches resolve against `process.cwd()`**, not the app root (e.g. `legacy/server/providers/firms.js:38`), while logs/credentials use the source root — running from another directory silently forks the cache.
13. **Overpass is off by default** (`OVERPASS_UPSTREAMS=` empty, `legacy/.env.example:222-223`; `legacy/server/providers/overpass/constants.js:32-38`), so features/installations/admin outlines degrade unless an operator configures a private instance; the QL sanitizer (`legacy/server/providers/overpass/query.js:36-149`) is regex-based and security-relevant.
14. **MCP `panel_request` is a general proxy** to any app path except a short denylist (`legacy/src/tools/queries/panelRequest.js:31-35`); any local MCP client can reach every data route (and spend quotas) through it (`legacy/SECURITY.md:103`).
15. **Third-party data scraping:** InciWeb HTML regex scraping (`legacy/server/providers/firePerimeters.js:134-147`), undocumented adsb.lol trace files (`legacy/server/providers/aircraft/tracks.js:10-13`), NSW webcams require a spoofed browser User-Agent (`legacy/server/providers/cctv/constants.js:210-216`) — brittle and terms-sensitive.
