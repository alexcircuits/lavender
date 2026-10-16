# Lavender

Geospatial intelligence and OSINT platform by NoirSystems: a Rust core that ingests and
validates real-time spatial data, served to a TypeScript interface on a CesiumJS globe, on
the web and as a Tauri desktop app built on the same core.

Lavender is proprietary software (see [Licence](#licence)). It began as a rewrite of the
open-source [God's Eye View](legacy/README.md); that upstream project is kept intact under
[`legacy/`](legacy/) as the behavioural reference while features are migrated, and its
history is this repository's history.

## Development documentation

Start with [Lavender Development Documentation](docs/README.md): verified setup,
architecture walkthrough, migration status, provider tutorial and contribution guide.
[The roadmap](docs/roadmap.md) explains the next milestones; Flokzy holds current task status.

The browser documentation uses Lavender's design system, with keyboard search,
section links, highlighted examples, diagrams and a reader for linked source files.
Run `pnpm --dir apps/web run dev` and open [Developer docs](http://localhost:5410/docs/).
Reading docs requires only Vite; Rust is needed when opening the workspace.
The production build also serves `/docs/` through `lavender-server`.

## Layout

| Path | What |
|---|---|
| `crates/lavender-domain` | Contract types shared by every adapter, exported to TypeScript (ts-rs) |
| `crates/lavender-geo` | Geodesy, antimeridian-safe boxes, region cells, DMS/MGRS, SGP4 |
| `crates/lavender-core` | Egress-safe fetcher, feed hub, providers, licence gate, search: no Axum or Tauri |
| `crates/lavender-server` | Axum adapter: HTTP API, WebSocket stream, the built frontend, security layers |
| `apps/desktop/src-tauri` | Tauri 2 adapter: commands and channels over the same core |
| `apps/web` | React + TypeScript interface, design system, lazily loaded Cesium globe |
| `docs/` | Product spec, architecture, ADRs, design system, audits, progress log |
| `legacy/` | Upstream God's Eye View, unchanged |

## Develop

Current feeds (implementation checked 2026-10-14): USGS earthquakes, adsb.lol aircraft,
CelesTrak satellites and NOAA NHC/CPHC tropical cyclones. The workspace also supports
bounded pins, distance arrows, areas and share links with annotations; measurements are
approximate. Other upstream features and final acceptance checks are still being
migrated; see the [status table](docs/development/rust-rewrite.md).

Requirements: pinned Rust, Node 22.12+ and pnpm 12.11.2. See
[Getting Started](docs/development/getting-started.md) for verified installation
alternatives and the monorepo development command.

```bash
pnpm --dir apps/web install --frozen-lockfile
pnpm --dir apps/web run build   # frontend into apps/web/dist
cargo run -p lavender-server                      # API + stream + frontend on 127.0.0.1:7410
```

Development with hot reload: `cargo run -p lavender-server` in one shell and
`pnpm run dev` in `apps/web` (Vite on :5410 proxies `/api`).

Use recorded/synthetic feed fixtures (globe imagery can still use network tiles):

```bash
LAVENDER__SERVER__FIXTURES_DIR=apps/web/e2e/fixtures cargo run -p lavender-server
```

Desktop: `cargo build -p lavender-desktop --features custom-protocol` after building the
frontend (see `docs/architecture/tauri.md`).

## Checks

```bash
./scripts/check.sh                         # Rust + frontend, no native desktop libraries required
PLAYWRIGHT_CHANNEL=chrome pnpm --dir apps/web run e2e     # against lavender-server in fixture mode
```

## Data sources and attribution

Every source, its licence and its attribution are registered in the core and shown in the
app's Sources panel; `docs/audit/data-sources-and-licensing.md` is the inventory and
`THIRD_PARTY_NOTICES.md` the notices. Licence mode `evaluation` (default) enables sources
that do not allow commercial use; `commercial` disables them.

## Licence

Copyright © 2026 NoirSystems. All rights reserved. Lavender is proprietary and is **not**
released under the MIT License or any other open-source licence; see [`LICENSE`](LICENSE).

Third-party code keeps its own licence. The upstream God's Eye View source in `legacy/` is
MIT-licensed by its authors, and that licence requires its notice to stay with the code, so
`legacy/LICENSE` and the notices in `THIRD_PARTY_NOTICES.md` remain.
