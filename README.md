# Lavender

Geospatial intelligence and OSINT platform by NoirSystems: a Rust core that ingests and
validates real-time spatial data, served to a TypeScript interface on a CesiumJS globe, on
the web and as a Tauri desktop app built on the same core.

Lavender is proprietary software (see [Licence](#licence)). It began as a rewrite of the
open-source [God's Eye View](legacy/README.md); that upstream project is kept intact under
[`legacy/`](legacy/) as the behavioural reference while features are migrated, and its
history is this repository's history.

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

Requirements: Rust (version pinned in `rust-toolchain.toml`), Node 22+, pnpm 12 installed
standalone (https://pnpm.io/installation; corepack cannot start pnpm 12).

```bash
cd apps/web && pnpm install && pnpm run build   # frontend into apps/web/dist
cargo run -p lavender-server                      # API + stream + frontend on 127.0.0.1:7410
```

Development with hot reload: `cargo run -p lavender-server` in one shell and
`pnpm run dev` in `apps/web` (Vite on :5410 proxies `/api`).

Offline, without contacting any data source (recorded/synthetic fixtures):

```bash
LAVENDER__SERVER__FIXTURES_DIR=apps/web/e2e/fixtures cargo run -p lavender-server
```

Desktop: `cargo build -p lavender-desktop --features custom-protocol` after building the
frontend (see `docs/architecture/tauri.md`).

## Checks

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace && cargo deny check && scripts/check-contracts.sh
cd apps/web && pnpm run typecheck && pnpm test && pnpm run lint && pnpm run build
PLAYWRIGHT_CHANNEL=chrome pnpm run e2e     # against lavender-server in fixture mode
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
