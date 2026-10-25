# Lavender kanban — orchestrator board

Working board for the orchestrated continuation. One line per card, moved by
hand as waves complete; Flokzy (`task-index.md`) remains the system of record
for the ENG-\* tasks this board references. Update together with
[PROGRESS](../PROGRESS.md). Card numbering: LB-1xx (this board), defined in
[upstream-delta-2026-10.md](upstream-delta-2026-10.md) and
[language-fit.md](language-fit.md).

## Done

- **LB-110 Borders, divisions & region drawing** (commit 12129766):
  geofence draw tool (polygon/box, naming, persistence, share links),
  per-class boundary styling, honest division drill-down, zoom-aware
  class defaults. Follow-up: admin-1/admin-2 need a richer source
  (upstream `states_provinces.json`/`us_census_counties` are in legacy).
- **LB-111/112 Simulation stack** (commit f2c49257): motion predictor,
  deterministic synthetic aircraft (Aircraft contract), 2-DOF spherical
  ascent integrator with insertion physics + Ascent* TS contract. Hub/UI
  wiring behind a config flag is the next slice.
- **LB-113 Mission boards** (commit 387f01f7): analyst kanban, entity
  links, device persistence, validation.
- **LB-133 NoirSystems token alignment** (commit 06c74058).
- **LB-100 Upstream import.** 1095 commits merged under `legacy/` (`30047df2`);
  `HEAD..upstream/main` empty; legacy byte-identical to `9d3548c2`.
- **LB-101 WIP checkpoint.** Second provider wave + polyglot kernels committed
  (`0eaebbb3`) after fixing contract drift (Freshness union, bigint CCTV
  timestamps, `Located | undefined` + `symbolPx`); web 772/772, `cargo check`
  clean.
- **LB-102 Boundary data relocation.** Derived `boundaries.json` moved to
  `crates/lavender-core/data/natural_earth/` so `legacy/` stays pristine.

## In Progress

- **LB-150 Timeline replay (web)** — READY-TO-RUN brief: new
  features/replay panel ('replay' SidePanel + nav item), loads the
  selected entity's track via the existing platform aircraft/vessel
  track APIs, scrubber (play/pause, 1×/2×/4×, time readout), camera
  follow via globe.flyTo at modest step frequency; render the track by
  mirroring how VesselTrack draws today (do NOT edit GlobeEngine.ts);
  harness tests per features/missions idioms; typecheck+test+lint green.
  Two agent attempts culled by the provider concurrency cap before
  writing code.
- **LB-151 Simulation hub wiring + admin divisions (Rust)** —
  READY-TO-RUN brief: register AircraftSimulator (core/src/simulation.rs)
  as an aircraft-feed source behind config `simulation.enabled` (default
  OFF, seed+count, 1-4 s tick, source 'simulation', offline);
  flag-gated ascent enrichment via geo::simulate_ascent at the launches
  seam; copy natural_earth states_provinces + us_census_counties into
  crates/lavender-core/data/ and serve admin-1/-2 from regions with
  Natural Earth/US Census attribution; regenerate contracts; fmt +
  clippy -D warnings + workspace tests + check-contracts green. Same
  cull.
- **LB-130 Zig kernels + WASM provenance** (wrappers + script done): the Zig
  kernels already exported point-in-polygon (batch/multi-polygon/wrapped) and
  heatmap binning; `wasmHeatmap.ts` adds the TS wrapper with a pure-TS
  fallback, `scripts/build-zig-kernels.sh` pins the toolchain and rebuilds
  the artifact. BLOCKED: this 0.16-dev toolchain cannot link on macOS —
  bump the pin and rerun (see language-fit.md provenance note).
- **LB-140 Gotham-inspired build list** (from `docs/research/gotham-feature-set.md`
  top-10): geo-search (1), geofence alerting + inbox (2), timeline replay (3),
  provenance view (4), saved searches (5), entity resolution (6), briefing
  export (7), heatmaps (8 — pairs with LB-130), monitoring missions (9 —
  extends LB-113), graph view (10). Pick one per review.

## Backlog (ranked)

- **LB-109** Per-provider verification of the landed second wave (fixtures,
  licence lines, renderer journeys) — gates ENG-990/991 completion.
- **LB-114** airQuality provider · **LB-115** GNSS integrity · **LB-116**
  streetLevel remainder (Mapillary/KartaView) · **LB-117** SDR/local receivers
  via Zig native artifact · **LB-118** scenes/evidence tours · **LB-119** maps
  catalog/credits · **LB-120** i18n decision · **LB-121** query packs over
  DuckDB · **LB-122** AI enrichment (owner gate) · **LB-123** awareness layer.
- **LB-130** Zig WASM rebuild-provenance script + CI · **LB-131** vision
  sidecar lifecycle in lavender-server · **LB-132** Nim CEP retire-or-wire
  decision.
- Release track (Flokzy-owned): ENG-996–999 desktop, ENG-1000–1004
  measurement/CI, ENG-1005–1009 guides/deploy/licence/legacy-retirement.

## In Review

- (none — ENG-974/976/983/1003 remain in review in Flokzy from before this
  board; see `task-index.md`.)
