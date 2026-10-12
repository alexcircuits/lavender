# ADR 0001 — Repository layout, and keeping the upstream as a frozen reference

- Status: Accepted
- Date: 2026-10-11

## Problem

`products/lavender` started as an untracked, verbatim copy of God's Eye View
(GEV): a 210 000-line vanilla-JavaScript CesiumJS application whose backend is
18 500 lines of Node.js mounted as Vite dev-server middleware. Lavender replaces
that backend with Rust, replaces the interface with a redesigned TypeScript
application, and must later ship as a Tauri desktop app.

Two layouts were possible:

1. **In-place strangler.** Keep GEV's tree at the product root and replace it file by file.
2. **New tree beside a frozen reference.** Build Lavender's Rust workspace and
   frontend at the product root, and keep GEV intact under `legacy/` until
   feature parity is verified, then delete it.

## Options and trade-offs

| | In-place strangler | New tree + frozen `legacy/` |
|---|---|---|
| Interface redesign | Fights GEV's DOM ownership (`src/ui/shellFacade.js`, rails, 2 800-line theme CSS) on every change | Clean component architecture from the first commit |
| Backend contract | Must keep GEV's 40+ ad-hoc `/api/*` routes alive in Rust | New typed `/api/v1` contract designed for two transports (HTTP and Tauri IPC) |
| Baseline measurements | Baseline disappears as code changes | `legacy/` stays runnable for before/after benchmarks and behavioural comparison |
| Risk of silent feature loss | Low (old code keeps running) | Mitigated by the feature-parity matrix (`docs/audit/feature-parity-matrix.md`) |
| Repository size during migration | Smaller | Larger until `legacy/` is deleted (Phase 8) |

## Decision

Option 2. The layout is:

```
products/lavender/
├── Cargo.toml            Rust workspace (crates/* and apps/desktop/src-tauri)
├── rust-toolchain.toml   1.99.0 (latest stable; was 1.98.0 until 2026-10-12)
├── crates/
│   ├── lavender-domain   typed domain model + validation; TS bindings via ts-rs
│   ├── lavender-geo      pure geospatial computation (no I/O)
│   ├── lavender-core     application core: providers, cache, feed hub, services
│   └── lavender-server   Axum HTTP/WebSocket adapter + static hosting (binary)
├── apps/
│   ├── web/              TypeScript + React + Vite frontend (one frontend)
│   └── desktop/          Tauri 2 shell around apps/web/dist and lavender-core
├── docs/                 specifications, audits, ADRs, roadmap, progress log
├── spikes/               throwaway feasibility experiments (never imported)
├── legacy/               GEV as copied; read-only reference until Phase 8
└── project.json          Nx registration (monorepo contract)
```

`legacy/` is **read-only**. Nothing outside it imports from it. Its own CI
workflow is inert (GitHub only runs root workflows) and is listed for removal
in the release-readiness work. It is deleted after the parity matrix is
signed off. Its MIT licence notice is carried forward in `THIRD_PARTY_NOTICES.md`
at the product root, independent of `legacy/`.

## Consequences

- The Rust API does not have to emulate GEV's route shapes; it is designed once
  for HTTP and IPC.
- Every GEV feature must appear in the parity matrix with an explicit
  Preserve / Improve / Replace / Remove / Defer decision, because nothing
  carries over implicitly.
- `legacy/` needs Node 24 to run (`"node": ">=24.14.0 <25 || >=26 <27"`); the new
  frontend targets the monorepo's Node 22 floor.
- `spikes/` code is evidence, not product. It is excluded from builds and may
  be deleted once its findings are recorded in `docs/`.
