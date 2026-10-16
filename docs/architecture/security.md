# Lavender — security specification

- Status: v1.2, 2026-10-14 (regression coverage: see the PROGRESS log)
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
- Public providers resolve once per connection, validate every returned address
  (at most 64), and pass that exact set to the connector. Private, reserved,
  mixed public/private and embedded-private answers fail before connecting;
  literal IPs and each redirect receive the same checks. Ambient proxy settings
  are disabled. Explicit `fetch.allow_http_hosts` entries are development/internal
  exceptions that permit HTTP and private addresses; the default list is empty.
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
  caps, enum whitelists. Row-oriented providers drop malformed rows and count them;
  cyclone status requires a valid whole document, while malformed GIS geometry
  leaves valid storm positions available with degraded geometry. Dropped rows are counted,
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
  `key`, `token`, `api_key` query parameters. Request spans record method/path
  without query strings; caught panics record fixed diagnostics without payloads.
  Both executable adapters install the redacting process panic hook before
  starting runtime work, so the default hook cannot print a caught panic's secret.

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

The canonical local gate is `scripts/check.sh`. Rust tests run against real router,
fetcher and WebSocket paths; build fixtures exercise the same exported checker as
`postbuild`. The existing tests below are retained rather than duplicated.

| Control | Executable regression coverage |
|---|---|
| HTTPS and host/redirect allowlist; fetch body ceiling and timeouts | Core `fetch::tests::{refuses_hosts_outside_the_policy_and_plain_http, refuses_redirects_to_other_hosts, caps_body_size, per_request_body_caps_are_authoritative_within_the_ceiling, times_out}` |
| Public address validation, DNS rebinding and mixed answers | Core `fetch::dns::tests::{refuses_private_reserved_and_embedded_private_addresses, rejects_mixed_answers_and_rebinding_then_passes_exact_validated_set}`; `tests/fetch_public_dns.rs::private_literals_and_localhost_dns_cannot_reach_the_socket` |
| Query secrets excluded from upstream failure text | Core `fetch::tests::does_not_retry_404_and_reports_host_only`; HTTP `security_regression_tests::request_logs_and_error_envelopes_exclude_query_and_authorization_secrets` captures DEBUG tracing output and checks the error envelope |
| Host authority and security headers on refusals | Server `api::{foreign_hosts_and_cross_site_origins_are_refused, requests_without_a_host_header_use_the_target_authority, host_refusals_carry_the_security_headers}` |
| Same-origin scheme and WebSocket upgrades | Server `api::{scheme_downgraded_origin_is_refused_through_the_router, websocket_from_a_foreign_origin_is_refused}` plus `security::tests::same_origin_checks_the_scheme` |
| Rate limits, positive Retry-After, rotating X-Forwarded-For, IPv6 /64 | Server `api::{rate_limit_returns_429_with_retry_after, rotating_untrusted_forwarded_for_cannot_bypass_router_rate_limit}`; `security::tests::{forwarded_for_is_read_right_to_left_from_trusted_proxies_only, ipv6_clients_are_keyed_by_their_slash_64, the_bucket_table_is_bounded}` |
| JSON envelopes, body limits, caught panic privacy | Server `api::{query_and_path_rejections_use_the_api_envelope, method_mismatches_and_oversized_bodies_use_the_api_envelope, timeouts_use_the_api_envelope}`; `security_regression_tests::caught_panic_response_and_logs_exclude_panic_payload` checks the actual panic response factory without invoking the process panic hook |
| Process panic-hook privacy, including backtrace-enabled execution | Core `tests/panic_redaction.rs::actual_panic_payload_never_reaches_stdout_or_stderr` invokes its isolated child with `RUST_BACKTRACE=1` and checks captured process output; the child is marked ignored for ordinary discovery and explicitly executed by the parent regression |
| WebSocket total/per-client admission, headers, subscription limit | Server `api::{websocket_total_capacity_refusal_has_retry_after_and_security_headers, websocket_connections_are_capped_per_client, websocket_subscription_limit_does_not_evict_an_existing_subscription}` |
| WebSocket flood, malformed, single-frame and fragmented-message budgets | Server `api::{websocket_message_floods_are_closed, websocket_unreadable_message_budget_closes_with_policy_violation, websocket_oversized_frame_is_disconnected_before_application_parsing, websocket_fragmented_message_is_capped_across_individually_small_frames}` |
| WebSocket idle and slow consumer cleanup | Server `api::idle_websockets_are_closed`; `ws::tests::a_client_that_stops_reading_ends_the_writer` uses a sink that never becomes ready |
| Region capacity, restart, provider error bounds and row isolation | Core `tests/hub.rs` capacity/budget/linger/restart regressions and `provider_errors_are_capped_before_frames_health_and_logs`; provider parser row tests. The error-bound test checks stream/health content, while captured-log tests above specifically check request and panic diagnostics |
| Conditional cache, restart persistence, stale failure and retry floor | Core `tests/celestrak_revalidation.rs` and CelesTrak provider tests |
| Built HTML, desktop CSP, secret shapes, engine split and entry budget | `node --test apps/web/scripts/check-dist.test.mjs` plants violations into passing disposable builds; `apps/web/scripts/check-dist.mjs` checks both actual workspace and docs entry pages during `postbuild` |
| Server CSP | `security::tests::csp_has_no_unsafe_eval_and_lists_tile_hosts` and `api::static_files_spa_fallback_caching_and_security_headers` check the real HTTP header |

Build checks reject actual inline scripts, style elements, style attributes and event
handlers in entry HTML. They inspect the desktop CSP directive map for the exact
`unsafe-eval` permission and validate the separate `script-src-elem` and `script-src-attr`
contexts after case normalization. Directive names must follow the ASCII directive
grammar; raw values cannot contain semicolons, controls or line delimiters, including
within array elements, so verbatim serialization cannot inject another directive.
Element overrides permit only self or none; attribute
overrides permit only none. The separate WASM exception and runtime inline styles remain
explicitly permitted by the architecture. Documentation text and code samples naming
forbidden APIs do not trip the checks. Recognisable Google/OpenAI/private-key/JWT shapes
and labelled server credential assignments, including the actual `LAVENDER_*` secret
environment names, are scanned in emitted assets; diagnostics
name the file without repeating a credential. This is a regression backstop, not a
complete detector for arbitrary opaque credentials. Explicit public tile tokens stay governed by
§3 and the runtime catalog policy rather than being embedded in build artifacts.

`cargo deny --locked check` runs locally; the monorepo lint target checks licences, bans
and sources. `.github/workflows/security-advisories.yml` adds an uncached daily RustSec
check and dependency-change/manual triggers to the product repository, with read-only
permissions, an immutable checkout action and pinned cargo-deny 0.20.2. The workflow has
been pushed on the development branch but has not been observed on a hosted runner.
GitHub currently reports no registered workflows for this repository. A nested product
workflow is not executed by the host monorepo: that wiring remains ENG-1004.

The host monorepo contains `.githooks/pre-commit` and a checksum-pinned gitleaks 8.30.1
CI job with a planted-private-key control and full-history scan. Its configured hook
path applies to the host checkout; the nested Lavender Git repository currently has no
hook override. The hook explicitly skips scanning when gitleaks is unavailable. Neither
hosted job execution nor coverage of the nested Lavender history is inferred from the
presence of these files; secret-scanning integration remains an explicit release check.
A local gitleaks 8.30.1 planted-key control was exercised during the regression work.
The Vite transform removes the unused vendor default Ion token before emission, verified
against the installed `@cesium/engine/Source/Core/Ion.js` assignment; the globe still
clears `Ion.defaultAccessToken` at startup. The fresh production build passed the
distribution checker, and an independent gitleaks 8.30.1 scan of 18.03 MB found no leaks
without suppressing the vendor JWT finding. This is local evidence for that build;
hosted workflow activation and nested-repository scanning integration remain ENG-1004.
