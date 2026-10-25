# Language fit review: where Rust, Zig, Mojo, Nim and TypeScript belong

Written 2026-10-24, reviewing the rewrite after the 1095-commit upstream import
(`30047df2`) and the second-wave checkpoint (`0eaebbb3`). Companion to
ADR 0009 (targeted polyglot performance extensions) — this file records the
*review* of how the split is holding up and what changes.

## Verdict per language

| Language | Role | Verdict |
|---|---|---|
| **Rust** | Core: domain contracts (ts-rs), geodesy/SGP4, feed hub, providers, licence gate, server, Tauri commands | **Keep as the spine.** Nothing here is better in another language; contracts → TS generation is the load-bearing bridge. |
| **TypeScript** | Interface: React shell, Cesium renderers, platform adapters, design system | **Keep.** The Cesium/React ecosystem is TS-first; upstream's own TS is the behavioural reference. |
| **Zig** | Micro-kernels compiled to `wasm32-freestanding` (`kernels/zig`): SGP4 batch, CPR/ADS-B, wind sampling, geometry (RDP), labels, CCTV frustums; loaded by `platform/wasmLoader.ts` | **Keep, but finish the wiring.** The WASM path works in the browser (renderer tests pass without a Zig toolchain because the artifact is committed), but nothing rebuilds `apps/web/public/wasm/lavender_kernels.wasm` from source, so it can silently drift. |
| **Mojo** | Vision sidecar `services/lavender-vision` (`perception.mojo`: SIMD bbox/IoU/NMS) over a unix socket; Rust IPC client `providers/cctv/vision_ipc.rs` | **Keep for CCTV detection.** Same gap as Zig: no build orchestration — the sidecar must be running out-of-band for detections to appear. |
| **Nim** | `kernels/nim` CEP rule engine dylibs (`liblavender_rules.dylib`, `liblavender_cep.dylib`); Rust counterpart `core/src/analytics/cep.rs` exists | **Decide, don't drift.** Nothing loads the dylibs. Either wire them behind a feature flag with a build script, or retire the Nim experiment and keep the Rust CEP. Recommendation: retire unless a rule hot-loop actually needs it — one rule engine is enough to test. |
| **DuckDB** (embed, via Rust) | Analytics sink `core/src/analytics/{cep,queries,sink}.rs` | **Keep.** It is the natural base for upstream's query tool packs (LB-121). |

## Actions

1. **Kernel provenance (LB-130).** Add a script (and CI step) that rebuilds the
   Zig WASM artifact with a pinned Zig version and diffs it against the
   committed one; record the toolchain in `kernels/zig/build.zig.zon`. Same for
   a `zig build test` gate. Without this the committed binary is unverifiable.
2. **Sidecar lifecycle (LB-131).** `lavender-server` should optionally spawn
   and health-check `services/lavender-vision` (flag off by default), so CCTV
   detections don't depend on a manual process.
3. **Nim decision (LB-132).** Per the verdict above; if retired, move the code
   to `spikes/` and note it in ADR 0009.
4. **`unsafe` policy.** The workspace lint was relaxed `forbid` → `deny` in the
   WIP (Cargo.toml). Keep `deny`, but confine any `unsafe` blocks to a single
   audited FFI module per crate that needs them, with `// SAFETY:` comments;
   clippy's `undocumented_unsafe_blocks` should stay enforced.
5. **Zig native lib.** `build.zig` also emits a C-ABI static lib. If the SDR
   lane (LB-117) lands, prefer that artifact linked into a small Rust binary
   over a second WASM path — one kernel, two targets, no JS in the loop.

## Why this split (for future reviewers)

- The browser cannot call Rust; it *can* call WASM — hence Zig kernels for
  renderer hot paths (label layout, streamline geometry, propagation) where
  JS was the bottleneck and Rust-on-server would add latency or traffic.
- Mojo buys SIMD ergonomics for the perception model; the alternative (Rust +
  ndarray) is fine but the sidecar isolates model churn from the server.
- Everything that must be trustworthy (ingestion, licensing, contracts, maths
  used by both sides) stays in Rust, once, behind tests.

## Provenance note (2026-10-24, LB-130)

`scripts/build-zig-kernels.sh` rebuilds `apps/web/public/wasm/lavender_kernels.wasm`
from `kernels/zig` with a pinned toolchain (`0.16.0-dev.2296+fd3657bf8` via zigup),
runs the Zig test suite and prints the artifact's sha256. Run it after any kernel
change and commit the refreshed artifact with the source.

RESOLVED 2026-10-25: the 0.16-dev build-runner could not link on macOS
(`undefined symbol: _sigemptyset/_sysctlbyname`), but Zig 0.16.0 stable can —
the pin is now `0.16.0` (installed via `zigup 0.16.0`; one `var`→`const` lint
fix in labels.zig was the only source change). All 46 kernel tests pass and the
committed artifact was rebuilt through the script (sha256
d9de1487…54ffbe), so the heatmap and point-in-polygon kernels are live in the
browser path; the pure-TS fallback in `heatmapBin` remains as the graceful
degradation for older artifacts.
