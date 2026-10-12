# ADR 0005 — One platform interface, two transports, subscription-based live data

- Status: Accepted
- Date: 2026-10-11
- Evidence: Tauri channel throughput in `docs/architecture/spikes/2026-10-11-tauri-cesium-macos.md`

## Problem

GEV has no push channel at all. Every layer polls its own route on its own
timer and most receive whole-world payloads (`docs/audit/upstream-backend.md`
§2.0; `docs/audit/upstream-layers.md` §8). Lavender needs live updates that:

- coalesce upstream requests across viewers and layers;
- filter spatially on the server;
- work identically over HTTP/WebSocket (web) and Tauri IPC (desktop);
- degrade honestly (stale, degraded, unconfigured) instead of silently.

## Options for the live transport

| Option | Web | Desktop | Notes |
|---|---|---|---|
| Client polling per layer (GEV) | ✓ | ✓ (invoke) | N timers, no coalescing, wasted bandwidth |
| Server-Sent Events | ✓ simple, auto-reconnect | n/a | Changing a viewport means reopening the stream; one stream per subscription or a separate control channel |
| **WebSocket, multiplexed** | ✓ one socket, many subscriptions, client→server viewport updates | n/a | Needs a small protocol |
| **Tauri `Channel` per subscription** | n/a | ✓ ordered, fast (200 msgs / 12 ms measured) | Native fit for streams |

## Decision

### Frontend: `LavenderPlatform`

A typed interface in `apps/web/src/platform/types.ts`, implemented by
`WebPlatform` (fetch + WebSocket) and `TauriPlatform` (invoke + Channel).
The composition root picks the implementation once, with `isTauri()`; nothing
else branches on environment.

```ts
interface LavenderPlatform {
  readonly kind: 'web' | 'desktop';
  catalog(): Promise<Catalog>;
  status(): Promise<SystemStatus>;
  snapshot(query: FeedQuery, opts?: { signal?: AbortSignal }): Promise<FeedSnapshot>;
  subscribe(query: FeedQuery, onUpdate: (u: FeedUpdate) => void): FeedSubscription; // update(query), close()
  search(query: SearchQuery, opts?: { signal?: AbortSignal }): Promise<SearchResults>;
  satelliteTrack(req: SatelliteTrackRequest): Promise<SatelliteTrack>;
  preferences: PreferenceStore; // per-device UI state (localStorage / Tauri store)
}
```

The request and response types are generated from Rust (ADR 0007), so both
sides of every call share one definition.

### Live data model

- `FeedQuery = { feed, bbox?, filters? }`. The core turns it into a *region key*
  (a quantised bbox or "global") and runs **one refresh task per active
  (feed, region key)**, shared by every subscriber of that key. When the last
  subscriber leaves, the task is cancelled.
- Updates are **latest-value** (`tokio::sync::watch`). A slow consumer skips to
  the newest snapshot and never queues stale ones; this is the correct semantics
  for positional feeds.
- Each update is a full `FeedSnapshot` for that subscription's bbox, carrying its
  metadata (source, fetched-at, freshness state, attribution, notices).
  Snapshots are small after server-side bbox filtering. Deltas are a later
  optimisation once measurements require them (performance spec).
- Event-stream feeds (AIS) keep a store in the core. Subscriptions read
  bbox-filtered views of it on a fixed cadence.

### Web transport

- `GET /api/v1/feeds/{feed}?bbox=…` returns a snapshot (with `ETag` /
  `If-None-Match`).
- `GET /api/v1/stream` is a WebSocket carrying JSON frames:
  - `{op:"subscribe", id, query}`, `{op:"update", id, query}`, `{op:"unsubscribe", id}`
  - `{type:"snapshot", id, snapshot}`, `{type:"error", id, error}`, `{type:"status", status}`
- The client reconnects with jittered backoff and resubscribes. If WebSockets
  are unavailable, the web adapter falls back to snapshot polling at the feed's
  cadence; the UI shows that state as "polling".

### Desktop transport

- `invoke('feed_subscribe', { query, channel })` returns a subscription id;
  `feed_update` and `feed_unsubscribe` complete the set. The Rust side forwards
  the same `FeedUpdate` values from the same core subscription.
- Window close drops every subscription (Rust holds them per webview label).

## Consequences

- Upstream request volume scales with active (feed, region) pairs, not with
  viewers × layers.
- One protocol definition serves both transports and their tests.
- The WebSocket endpoint needs origin checks, per-connection subscription caps
  and message-size limits (security spec).
- Moving from snapshots to deltas is backwards-compatible: a new
  `FeedUpdate` variant.
