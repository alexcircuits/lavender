# ADR 0006 — Storage, secrets and configuration per deployment mode

- Status: Accepted
- Date: 2026-10-11

## Problem

GEV keeps provider keys in a `.env` file that a dev-server endpoint rewrites,
and then restarts the server so Vite can bake three "browser" keys into the
bundle (`docs/audit/upstream-backend.md` §5, §9.2). Disk caches resolve against
the current working directory. Lavender has three kinds of state with
different owners:

| State | Example | Owner |
|---|---|---|
| Operator configuration and secrets | FIRMS map key, AISStream key, OpenSky client credentials | whoever runs the backend (server operator / desktop user) |
| Workspace data | saved views, annotations, investigations (future) | the analyst, durable |
| Per-device preferences | panel widths, theme, last camera | the device, disposable |
| Caches | provider responses, TLE catalogues, tiles | the runtime, disposable |

## Decision

### Core abstractions (in `lavender-core`)

- `SecretStore` trait: `get(SecretKey) -> Option<SecretString>`, plus
  `set/delete` for adapters that support them. `SecretKey` is an enum of known
  keys (no arbitrary names), and values use `secrecy::SecretString` so they never
  appear in `Debug` output or logs.
- `SettingsStore` trait: typed `CoreSettings` (enabled providers, refresh
  overrides, units), loaded and saved as a whole document with a schema version.
- `CacheDir`: an explicit path handed in by the adapter. The core never uses
  the working directory.

### Web server (`lavender-server`)

- Configuration: an optional TOML file plus environment overrides
  (`LAVENDER__SECTION__KEY`, the ProxyPulse convention). All errors are reported
  together at startup, and unknown keys are rejected.
- Secrets: environment variables or `*_FILE` paths (container secrets). They are
  read-only at runtime. Changing a key is an operator action plus restart; there
  is no HTTP endpoint that writes secrets. GEV's key-setup UI is not carried over
  to the web server.
- Browser-usable credentials (map-tile keys that providers require to be
  sent from the browser) are delivered at **runtime** through `/api/v1/catalog`,
  only when an operator has explicitly marked them as public.
- Workspace data: SQLite (`rusqlite`, bundled) under `LAVENDER_DATA_DIR`, once
  workspaces exist (roadmap Phase 5+). Multi-user deployments move to Postgres
  behind the same repository traits; that is an enterprise roadmap item.

### Desktop (`apps/desktop`)

- Secrets: the OS credential store via the `keyring` crate (macOS Keychain,
  Windows Credential Manager, Secret Service on Linux), implementing
  `SecretStore`. The settings UI writes keys through one narrowly-scoped command
  per key; keys are never returned to the WebView.
- Settings and workspace data: `app_config_dir()` / `app_data_dir()`, with the
  same SQLite schema as the server. Caches go in `app_cache_dir()`.
- Per-device UI preferences: `tauri-plugin-store` with a single store file, or
  `localStorage`, behind the `PreferenceStore` platform interface.

### Browser

- Per-device preferences only, in `localStorage`, behind `PreferenceStore`.
  No secrets ever live in the browser.

## Consequences

- No secret can reach a JS bundle at build time; there is no `define`-time key
  injection.
- Desktop users can enter their own provider keys and use them locally, with no
  remote server. This is the main reason the core is embeddable.
- Linux Secret Service availability varies by distribution. When it is missing,
  the desktop app falls back to session-only keys (not persisted) and says so,
  rather than writing secrets to plain files.
