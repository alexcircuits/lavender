# Engineering Verification — 2026-10-13

This record starts with the contributor readability pass and preserves its
historical evidence. The autonomous feature work below records later changes
and verification; neither pass claims completion of the wider feature migration.
Changes are in the nested Lavender repository on
`improvement/human-first-engineering`; they are local and uncommitted.

## What changed and why

The four crates remain: domain contracts, pure geographic calculations, reusable
core, and HTTP server. Each has a concrete responsibility, and the core has two
real consumers (HTTP and Tauri). Consolidating them would blur useful boundaries;
creating more service/provider crates would add navigation without solving a need.

The large hub now has private modules for region leases/capacity, refresh/retry,
snapshot composition and subscriber lifecycle. The public types and entry points
remain in `hub.rs`. Fetcher, security and WebSocket tests moved to adjacent test
files. CelesTrak parsing is separate from cache/network work, with the existing
public `providers::celestrak::parse` export retained.

| Main implementation file | Before | After | Where the extracted work lives |
|---|---:|---:|---|
| `hub.rs` | 1,128 lines | 280 lines | `hub/{slots,refresh,snapshot,subscription,tests}.rs` |
| `fetch.rs` | 656 | 384 | `fetch/tests.rs` |
| `providers/celestrak.rs` | 476 | 320 | `celestrak/{parsing,tests}.rs` |
| `security.rs` | 518 | 343 | `security/tests.rs` |
| `ws.rs` | 421 | 370 | `ws/tests.rs` |

These are extractions, not deletion of the underlying scheduling/security logic.
Original test functions in all five files remain. At the end of that readability
pass, production Rust source under `crates/*/src` was at most 400 lines per file;
every Rust module there had a purpose
comment. Private names now identify normalization, refresh, cached-group loading,
saving and retry timing. The public wire types and parser signatures are unchanged.

Contributor docs cover all ten requested sections, plus a glossary, task index
and this evidence record. The roadmap keeps six understandable milestones and
links ready tasks. The original technical breakdown and upstream code are preserved.
Nx development starts Rust and Vite together; Lavender's Nx frontend/check commands
launch its pinned pnpm independently of the monorepo pin.

## Checks run successfully

| Verification | Result |
|---|---|
| Exact web package manager install with `npm exec --yes --package=pnpm@12.11.2 -- pnpm --dir apps/web install --frozen-lockfile` | Passed; no lockfile changes |
| `LAVENDER_CHECK_DESKTOP=1 npm exec --yes --package=pnpm@12.11.2 -- bash scripts/check.sh` | Passed: formatting, full-workspace clippy, Rust tests/build, dependency checks, contracts, TypeScript, frontend tests/lint/build |
| `CI=1 npm exec --yes --package=pnpm@10.22.0 -- pnpm exec nx run lavender:check` from monorepo root | Passed after fixing the pnpm-version handoff and formatting the changed frontend project JSON |
| Rust tests | Original 217 unit/integration tests pass; one new compiled Rustdoc example makes 218 total |
| Frontend tests | 42 files, 319 tests passed |
| `CI=1 PLAYWRIGHT_CHANNEL=chrome npm exec --yes --package=pnpm@12.11.2 -- pnpm --dir apps/web run e2e` | Four journeys passed on a fresh fixture server |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps` | Passed, with no Rustdoc warnings |
| `cargo build --locked -p lavender-desktop --features custom-protocol` | Passed on this macOS host |
| Generated TypeScript contract check | Up to date; no serialized-contract changes |
| Local Markdown file/directory targets, documented symbols, `git diff --check` | Checked and corrected; passed |
| Flokzy read-back | All 67 edited tasks retain comments, status, priority, ownership, dates, parent and dependency links before the explicit ENG-1012 completion update |

The host used Rust 1.99.0, Node 25.5.0, web pnpm 12.11.2 and monorepo pnpm 10.22.0.
Node 22.12 is the package's stated minimum, not a second runtime tested in this pass.
Existing cargo-deny duplicate-version warnings and the documented scoped advisory
ignore remain; its advisories, bans, licences and sources checks pass.

## Runtime checks

The monorepo development target started both processes using recorded feed fixtures.
`/healthz` answered on 7410, and `/api/v1/status` answered through Vite on 5410.
Ctrl+C stopped both processes. `CI=1` was used during this terminal verification
to suppress Nx's first-run interactive prompt.

The checked-in `config/development.example.toml` also started the server with the
documented `--config` argument. An aircraft request for bbox `20,58,30,62` returned
six records including `FIN7KV`, with complete coverage and a recorded-data notice.
Coordinate search returned `60.17,24.94` and honestly reported that place search is
not configured. The verification servers were stopped after the checks.

Fixture mode substitutes feed providers; catalog imagery can still contact its
allowed tile hosts. Browser tests exercise the production frontend and real
Rust parsers/hub/API/stream/security rather than a mocked frontend transport.

## Tracker and remaining work

### Browser documentation follow-up — ENG-1014

The developer guides now have a browser view at `/docs/`, using the existing
Lavender design system. It reads all 42 Markdown documents and supports guide
navigation, section links, full-text keyboard search, copied code, diagrams,
reference tables and source reading. Its separate entry stays below the same
250 KiB compressed initial JavaScript budget as the workspace.

The follow-up passed TypeScript, lint, 325 frontend tests in 43 files and seven
production browser journeys. The three new docs journeys cover real static
serving/CSP, console errors, section reload/back, source/Markdown reading, exact
copied text, keyboard search, missing-page recovery, mobile focus restoration and
320/360 px reflow. Desktop and mobile screenshots are attached to the browser
test report. These web checks do not claim a new native desktop GUI run.

Vite's allowed file scope now includes the Lavender repository so lazy Markdown
and Rust-source imports survive hot reload. The browser remains connected to a
local docs preview on 5410; no hosted documentation was published. Implementation
notes are in [Documentation design](../design/documentation.md).

### Engineering task snapshot

Ninety existing Lavender tasks were inspected. The 67 open titles/descriptions were
refined in place into Why, What to do, Where to work, Dependencies, Done when,
How to test and Technical notes. Existing acceptance requirements and discussion
were retained. Flokzy's description field preserves rich-text HTML rather than
Markdown layout, so the seven sections use its supported headings and lists;
all 67 were read back to confirm that formatting. Partial implementations now
identify their remaining scope;
workstream/wave tickets are explicitly tracking umbrellas. No duplicate project,
task or sprint was created.

The completion note for ENG-1012 records this pass. No unrelated provider,
desktop or release task was closed. See the [Task Index](task-index.md) and
[current roadmap](../roadmap.md) for the dated states and prerequisites.

At that earlier snapshot, still open: optional aircraft sources/history/enrichment,
satellite passes, place geocoding, remaining source/workflow migrations, measured performance,
keychain, Windows/Linux, packaging, hosted CI integration and release permissions.
The native app built here, but its GUI rendering was not rerun: the
[successful macOS self-check](../architecture/evidence/2026-10-12-desktop-self-check.json)
is earlier evidence. No legacy-wide test run, cross-platform build, production
deployment or commercial sign-off is claimed.

## Autonomous feature execution — 2026-10-13–14

Three coding agents implemented separate aircraft/history, place-search and
workspace-share scopes, then extended satellite passes and conditional caching.
General, Rust and TypeScript reviews found and resolved delayed-response history
ordering, same-text place retries, unavailable restored selections, narrow pass
window edges, camera-subpoint versus map-centre observers, and redirect validator
identity. Each correction has a focused regression.

| Final verification | Result |
|---|---|
| `npm exec --yes --package=pnpm@12.11.2 -- bash scripts/check.sh` | Passed after the last Rust fix: formatting, locked workspace Clippy/tests/server build, cargo-deny, contracts, TypeScript, frontend tests, Biome and production build. |
| Rust tests, excluding desktop | 293 unit/integration tests plus one compiled Rustdoc example passed. Includes 11 conditional-cache integration cases. |
| Frontend tests | 47 files, 382 tests passed. |
| `cargo clippy --locked -p lavender-desktop --all-targets -- -D warnings` | Passed on macOS with the new history/pass commands and permissions. |
| `CI=1 PLAYWRIGHT_CHANNEL=chrome npm exec --yes --package=pnpm@12.11.2 -- pnpm --dir apps/web run e2e` | 11 production Chrome journeys passed: docs, rendered globe, aircraft history, satellite prediction and exact share/back/forward restoration; CSP/console checks passed and screenshots were inspected. |
| Generated TS and both platform adapter contracts | Passed; domain-generated aircraft history, satellite pass and explicit place-search contracts are integrated. |
| `git diff --check` | Passed; unrelated pre-existing changes remain preserved. |

The build budget check passed again after documentation synchronization for both
entries (workspace 200.9 KiB, docs 207.4 KiB compressed initial JavaScript).
Existing cargo-deny
duplicate-version warnings remain; its checks pass.

Satellite reference fixtures cover independent satellite.js ISS TEME/look angles,
frozen legacy observer passes and the published Vallado example. Computation is
bounded and geometric: illumination, atmospheric refraction and naked-eye
visibility are not predictions supplied by this API. Track requests include their
starting point in the 1,440-point cap. Cache tests simulate disk expiry/restart,
304, quota failures, malformed metadata and redirects without live CelesTrak calls.

Geocoder checks use recorded Nominatim/Photon responses and mock HTTP. No public
instance is configured by default or used for autocomplete; place lookup requires
explicit submission. Aircraft fixtures use a documented replay clock; live
observation history requires reliable upstream timestamps and never initiates
upstream requests when read.

Flokzy confirms ENG-949 and ENG-972 Done and 27/91 project tasks complete.
ENG-948 remains In Progress for optional licensed sources/enrichment and a live
pacing soak; ENG-952 remains In Progress for vessel search and production service
decisions. ENG-957 and ENG-989 have completed recorded prerequisites. ENG-976 is
still In Review for renderer/performance scope. Release permissions (ENG-938),
native deep-link registration, fresh native GUI/cross-platform runs and measured
performance remain separate work. The earlier docs preview and legacy reference
were preserved; all changes remain local and uncommitted.
