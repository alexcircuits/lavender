# Lavender — security specification

- Status: v1.1, 2026-10-12 (hardening pass: see the PROGRESS log of that date)
- Inputs: `docs/audit/upstream-backend.md` §3, §5, §9; `docs/audit/upstream-layers.md` §6; ADR 0006, 0008

## 1. Threat model (summary)

| Asset | Threat | Control |
|---|---|---|
| Provider keys and quotas | Leakage via bundle; quota draining by third-party pages (GEV's Street View and TomTom routes were ungated) | Server-held keys (ADR 0006); no browser key injection; host check + same-origin enforcement on `/api`; per-IP rate limits; per-provider budgets |
| Server | SSRF through user-supplied URLs; DNS rebinding against a local server | No URL parameters reach the fetcher; host allowlists; DNS resolution checks for public-only providers; Host-header allowlist before routing |
| Server availability | Oversized upstream bodies, slow upstreams, WebSocket floods, one client occupying shared capacity | Body caps, connect/total timeouts, bounded caches, connection caps (total and per client), message and frame size limits, inbound message rate per connection, subscription caps per connection, region budgets per client, idle and send timeouts, bounded feed-hub slots with idle eviction |
| Users | XSS through provider strings (names, place labels, HTML in feeds) | React text rendering only; no `dangerouslySetInnerHTML` (lint); strict CSP without `unsafe-eval`; provider strings length-capped and control-char-stripped in the core |
| Desktop | Over-privileged IPC | Least-privilege capabilities, no fs/shell/http/sql plugins in the WebView, typed commands only |
| Privacy | Viewport leakage to third parties; sensitive layers | Only tile hosts see viewports; CCTV and ALPR are off by default with notices; no telemetry |

## 2. Controls by layer

### Core (`lavender-core::fetch`)
- HTTPS only. Per-provider host allowlist. Redirects followed only to
  allowlisted hosts (max 3).
- Timeouts: connect 5 s, total 20 s by default (per-provider overrides).
- Body caps: each provider's own cap is authoritative (USGS 32 MiB, since
  `all_month` can exceed 16 MiB; CelesTrak 20 MiB per group; adsb.lol 12 MiB).
  `fetch.max_body_bytes` (16 MiB) applies to requests without one. Every cap is
  bounded by a hard 64 MiB ceiling, which the configuration cannot exceed.
- Pacing for hosts with burst limits (adsb.lol): a request waiting for its slot
  reserves nothing, so cancelled requests never delay later ones.
- `User-Agent: Lavender/<version> (+contact)`. No forwarding of client headers.
- Retries only for idempotent GETs on 429/5xx/timeouts, with jittered backoff,
  and respecting `Retry-After`. A provider can opt out (CelesTrak does, see below).
- Parsed data is validated: coordinate ranges, finite numbers, string length
  caps, enum whitelists. Documents are parsed row by row: a malformed or
  wrong-typed row is dropped, never the whole feed. Dropped rows are counted,
  logged and reported as a snapshot notice ("USGS: 3 malformed records skipped.").
- Error text that leaves a provider is cleaned (control characters removed) and
  capped at 200 characters before it reaches health, logs or stream frames,
  because parser errors can quote upstream content.
- CelesTrak usage policy: each group is cached (disk and memory) and not
  requested again within its cadence (2 h) after a download, nor within
  1/12 of the cadence (10 min) after a failure, however often a snapshot or
  subscription asks; a fetch never retries a group. Data served from a cache
  carries the cache's age (`fetchedAt`), and a failed or partial refresh is
  reported as `Degraded` freshness and `Degraded` health, never as live.

### Core (`lavender-core::hub`)
- At most `hub.max_slots` (256) regions are refreshed at once. A region whose
  last subscriber left lingers (one timer per region) and is evicted at once
  when the table is full, so idle regions never block active ones.
- A region budget caps the distinct regions one client holds (see Server).
  Regions a subscription cannot get are reported as `Partial` coverage with a
  notice and retried when capacity frees up; a request that gets none fails
  with `rateLimited` (HTTP 429).
- Refresh tasks are supervised: a provider panic is reported as a failure and
  the task restarts with backoff. Dropping the core without `shutdown()` stops
  every task. Freshness is re-judged on a timer, so a stalled refresh turns
  `Stale`.

### Server (`lavender-server`)
- Host allowlist (`server.allowed_hosts`), checked for every request. The host
  is the `Host` header or, for HTTP/2 and absolute-form requests, the request
  target's authority; when both are present they must agree.
- `/api` accepts same-origin requests only: `Origin`, when present, must name
  the request's own host with an `http` or `https` scheme, and must be `https`
  when the request arrived over HTTPS (request scheme or `X-Forwarded-Proto`);
  `Sec-Fetch-Site` must not be `cross-site`. CORS is not enabled.
- Errors: every `/api` error, including query and path rejections, non-WebSocket
  requests to the stream, method mismatches, body limits, timeouts (504) and
  caught panics (500, without the panic message), uses the JSON `ApiError`
  envelope. The security headers wrap every response, host refusals included.
- WebSocket: origin check on upgrade; at most 512 connections in total (503,
  `rateLimited`, `Retry-After`) and 16 per client (429); 64 KiB max inbound
  message and frame; at most 10 inbound messages per second per connection
  with a burst of 40 (exceeding it closes with 1008); 10 unreadable messages
  close with 1008; at most 32 subscriptions per connection; the server pings
  every 30 s and closes a connection silent for 75 s (1001); a client that
  does not accept a frame within 10 s is disconnected. All configurable under
  `server.ws_*` and `server.max_connections*`.
- Region budget: one client (IPv4 address or IPv6 /64) may hold at most 32
  distinct feed regions across its streams and snapshots
  (`server.max_regions_per_client`). Regions its subscriptions share count once.
  Behind a reverse proxy, configure `trusted_proxies` or all users share one
  budget.
- Headers: CSP (web variant of the desktop policy, plus `connect-src` for the
  WebSocket), `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY` /
  `frame-ancestors 'none'`, `Referrer-Policy: strict-origin-when-cross-origin`,
  `Permissions-Policy` denying camera, microphone and geolocation (microphone is
  re-enabled only when voice ships).
- Rate limits: token bucket per client on `/api` (configurable). A client is an
  IPv4 address or an IPv6 /64. Buckets live in a bounded cache (100 000
  clients) and expire once they would have refilled. `X-Forwarded-For` is read
  only from configured proxies, right to left, skipping trusted proxies: the
  first untrusted address is the client, so entries a client writes itself are
  ignored.
- Errors never echo upstream bodies or secrets. Logs redact `Authorization`,
  `key`, `token`, `api_key` query parameters.

### Frontend
- No secrets; runtime catalog may contain explicitly public tile keys only.
- No `innerHTML` with provider data; Cesium credit HTML comes only from our own
  registry strings.
- Links to provider records open with `rel="noopener noreferrer"`, `https:` only.

- Diagnostics (`GlobeController.captureFrame`, the desktop self-check) read back only
  Lavender's own canvas; reports go to the local `smoke_report` command, never to the
  network.

### Fixture mode
- `server.fixtures_dir` makes `lavender-server` replay recorded or synthetic provider
  documents from disk instead of contacting sources (end-to-end tests, offline demos).
  It is off by default, logs a warning at start, caps file size at 64 MiB, parses with
  the production parsers (malformed rows dropped and counted), and every snapshot
  carries the notice "Recorded fixture data, not live." so recorded data is never
  presented as live. Fixtures must not contain personal data or content a provider's
  terms forbid redistributing (`apps/web/e2e/fixtures/README.md`).

### Desktop
See `tauri.md` §5.

## 3. Secrets inventory (target)

| Secret | Where held | Scope |
|---|---|---|
| `FIRMS_MAP_KEY` | server env / desktop keychain | FIRMS provider |
| `AISSTREAM_API_KEY` | server env / keychain | AIS provider |
| `OPENSKY_CLIENT_ID` / `_SECRET` | server env / keychain | OpenSky provider (evaluation licence) |
| `TOMTOM_API_KEY` | server env / keychain | Traffic tiles (proxied) |
| `GOOGLE_MAPS_SERVER_KEY` | server env / keychain | Places, tiles session token |
| `CESIUM_ION_TOKEN` | public (browser) if configured | Cesium ion assets; labelled public |
| `MAPILLARY_CLIENT_TOKEN` | public (browser) if configured | MapillaryJS requires a client token |
| `OPENAI_API_KEY` | server env / keychain | Voice (P5) |

## 4. Verification

- Unit tests for the fetch policy: redirect to non-allowlisted host refused;
  oversized body truncated with error; non-HTTPS refused.
- Server tests: foreign `Host` → 403 with security headers; host taken from
  the request target when there is no `Host` header; cross-site or
  scheme-downgraded `Origin` on `/api` → 403; WebSocket from a foreign origin
  refused; rate limit returns 429 with `Retry-After` and resists rotating
  `X-Forwarded-For`; every error path returns the JSON envelope; WebSocket
  floods, idle clients and excess connections are closed or refused.
- Core tests: one linger timer per region; capacity and budget shortfalls
  reported as partial coverage and retried; provider errors capped; CelesTrak
  cache age, degradation and retry floor; malformed rows dropped one by one.
- Build test: built `index.html` contains no inline scripts or styles; the CSP
  contains no `unsafe-eval`.
- `cargo deny` (licences, bans, sources) in lint; advisories daily (monorepo
  pattern).
- Secret scanning: monorepo gitleaks pre-commit hook and CI.
