# Start Here

Lavender is NoirSystems' geospatial intelligence and OSINT application. Analysts
turn layers on, inspect objects with their source and age, and search coordinates
or records already loaded on the map. The interface uses CesiumJS for the globe;
React owns the surrounding controls. Rust fetches, validates and shares provider
records. Tauri hosts the same interface and core in the desktop prototype.

## What works today

The checked-in application has three feed types: earthquakes from USGS, aircraft
from adsb.lol, and satellite orbital elements from CelesTrak. Their parsers, core,
HTTP API and frontend renderers exist. Aircraft queries cover map regions; a wide
view uses the worldwide military feed and explicitly reports partial coverage.
Satellites move through browser SGP4 propagation, with ground tracks calculated in
Rust. Coordinate and loaded-entity search, layer controls, inspection, source
credits, transport status and stored workspace preferences are implemented.

A macOS desktop prototype exists, with a successful recorded self-check from
2026-10-12. Cross-platform packaging, signing and keychain storage remain planned.
Aircraft tracks, OpenSky, satellite passes and place search are still partial
migrations. Ships, fires, cyclones, weather and many other upstream capabilities
remain in the reference app. See the [migration table](rust-rewrite.md) before
assuming that a capability from a specification is available.

## Where things live

- [`apps/web/src`](../../apps/web/src/) contains the interface and globe.
- [`crates/lavender-core/src`](../../crates/lavender-core/src/) contains providers,
  the shared feed hub, licensing and reusable application operations.
- [`crates/lavender-server/src`](../../crates/lavender-server/src/) contains HTTP,
  WebSocket and server configuration.
- [`apps/desktop/src-tauri/src`](../../apps/desktop/src-tauri/src/) contains Tauri commands.
- [`legacy/`](../../legacy/) is the frozen upstream reference, with its original
  license. The new app does not call its Node backend.

This checkout is a separate Git repository inside `products/lavender` in the
NoirSystems monorepo. Run `git rev-parse --show-toplevel` here before branching:
changes belong to Lavender's repository. Monorepo CI integration is still an open
Flokzy task; local Nx discovery does not establish that hosted CI covers it.

## Your first 30 minutes

1. **Minutes 0–10:** follow [Getting Started](getting-started.md), install the web
   dependencies and start Rust plus Vite. Use fixture mode for predictable records
   without contacting live feed providers. Open `http://localhost:5410`.
2. **Minutes 10–15:** inspect an earthquake, turn Aircraft on and search `FIN7KV`
   in fixture mode. Open Sources and compare provenance and freshness. `⌘K` opens
   the command palette on macOS; `/` focuses search.
3. **Minutes 15–25:** read the [aircraft walkthrough](architecture.md#an-aircraft-record-end-to-end).
   Open `FeedController.ts`, the WebSocket handler and `adsb_lol.rs` in that order.
   Run `cargo test -p lavender-core --test providers`.
4. **Minutes 25–30:** use the [roadmap](../roadmap.md#what-to-work-on-next) to find a
   task, read its current Flokzy description, comments and blockers, and check the
   [contribution workflow](contributing.md).

If setup takes longer than ten minutes, use [troubleshooting](getting-started.md#common-setup-problems)
and continue the walkthrough after the application starts. A useful first small
change is a malformed-record regression case in an existing provider test, chosen
with the task's owner rather than inventing a new migration.
