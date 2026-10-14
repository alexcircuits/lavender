# Web and Tauri Architecture

Both environments run `apps/web`. A single platform interface changes how it
calls Rust, so layer controls, inspection and globe rendering remain shared.
The shared Rust core is already used by both adapters; a macOS prototype exists.
Desktop distribution and Windows/Linux readiness remain future work.

| Concern | Web | Current desktop prototype |
|---|---|---|
| Interface | Vite build served by Rust, or Vite in development | Same built `apps/web/dist` in a Tauri WebView |
| Application calls | `WebPlatform`: HTTP and one multiplexed WebSocket | `TauriPlatform`: `invoke` commands and `Channel` updates |
| Rust application | `lavender-server` builds the core | Tauri `main.rs` builds the same core in-process |
| Feed work | Rust server makes upstream requests | Rust process in the app makes upstream requests |
| Preferences | Browser preference store | Same current local preference implementation |
| Secrets | `EnvSecretStore` reads server environment / `_FILE` | `MemorySecretStore`: session-only; keychain task ENG-997 remains open |
| Settings/cache | Configured `var/data`, `var/cache` by default | OS app config/cache directories |
| Network socket | HTTP listen address required | No localhost HTTP server required for core calls |

[`createPlatform`](../../apps/web/src/platform/index.ts) chooses the adapter at
startup using Tauri's `isTauri` global. Components use
[`LavenderPlatform`](../../apps/web/src/platform/types.ts). Tauri imports are confined
to `src/platform`, and [`architecture.test.ts`](../../apps/web/src/architecture.test.ts)
enforces that boundary. Concrete Web/Tauri classes implement the same contract.

## One practical example

To fetch earthquakes from a screen or controller:

```ts
const snapshot = await platform.snapshot({ feed: 'earthquakes', filters: {} });
```

Web sends `GET /api/v1/feeds/earthquakes`; desktop invokes `feed_snapshot`.
Both adapters call `LavenderCore::snapshot`. The same `FeedSnapshot` then reaches
the existing renderer. Live feeds use `platform.subscribe(query, onEvent)`;
call the returned handle's `close()` when no longer needed.

Native-only work belongs behind the platform interface or a clearly named
optional capability when it is implemented. Current diagnostics are one example:
the desktop self-check can hand a report to Rust through `platform.diagnostics`.
No settings/keychain UI or remote-backed desktop mode should be inferred from
that hook. Remote-backed mode, custom binary/tile protocols and packaged updates
are specification targets, not current features.

## Build and verify the current prototype

Build the frontend first, then the desktop app:

```bash
pnpm --dir apps/web run build
cargo build --locked -p lavender-desktop --features custom-protocol
```

Default Cargo members skip desktop, so normal backend work does not require
WebView system libraries. Desktop prerequisites depend on OS; the
[detailed Tauri plan](../architecture/tauri.md) records native requirements and
platform tests still needed.

The existing self-check runs the default live earthquake layer, captures a frame,
checks console/CSP failures and records measured frame rate. From Lavender's root:

```bash
LAVENDER_SMOKE_OUT=/tmp/lavender-smoke LAVENDER_SMOKE_EXIT=1 ./target/debug/lavender-desktop
```

Use an unlocked desktop session; occluded or locked WebViews stop rendering.
Read [the recorded report](../architecture/evidence/2026-10-12-desktop-self-check.json)
and [frame](../architecture/evidence/2026-10-12-desktop-self-check.png). That evidence
supports macOS feasibility; it does not establish Windows/Linux behavior, signing,
updater safety or production performance. Flokzy ENG-996 remains In Progress in
the audit snapshot despite the later successful rendering report; keep the
verification evidence attached when reconciling its status.
