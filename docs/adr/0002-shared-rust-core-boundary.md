# ADR 0002 — A transport-independent Rust core, with thin HTTP and Tauri adapters

- Status: Accepted
- Date: 2026-10-11

## Problem

Lavender runs in two deployment modes that must share all non-presentation
logic:

- **Web**: a browser talks to a hosted Rust server over HTTP and WebSocket.
- **Desktop** (future): the same frontend inside a Tauri WebView talks to Rust
  in the same process over Tauri IPC (commands and channels), optionally also
  reaching remote services.

If provider integration, caching, normalisation or scheduling live inside Axum
handlers, the desktop build would have to either run a localhost HTTP server
(an extra attack surface, port conflicts, firewall prompts) or duplicate logic.

## Options

1. **Logic in Axum handlers; desktop embeds the Axum server on localhost.** Fast
   to start. It leaves an open port that every local process and every web page
   can reach (DNS rebinding), keeps a second transport alive inside the desktop
   app, and makes "Tauri-ready" a euphemism.
2. **Logic in a core library; Axum and Tauri are both adapters.** Some upfront
   design of the service API. Both adapters stay thin and testable.
3. **Desktop as a pure client of a remote Lavender server.** Simple, but no
   offline or local capability, and private keys stay with whoever hosts the server.

## Decision

Option 2, with option 3 available as a *configuration* of the desktop app (a
remote-backed mode) rather than as its architecture.

### Crate responsibilities

| Crate | Owns | Must not depend on |
|---|---|---|
| `lavender-domain` | Domain types (`Aircraft`, `Vessel`, `Earthquake`, `Satellite`, `FeedSnapshot`, `SourceInfo`, `ProviderHealth`, `BBox`, …), validation, error codes, TS bindings (ts-rs) | tokio, reqwest, axum, tauri |
| `lavender-geo` | Pure computation: geodesy, bbox/antimeridian logic, region keys, SGP4 propagation, spatial indexes | any I/O |
| `lavender-core` | `LavenderCore` service facade, provider adapters, HTTP fetch policy (allowlists, timeouts, size limits), caching and request coalescing, the feed hub (subscriptions and refresh scheduling), health tracking, settings and secret *traits* | axum, tower-http, tauri |
| `lavender-server` | Axum routing, WebSocket framing, static hosting, CSP and security headers, host checks, rate limits, env/TOML config loading, env-backed secret store | tauri |
| `apps/desktop/src-tauri` | Tauri commands and channels, OS keychain secret store, app-data-dir settings, window and capability configuration | axum |

### The service surface

`LavenderCore` exposes async methods that take and return domain types. The
shape is the same for both transports:

- `catalog()` lists layers and sources with attribution, licence, required keys, and whether each is configured.
- `snapshot(FeedQuery)` returns one `FeedSnapshot` (request/response).
- `subscribe(FeedQuery)` returns a `FeedSubscription`, a stream of `FeedSnapshot` updates.
  On the web it becomes WebSocket frames; on desktop, `tauri::ipc::Channel` messages.
- `search(SearchQuery)` covers geocoding plus entity search across live feeds.
- `satellite_track(...)` and similar computations are pure-core.
- `status()` reports provider health and freshness.
- `settings()` / `update_settings(...)` go through a `SettingsStore` trait implemented per adapter.

Secrets enter through a `SecretStore` trait: environment and secret files on the
server, the OS keychain (keyring) on desktop. The core never reads environment
variables or files directly; adapters build a typed `CoreConfig` and hand it in.

## Consequences

- Adding a provider touches only `lavender-domain` (if it adds a type) and
  `lavender-core`. Both transports pick it up without adapter changes.
- Both adapters are integration-tested against the same core behaviour.
- The desktop build needs no listening socket.
- The core must stay runtime-agnostic beyond Tokio (Tauri also runs Tokio, via
  `tauri::async_runtime`), and must be cancellation-correct: dropping a
  subscription stops its refresh work.
- Discipline is required: a reviewer rejects business logic in a route handler
  or a Tauri command.
